//! The answer engine (PRD §5): one streamed generation per question, in Rust (the Claude key
//! never reaches the webview).
//!
//! It reacts to pipeline events, like the overlay does: `transcript.final` feeds the bounded
//! context, `question.detected` starts an answer, `question.revised` restarts an answer that
//! is still being written. Every generation has its own request id, so a retry or a restart
//! supersedes the previous one for that question only, and late tokens of an aborted request
//! are rejected by the card lifecycle as stale.
//!
//! - **Early start.** When an utterance goes to the classifier, `speculate` starts its answer
//!   at once and holds the output. If the utterance becomes a new question, the held text is
//!   replayed into the card and the generation goes on live, which saves the classification
//!   time (~1 s). Otherwise it is aborted, never shown.
//! - **Rolling summary.** Lines that leave the context are folded into a short summary
//!   (`summary.rs`), sent with every answer.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::runtime::Handle;
use tokio::task::AbortHandle;

use super::prompt::{AnswerInput, request_body};
use super::sections::Splitter;
use super::summary;
use crate::audio::engine::Emit;
use crate::events::{AnswerSection, PipelineEvent};
use crate::intent::classifier::{ContextLine, Speaker};
use crate::intent::engine::Escalated;
use crate::intent::rules::{self, Verdict};
use crate::intent::text;
use crate::intent::types::{DetectedQuestion, Intent};
use crate::llm::{ClaudeClient, ClaudeModel, LlmError, LlmErrorCode};
use crate::profile::ProfileStore;

const TIMEOUT: Duration = Duration::from_secs(60);
const CONTEXT_LINES: usize = 12;
const REMEMBERED_QUESTIONS: usize = 50;
/// Early answers waiting for their decision; the oldest is dropped beyond this.
const SPECULATIONS: usize = 4;
/// An early answer whose utterance got no decision by then is dropped.
const SPECULATION_TTL: Duration = Duration::from_secs(20);
/// A failed summary is tried again after this delay, with the lines it could not fold.
const SUMMARY_RETRY: Duration = Duration::from_secs(60);
pub const MAX_MANUAL_CHARS: usize = 2_000;

pub type KeySource = Box<dyn Fn() -> Result<Option<String>, LlmError> + Send + Sync>;

struct Remembered {
    question: DetectedQuestion,
    /// The main answer once completed, for follow-ups.
    answer: Option<String>,
}

struct InFlight {
    request_id: String,
    abort: AbortHandle,
}

/// An answer started before its utterance was classified.
struct Speculation {
    segment_ids: Vec<String>,
    route: Arc<Route>,
    abort: AbortHandle,
    started: Instant,
}

struct State {
    model: ClaudeModel,
    early_start: bool,
    client: Option<(String, Arc<ClaudeClient>)>,
    in_flight: HashMap<String, InFlight>,
    speculations: Vec<Speculation>,
    questions: VecDeque<Remembered>,
    transcript: VecDeque<ContextLine>,
    summary: Option<String>,
    /// Lines that left `transcript` and are not in `summary` yet.
    unsummarized: Vec<ContextLine>,
    summarizing: bool,
    summary_retry_at: Option<Instant>,
    /// Incremented when the history is cleared, so a late summary of it is dropped.
    conversation: u64,
    next_request: u64,
    next_manual: u64,
}

pub struct AnswerEngine {
    emit: Emit,
    runtime: Handle,
    profile: Arc<ProfileStore>,
    key: KeySource,
    endpoint: Option<String>,
    state: Mutex<State>,
}

/// What the overlay shows on a failed card (French UI).
pub fn failure_message(code: LlmErrorCode) -> &'static str {
    match code {
        LlmErrorCode::MissingKey => "Pas de clé Claude : ajoute-la dans Réglages › Audio.",
        LlmErrorCode::Unauthorized => "Clé Claude refusée.",
        LlmErrorCode::Quota => "Crédit Claude épuisé.",
        LlmErrorCode::RateLimited => "Trop de requêtes vers Claude, réessaie dans un instant.",
        LlmErrorCode::Overloaded => "Claude est surchargé, réessaie.",
        LlmErrorCode::Network => "Connexion à Claude perdue.",
        LlmErrorCode::Timeout => "Claude n'a pas répondu à temps.",
        LlmErrorCode::InvalidResponse => "Réponse coupée ou inattendue.",
        LlmErrorCode::Refused => "Claude a refusé de répondre à cette question.",
        LlmErrorCode::Protocol => "Requête refusée par Claude (mise à jour nécessaire).",
        LlmErrorCode::CredentialStore => "Gestionnaire d'identification Windows indisponible.",
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn ended_event(
    question_id: &str,
    request_id: &str,
    result: &Result<(), LlmError>,
) -> PipelineEvent {
    match result {
        Ok(()) => PipelineEvent::AnswerCompleted {
            question_id: question_id.into(),
            request_id: request_id.into(),
        },
        Err(error) => PipelineEvent::AnswerFailed {
            question_id: question_id.into(),
            request_id: request_id.into(),
            message: failure_message(error.code).into(),
            retryable: !matches!(error.code, LlmErrorCode::Refused | LlmErrorCode::Protocol),
        },
    }
}

/// Where a generation's output goes: to its card, or held until its utterance is confirmed
/// as a question. Events are emitted under the lock, so a replay and the live stream that
/// follows it stay in order.
#[derive(Default)]
struct Route(Mutex<RouteState>);

#[derive(Default)]
struct RouteState {
    /// `(question id, request id)` once the generation writes into a card.
    card: Option<(String, String)>,
    seq: u64,
    held: Vec<(AnswerSection, String)>,
    primary: String,
    end: Option<Result<(), LlmError>>,
}

/// A generation that ended in a card.
struct Ended {
    question_id: String,
    request_id: String,
    /// The main answer, when the generation succeeded.
    answer: Option<String>,
}

enum Attach {
    /// Attached; the rest streams live.
    Live,
    /// Attached and replayed in full.
    Ended(Option<String>),
    /// The early answer failed: answer the question from scratch.
    Failed,
}

impl RouteState {
    fn send(&mut self, emit: &Emit, section: AnswerSection, delta: String) {
        let Some((question_id, request_id)) = &self.card else {
            self.held.push((section, delta));
            return;
        };
        emit(PipelineEvent::AnswerDelta {
            question_id: question_id.clone(),
            request_id: request_id.clone(),
            seq: self.seq,
            section,
            delta,
        });
        self.seq += 1;
    }

    fn ended(&self, emit: &Emit) -> Option<Ended> {
        let ((question_id, request_id), end) = (self.card.as_ref()?, self.end.as_ref()?);
        emit(ended_event(question_id, request_id, end));
        Some(Ended {
            question_id: question_id.clone(),
            request_id: request_id.clone(),
            answer: end.is_ok().then(|| self.primary.trim().to_string()),
        })
    }
}

impl Route {
    fn live(question_id: &str, request_id: &str) -> Self {
        Self(Mutex::new(RouteState {
            card: Some((question_id.into(), request_id.into())),
            ..RouteState::default()
        }))
    }

    fn started(&self, emit: &Emit) {
        if let Ok(state) = self.0.lock()
            && let Some((question_id, request_id)) = &state.card
        {
            emit(PipelineEvent::AnswerStarted {
                question_id: question_id.clone(),
                request_id: request_id.clone(),
            });
        }
    }

    fn push(&self, emit: &Emit, parts: Vec<(AnswerSection, String)>) {
        let Ok(mut state) = self.0.lock() else {
            return;
        };
        for (section, delta) in parts {
            if section == AnswerSection::Primary {
                state.primary.push_str(&delta);
            }
            state.send(emit, section, delta);
        }
    }

    /// Records the outcome; returns it when the generation writes into a card.
    fn end(&self, emit: &Emit, result: Result<(), LlmError>) -> Option<Ended> {
        let mut state = self.0.lock().ok()?;
        state.end = Some(result);
        state.ended(emit)
    }

    /// Attaches a held generation to its card: replays what it wrote, then goes on live.
    fn attach(&self, emit: &Emit, question_id: &str, request_id: &str) -> Attach {
        let Ok(mut state) = self.0.lock() else {
            return Attach::Failed;
        };
        if matches!(state.end, Some(Err(_))) {
            return Attach::Failed;
        }
        state.card = Some((question_id.into(), request_id.into()));
        emit(PipelineEvent::AnswerStarted {
            question_id: question_id.into(),
            request_id: request_id.into(),
        });
        for (section, delta) in std::mem::take(&mut state.held) {
            state.send(emit, section, delta);
        }
        match state.ended(emit) {
            Some(ended) => Attach::Ended(ended.answer),
            None => Attach::Live,
        }
    }
}

impl AnswerEngine {
    pub fn new(
        emit: Emit,
        runtime: Handle,
        profile: Arc<ProfileStore>,
        key: KeySource,
        endpoint: Option<String>,
    ) -> Self {
        Self {
            emit,
            runtime,
            profile,
            key,
            endpoint,
            state: Mutex::new(State {
                model: ClaudeModel::Sonnet55,
                early_start: true,
                client: None,
                in_flight: HashMap::new(),
                speculations: Vec::new(),
                questions: VecDeque::new(),
                transcript: VecDeque::new(),
                summary: None,
                unsummarized: Vec::new(),
                summarizing: false,
                summary_retry_at: None,
                conversation: 0,
                next_request: 0,
                next_manual: 0,
            }),
        }
    }

    /// `early_start`: start answers while the classifier decides (faster, and it spends a
    /// little on utterances that turn out not to be questions).
    pub fn configure(&self, model: ClaudeModel, early_start: bool) {
        if let Ok(mut state) = self.state.lock() {
            state.model = model;
            state.early_start = early_start;
            if !early_start {
                for s in state.speculations.drain(..) {
                    s.abort.abort();
                }
            }
        }
    }

    /// Reacts to a pipeline event that was just sent to the overlay.
    pub fn observe(self: &Arc<Self>, event: &PipelineEvent) {
        match event {
            PipelineEvent::TranscriptFinal { segment } if !segment.text.trim().is_empty() => {
                let speaker = if segment.source_id == "microphone" {
                    Speaker::Candidate
                } else {
                    Speaker::Interviewer
                };
                if let Ok(mut state) = self.state.lock() {
                    state.transcript.push_back(ContextLine {
                        speaker,
                        text: segment.text.clone(),
                    });
                    while state.transcript.len() > CONTEXT_LINES {
                        if let Some(line) = state.transcript.pop_front() {
                            state.unsummarized.push(line);
                        }
                    }
                    let excess = state
                        .unsummarized
                        .len()
                        .saturating_sub(summary::MAX_PENDING);
                    state.unsummarized.drain(..excess);
                    self.maybe_summarize(&mut state);
                }
            }
            PipelineEvent::QuestionDetected { question } if question.source == "speech" => {
                self.remember(question.clone());
                if !self.adopt(question) {
                    self.start(&question.id);
                }
            }
            PipelineEvent::QuestionRevised { question_id, text } => {
                let restart = self.state.lock().is_ok_and(|mut state| {
                    if let Some(r) = state
                        .questions
                        .iter_mut()
                        .find(|r| r.question.id == *question_id)
                    {
                        r.question.normalized_question = text.clone();
                    }
                    state.in_flight.contains_key(question_id)
                });
                // A completed answer is immutable (the card records the revision instead).
                if restart {
                    self.start(question_id);
                }
            }
            // Every escalated utterance ends with a report; an early answer that was not
            // adopted by then is not wanted.
            PipelineEvent::UtteranceClassified { utterance } => {
                if let Ok(mut state) = self.state.lock() {
                    state.speculations.retain(|s| {
                        let keep = s.segment_ids != utterance.segment_ids;
                        if !keep {
                            s.abort.abort();
                        }
                        keep
                    });
                }
            }
            _ => {}
        }
    }

    /// An utterance went to the classifier: start its answer now, held until the decision.
    pub fn speculate(self: &Arc<Self>, utterance: &Escalated) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if !state.early_start {
            return;
        }
        state.speculations.retain(|s| {
            let keep = s.started.elapsed() < SPECULATION_TTL;
            if !keep {
                s.abort.abort();
            }
            keep
        });
        while state.speculations.len() >= SPECULATIONS {
            state.speculations.remove(0).abort.abort();
        }
        // Without a usable key, the question's own start reports the error on its card.
        let Ok(client) = self.client(&mut state) else {
            return;
        };
        let input = AnswerInput {
            question: utterance.text.clone(),
            as_heard: utterance.text.clone(),
            intent: None,
            language: None,
            summary: state.summary.clone(),
            transcript: state.transcript.iter().cloned().collect(),
            earlier: None,
        };
        let body = request_body(state.model, &input, self.profile.get().as_ref());
        let route = Arc::new(Route::default());
        let abort = self.spawn_generation(Ok(client), body, state.model.beta(), route.clone());
        state.speculations.push(Speculation {
            segment_ids: utterance.segment_ids.clone(),
            route,
            abort,
            started: Instant::now(),
        });
    }

    /// A question typed in the overlay. Emits `question.detected`, then answers it.
    pub fn ask_manual(self: &Arc<Self>, text: &str) -> Result<String, LlmError> {
        let text = text.trim();
        if text.is_empty() || text.chars().count() > MAX_MANUAL_CHARS {
            return Err(LlmError::new(
                LlmErrorCode::Protocol,
                format!("a question must have 1 to {MAX_MANUAL_CHARS} characters"),
            ));
        }
        let (n, latest) = {
            let mut state = self.lock()?;
            state.next_manual += 1;
            let latest = state.questions.back().map(|r| r.question.id.clone());
            (state.next_manual, latest)
        };
        let intent = match rules::evaluate(text, latest.is_some()) {
            Verdict::Greeting => Intent::GreetingOrSmallTalk,
            Verdict::FollowUp => Intent::ClarificationOrFollowUp,
            Verdict::Escalate(hints) if hints.intent != Intent::NonQuestionStatement => {
                hints.intent
            }
            _ => Intent::TechnicalQuestion,
        };
        let question = DetectedQuestion {
            id: format!("manual-{}-{n}", now_ms()),
            session_id: "manual".into(),
            source: "manual",
            source_segment_ids: Vec::new(),
            original_utterance: text.to_string(),
            normalized_question: text.to_string(),
            intent,
            language: text::guess_language(&text::tokens(text)),
            domain: None,
            context_ref: if intent == Intent::ClarificationOrFollowUp {
                latest
            } else {
                None
            },
            detected_at: now_ms(),
        };
        let id = question.id.clone();
        (self.emit)(PipelineEvent::QuestionDetected {
            question: question.clone(),
        });
        self.remember(question);
        self.start(&id);
        Ok(id)
    }

    /// Generates again (a failed card's retry button). Unknown ids are ignored.
    pub fn retry(self: &Arc<Self>, question_id: &str) {
        let known = self
            .state
            .lock()
            .is_ok_and(|s| s.questions.iter().any(|r| r.question.id == question_id));
        if known {
            self.start(question_id);
        }
    }

    /// Stops every generation and forgets the conversation (the history was cleared).
    pub fn cancel_all(&self) {
        if let Ok(mut state) = self.state.lock() {
            for (_, flight) in state.in_flight.drain() {
                flight.abort.abort();
            }
            for s in state.speculations.drain(..) {
                s.abort.abort();
            }
            state.questions.clear();
            state.transcript.clear();
            state.summary = None;
            state.unsummarized.clear();
            state.summarizing = false;
            state.summary_retry_at = None;
            state.conversation += 1;
        }
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, State>, LlmError> {
        self.state
            .lock()
            .map_err(|_| LlmError::new(LlmErrorCode::Protocol, "answer engine state poisoned"))
    }

    fn remember(&self, question: DetectedQuestion) {
        if let Ok(mut state) = self.state.lock() {
            state.questions.retain(|r| r.question.id != question.id);
            state.questions.push_back(Remembered {
                question,
                answer: None,
            });
            while state.questions.len() > REMEMBERED_QUESTIONS {
                state.questions.pop_front();
            }
        }
    }

    fn client(&self, state: &mut State) -> Result<Arc<ClaudeClient>, LlmError> {
        let key = (self.key)()?
            .ok_or_else(|| LlmError::new(LlmErrorCode::MissingKey, "no Claude API key stored"))?;
        if let Some((cached, client)) = &state.client
            && *cached == key
        {
            return Ok(client.clone());
        }
        let client = Arc::new(ClaudeClient::new(
            key.clone(),
            TIMEOUT,
            self.endpoint.clone(),
        )?);
        state.client = Some((key, client.clone()));
        Ok(client)
    }

    /// Gives a new spoken question the early answer of its utterance. Returns false when the
    /// question must be answered from scratch.
    fn adopt(&self, question: &DetectedQuestion) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        let Some(i) = state
            .speculations
            .iter()
            .position(|s| s.segment_ids == question.source_segment_ids)
        else {
            return false;
        };
        let speculation = state.speculations.remove(i);
        // A follow-up needs the earlier exchange, which the early answer did not have.
        if question.context_ref.is_some() {
            speculation.abort.abort();
            return false;
        }
        state.next_request += 1;
        let request_id = format!("{}-a{}", question.id, state.next_request);
        match speculation
            .route
            .attach(&self.emit, &question.id, &request_id)
        {
            Attach::Live => {
                state.in_flight.insert(
                    question.id.clone(),
                    InFlight {
                        request_id,
                        abort: speculation.abort,
                    },
                );
                true
            }
            Attach::Ended(answer) => {
                if let Some(r) = state
                    .questions
                    .iter_mut()
                    .find(|r| r.question.id == question.id)
                {
                    r.answer = answer;
                }
                true
            }
            Attach::Failed => false,
        }
    }

    fn start(self: &Arc<Self>, question_id: &str) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let Some(remembered) = state
            .questions
            .iter()
            .find(|r| r.question.id == question_id)
        else {
            return;
        };
        let q = &remembered.question;
        let earlier = q.context_ref.as_ref().and_then(|id| {
            state
                .questions
                .iter()
                .find(|r| r.question.id == *id)
                .map(|r| {
                    (
                        r.question.normalized_question.clone(),
                        r.answer.clone().unwrap_or_default(),
                    )
                })
        });
        let input = AnswerInput {
            question: q.normalized_question.clone(),
            as_heard: q.original_utterance.clone(),
            intent: Some(q.intent),
            language: Some(q.language),
            summary: state.summary.clone(),
            transcript: state.transcript.iter().cloned().collect(),
            earlier,
        };
        state.next_request += 1;
        let request_id = format!("{question_id}-a{}", state.next_request);
        if let Some(previous) = state.in_flight.remove(question_id) {
            previous.abort.abort();
        }
        let model = state.model;
        let client = self.client(&mut state);
        let body = request_body(model, &input, self.profile.get().as_ref());
        let route = Arc::new(Route::live(question_id, &request_id));
        let abort = self.spawn_generation(client, body, model.beta(), route);
        state
            .in_flight
            .insert(question_id.to_string(), InFlight { request_id, abort });
    }

    fn spawn_generation(
        self: &Arc<Self>,
        client: Result<Arc<ClaudeClient>, LlmError>,
        body: serde_json::Value,
        beta: Option<&'static str>,
        route: Arc<Route>,
    ) -> AbortHandle {
        let engine = self.clone();
        self.runtime
            .spawn(async move {
                if let Some(ended) = engine.generate(client, body, beta, &route).await {
                    engine.finished(ended);
                }
            })
            .abort_handle()
    }

    async fn generate(
        &self,
        client: Result<Arc<ClaudeClient>, LlmError>,
        body: serde_json::Value,
        beta: Option<&'static str>,
        route: &Route,
    ) -> Option<Ended> {
        let emit = &self.emit;
        route.started(emit);
        let result = match client {
            Ok(client) => {
                let mut splitter = Splitter::default();
                let result = client
                    .stream(&body, beta, &mut |piece, _, _| {
                        route.push(emit, splitter.push(piece))
                    })
                    .await;
                route.push(emit, splitter.finish());
                result.map(|_| ())
            }
            Err(error) => Err(error),
        };
        route.end(emit, result)
    }

    fn finished(&self, ended: Ended) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if let Some(answer) = ended.answer
            && let Some(r) = state
                .questions
                .iter_mut()
                .find(|r| r.question.id == ended.question_id)
        {
            r.answer = Some(answer);
        }
        if state
            .in_flight
            .get(&ended.question_id)
            .is_some_and(|f| f.request_id == ended.request_id)
        {
            state.in_flight.remove(&ended.question_id);
        }
    }

    /// Folds the lines that left the context into the summary, one batch at a time.
    fn maybe_summarize(self: &Arc<Self>, state: &mut State) {
        if state.summarizing
            || state.unsummarized.len() < summary::BATCH
            || state.summary_retry_at.is_some_and(|at| Instant::now() < at)
        {
            return;
        }
        let Ok(client) = self.client(state) else {
            return;
        };
        let lines = std::mem::take(&mut state.unsummarized);
        let body = summary::request_body(state.summary.as_deref(), &lines);
        let conversation = state.conversation;
        state.summarizing = true;
        let engine = self.clone();
        self.runtime.spawn(async move {
            let result = client
                .stream(&body, summary::MODEL.beta(), &mut |_, _, _| {})
                .await;
            let Ok(mut state) = engine.state.lock() else {
                return;
            };
            if state.conversation != conversation {
                return;
            }
            state.summarizing = false;
            match result {
                Ok(streamed) if !streamed.text.trim().is_empty() => {
                    state.summary = Some(summary::clip(&streamed.text));
                    state.summary_retry_at = None;
                }
                _ => {
                    let mut lines = lines;
                    lines.append(&mut state.unsummarized);
                    let excess = lines.len().saturating_sub(summary::MAX_PENDING);
                    lines.drain(..excess);
                    state.unsummarized = lines;
                    state.summary_retry_at = Some(Instant::now() + SUMMARY_RETRY);
                }
            }
        });
    }

    #[cfg(test)]
    fn summary(&self) -> Option<String> {
        self.state.lock().ok()?.summary.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::test_support::{fake_api, fake_api_paused, streamed_text};
    use crate::profile::Profile;
    use crate::stt::types::TranscriptSegment;

    fn recorder() -> (Emit, Arc<Mutex<Vec<PipelineEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        (Arc::new(move |e| sink.lock().unwrap().push(e)), events)
    }

    fn engine(
        endpoint: Option<String>,
        key: Option<&'static str>,
    ) -> (Arc<AnswerEngine>, Arc<Mutex<Vec<PipelineEvent>>>) {
        let (emit, events) = recorder();
        let profile = Arc::new(ProfileStore::open(None));
        let engine = AnswerEngine::new(
            emit,
            Handle::current(),
            profile,
            Box::new(move || Ok(key.map(str::to_string))),
            endpoint,
        );
        (Arc::new(engine), events)
    }

    fn question(id: &str, text: &str, context_ref: Option<&str>) -> DetectedQuestion {
        DetectedQuestion {
            id: id.into(),
            session_id: "s1".into(),
            source: "speech",
            source_segment_ids: vec![],
            original_utterance: text.into(),
            normalized_question: text.into(),
            intent: Intent::TechnicalQuestion,
            language: crate::intent::types::Language::Fr,
            domain: None,
            context_ref: context_ref.map(str::to_string),
            detected_at: 0,
        }
    }

    fn final_line(source: &str, text: &str) -> PipelineEvent {
        PipelineEvent::TranscriptFinal {
            segment: TranscriptSegment {
                id: format!("{source}-1-0"),
                session_id: "s1".into(),
                source_id: source.into(),
                start_ms: 0,
                end_ms: 0,
                text: text.into(),
                normalized_text: None,
                is_final: true,
                confidence: None,
                speaker_id: None,
            },
        }
    }

    /// Waits until `count` answers have ended (completed or failed), 5 s at most.
    async fn ended(events: &Arc<Mutex<Vec<PipelineEvent>>>, count: usize) {
        for _ in 0..500 {
            let done = events
                .lock()
                .unwrap()
                .iter()
                .filter(|e| {
                    matches!(
                        e,
                        PipelineEvent::AnswerCompleted { .. } | PipelineEvent::AnswerFailed { .. }
                    )
                })
                .count();
            if done >= count {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("answers did not end: {:#?}", events.lock().unwrap());
    }

    fn summary(events: &[PipelineEvent]) -> Vec<String> {
        events
            .iter()
            .map(|e| match e {
                PipelineEvent::QuestionDetected { question } => format!("detected {}", question.id),
                PipelineEvent::AnswerStarted { request_id, .. } => format!("started {request_id}"),
                PipelineEvent::AnswerDelta {
                    seq,
                    section,
                    delta,
                    ..
                } => {
                    format!("delta {seq} {section:?} {delta}")
                }
                PipelineEvent::AnswerCompleted { request_id, .. } => {
                    format!("completed {request_id}")
                }
                PipelineEvent::AnswerFailed {
                    message, retryable, ..
                } => {
                    format!("failed {message} {retryable}")
                }
                other => format!("{other:?}"),
            })
            .collect()
    }

    #[tokio::test]
    async fn a_detected_question_streams_its_answer_into_both_sections() {
        let answer = "J'utilise Pinia.\n<<<DETAILS>>>\n```ts\ndefineStore()\n```";
        let (url, seen) = fake_api(vec![(200, streamed_text(answer, 6))]).await;
        let (engine, events) = engine(Some(url), Some("k"));
        engine.configure(ClaudeModel::Haiku55, true);
        engine.observe(&final_line("system", "Tu gères le state comment ?"));
        engine.observe(&final_line("microphone", "Euh, bonne question."));
        engine.observe(&PipelineEvent::QuestionDetected {
            question: question("q1", "Comment gères-tu le state ?", None),
        });
        ended(&events, 1).await;

        let events = events.lock().unwrap();
        let lines = summary(&events);
        assert_eq!(lines.first().unwrap(), "started q1-a1");
        assert_eq!(lines.last().unwrap(), "completed q1-a1");
        let text = |section: AnswerSection| -> String {
            events
                .iter()
                .filter_map(|e| match e {
                    PipelineEvent::AnswerDelta {
                        section: s, delta, ..
                    } if *s == section => Some(delta.as_str()),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(text(AnswerSection::Primary), "J'utilise Pinia.");
        assert_eq!(text(AnswerSection::Details), "```ts\ndefineStore()\n```");
        // Sequence numbers are contiguous from 0 across both sections.
        let seqs: Vec<u64> = events
            .iter()
            .filter_map(|e| match e {
                PipelineEvent::AnswerDelta { seq, .. } => Some(*seq),
                _ => None,
            })
            .collect();
        assert_eq!(seqs, (0..seqs.len() as u64).collect::<Vec<_>>());

        let request = seen.lock().unwrap()[0].clone();
        assert!(request.contains("claude-haiku-5-5"));
        assert!(request.contains("[candidate] Euh, bonne question."));
        assert!(request.contains("No verified profile"));
    }

    #[tokio::test]
    async fn without_a_key_the_card_fails_with_a_retryable_message() {
        let (engine, events) = engine(None, None);
        engine.observe(&PipelineEvent::QuestionDetected {
            question: question("q1", "Tu connais Docker ?", None),
        });
        ended(&events, 1).await;
        assert_eq!(
            summary(&events.lock().unwrap()),
            [
                "started q1-a1",
                "failed Pas de clé Claude : ajoute-la dans Réglages › Audio. true"
            ]
        );
    }

    #[tokio::test]
    async fn follow_ups_see_the_earlier_answer_and_the_profile() {
        let (url, seen) = fake_api(vec![
            (200, streamed_text("J'utilise Pinia chez [entreprise].", 8)),
            (200, streamed_text("Parce qu'il est mieux typé.", 8)),
        ])
        .await;
        let (emit, events) = recorder();
        let store = Arc::new(ProfileStore::open(None));
        store
            .save(
                Profile {
                    name: "Alex Martin".into(),
                    skills: "Vue.js, Pinia".into(),
                    ..Profile::default()
                },
                1,
            )
            .unwrap();
        let engine = Arc::new(AnswerEngine::new(
            emit,
            Handle::current(),
            store,
            Box::new(|| Ok(Some("k".into()))),
            Some(url),
        ));
        engine.observe(&PipelineEvent::QuestionDetected {
            question: question("q1", "Quel state management ?", None),
        });
        ended(&events, 1).await;
        engine.observe(&PipelineEvent::QuestionDetected {
            question: question("q2", "Pourquoi ?", Some("q1")),
        });
        ended(&events, 2).await;
        let second = seen.lock().unwrap()[1].clone();
        assert!(second.contains("<earlier_question>Quel state management ?</earlier_question>"));
        assert!(second.contains("J'utilise Pinia chez [entreprise]."));
        assert!(second.contains("## Skills\\nVue.js, Pinia"));
    }

    #[tokio::test]
    async fn manual_questions_are_announced_then_answered_and_can_be_retried() {
        let busy = r#"{"type":"error","error":{"type":"invalid_request_error","message":"bad"}}"#;
        let (url, _seen) = fake_api(vec![
            (400, busy.into()),
            (200, streamed_text("Bonjour !", 4)),
        ])
        .await;
        let (engine, events) = engine(Some(url), Some("k"));
        let id = engine.ask_manual("  Bonjour, ça va ?  ").unwrap();
        ended(&events, 1).await;
        engine.retry(&id);
        ended(&events, 2).await;
        let lines = summary(&events.lock().unwrap());
        assert_eq!(lines[0], format!("detected {id}"));
        assert_eq!(lines[1], format!("started {id}-a1"));
        assert!(lines[2].starts_with("failed Requête refusée"), "{lines:?}");
        assert!(lines[2].ends_with("false"));
        assert_eq!(lines[3], format!("started {id}-a2"));
        assert_eq!(lines.last().unwrap(), &format!("completed {id}-a2"));
        match &events.lock().unwrap()[0] {
            PipelineEvent::QuestionDetected { question } => {
                assert_eq!(question.source, "manual");
                assert_eq!(question.intent, Intent::GreetingOrSmallTalk);
                assert_eq!(question.normalized_question, "Bonjour, ça va ?");
            }
            other => panic!("{other:?}"),
        }
        assert!(engine.ask_manual("   ").is_err());
    }

    #[tokio::test]
    async fn cancel_all_stops_generations_and_forgets_the_conversation() {
        let (url, _seen) = fake_api(vec![(200, streamed_text("x", 1))]).await;
        let (engine, events) = engine(Some(url), Some("k"));
        engine.observe(&PipelineEvent::QuestionDetected {
            question: question("q1", "Question ?", None),
        });
        engine.cancel_all();
        engine.retry("q1");
        tokio::time::sleep(Duration::from_millis(300)).await;
        let lines = summary(&events.lock().unwrap());
        assert!(
            !lines.iter().any(|l| l.starts_with("started q1-a2")),
            "{lines:?}"
        );
    }

    fn escalated(segment: &str, text: &str) -> Escalated {
        Escalated {
            segment_ids: vec![segment.into()],
            text: text.into(),
        }
    }

    fn spoken(id: &str, text: &str, segment: &str) -> DetectedQuestion {
        DetectedQuestion {
            source_segment_ids: vec![segment.into()],
            ..question(id, text, None)
        }
    }

    #[tokio::test]
    async fn an_early_answer_is_replayed_into_its_question_card_without_a_second_request() {
        let answer = "Oui, je l'utilise tous les jours.\n<<<DETAILS>>>\ndocker compose up";
        let (url, seen) = fake_api(vec![(200, streamed_text(answer, 5))]).await;
        let (engine, events) = engine(Some(url), Some("k"));
        engine.speculate(&escalated("system-1-0", "Tu connais Docker ?"));
        // Nothing is shown while the classifier decides, even once the answer is written.
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(events.lock().unwrap().is_empty());

        engine.observe(&PipelineEvent::QuestionDetected {
            question: spoken("q1", "Connais-tu Docker ?", "system-1-0"),
        });
        ended(&events, 1).await;
        let lines = summary(&events.lock().unwrap());
        assert_eq!(lines.first().unwrap(), "started q1-a1");
        assert_eq!(lines.last().unwrap(), "completed q1-a1");
        assert!(
            lines.contains(&"delta 0 Primary Oui,".to_string()),
            "{lines:?}"
        );
        assert_eq!(seen.lock().unwrap().len(), 1);
        let request = seen.lock().unwrap()[0].clone();
        // The early request leaves intent and language to the model.
        assert!(request.contains("<question>Tu connais Docker ?</question>"));
    }

    #[tokio::test]
    async fn an_early_answer_still_streaming_continues_live_in_its_card() {
        let answer = "Une closure capture les variables de sa portée de définition.";
        let (url, seen) = fake_api_paused(
            vec![(200, streamed_text(answer, 4))],
            Duration::from_millis(400),
        )
        .await;
        let (engine, events) = engine(Some(url), Some("k"));
        engine.speculate(&escalated("system-1-0", "C'est quoi une closure ?"));
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(events.lock().unwrap().is_empty());
        engine.observe(&PipelineEvent::QuestionDetected {
            question: spoken("q1", "Qu'est-ce qu'une closure ?", "system-1-0"),
        });
        // Replayed at once: the card is not waiting for the rest of the stream.
        assert_eq!(
            summary(&events.lock().unwrap()).first().map(String::as_str),
            Some("started q1-a1")
        );
        ended(&events, 1).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        // The adopted generation is tracked like any other, then released.
        assert!(engine.state.lock().unwrap().in_flight.is_empty());
        let events = events.lock().unwrap();
        let seqs: Vec<u64> = events
            .iter()
            .filter_map(|e| match e {
                PipelineEvent::AnswerDelta { seq, .. } => Some(*seq),
                _ => None,
            })
            .collect();
        assert_eq!(seqs, (0..seqs.len() as u64).collect::<Vec<_>>());
        let text: String = events
            .iter()
            .filter_map(|e| match e {
                PipelineEvent::AnswerDelta { delta, .. } => Some(delta.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(text, answer);
        assert_eq!(summary(&events).last().unwrap(), "completed q1-a1");
        assert_eq!(seen.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn an_early_answer_for_a_statement_is_never_shown() {
        let (url, seen) = fake_api(vec![(200, streamed_text("Super.", 3))]).await;
        let (engine, events) = engine(Some(url), Some("k"));
        engine.speculate(&escalated("system-1-0", "On utilise beaucoup Vue ici."));
        tokio::time::sleep(Duration::from_millis(300)).await;
        engine.observe(&PipelineEvent::UtteranceClassified {
            utterance: crate::intent::types::UtteranceReport {
                id: "s1-u1".into(),
                source_id: "system".into(),
                segment_ids: vec!["system-1-0".into()],
                text: "On utilise beaucoup Vue ici.".into(),
                decision: crate::intent::types::Decision::Ignored,
                reason: Some(crate::intent::types::IgnoreReason::Statement),
                intent: Some(Intent::NonQuestionStatement),
                stage: crate::intent::types::Stage::Llm,
                question_id: None,
                latency_ms: Some(900),
            },
        });
        // Same segments again: no early answer is left to adopt.
        assert!(!engine.adopt(&spoken("q1", "x", "system-1-0")));
        assert_eq!(seen.lock().unwrap().len(), 1);
        assert!(summary(&events.lock().unwrap()).is_empty());
    }

    #[tokio::test]
    async fn lines_leaving_the_context_are_summarized_for_later_answers() {
        let (url, seen) = fake_api(vec![
            (200, streamed_text("Présentation de l'équipe paiement.", 8)),
            (200, streamed_text("Réponse.", 4)),
        ])
        .await;
        let (engine, events) = engine(Some(url), Some("k"));
        for i in 1..=CONTEXT_LINES + summary::BATCH {
            engine.observe(&final_line("system", &format!("ligne {i}")));
        }
        for _ in 0..200 {
            if engine.summary().is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(
            engine.summary().as_deref(),
            Some("Présentation de l'équipe paiement.")
        );
        engine.observe(&PipelineEvent::QuestionDetected {
            question: question("q1", "Des questions ?", None),
        });
        ended(&events, 1).await;
        let seen = seen.lock().unwrap();
        assert!(seen[0].contains("claude-haiku-5-5"));
        assert!(seen[0].contains("[interviewer] ligne 1\\n"));
        assert!(!seen[0].contains("ligne 13"));
        assert!(seen[1].contains(
            "<interview_summary>\\nPrésentation de l'équipe paiement.\\n</interview_summary>"
        ));
        assert!(seen[1].contains("[interviewer] ligne 24"));
    }
}
