//! One transcription session per audio source. Three tasks cooperate:
//!
//! - **feeder**: receives 20 ms frames from the capture worker, pads silence to keep real-time
//!   pace, runs the local VAD (forcing an endpoint after the configured silence), and groups
//!   audio into 80 ms chunks;
//! - **supervisor**: runs provider connections, reconnecting with backoff (doubling, then at
//!   most every 5 s, for as long as a network outage lasts). Audio that queued up while
//!   disconnected is dropped, never replayed;
//! - **mapper**: turns provider events into `transcript.partial` / `transcript.final` events
//!   with stable ids (`{source}-{connection}-{turn}`), so a partial update never becomes a
//!   second segment, and reports `stt.status`.
//!
//! The session lives as long as its `SttSink`: when the capture worker drops the sink, the
//! feeder flushes, the provider ends the stream gracefully, and the tasks finish.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::runtime::Handle;
use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;

use super::pacing::{CHUNK_SAMPLES, Chunker, padding_needed};
use super::provider::{AudioMsg, EventSender, ProviderEvent, SttProvider};
use super::types::{SttError, SttState, SttStatus, TranscriptSegment};
use super::vad::{EnergyVad, VadEvent};
use crate::audio::convert::FRAME_SAMPLES;
use crate::audio::engine::Emit;
use crate::audio::types::SourceKind;
use crate::audio::worker::{FrameSink, RecoveryPolicy};
use crate::events::PipelineEvent;

/// 5 s of 20 ms frames between the capture thread and the feeder.
const FRAME_QUEUE: usize = 250;
/// ~5 s of 80 ms chunks between the feeder and the provider connection.
const AUDIO_QUEUE: usize = 64;
/// Silence that ends speech for latency timing when no forced endpoint is configured.
const DEFAULT_END_SILENCE_MS: u32 = 700;
const MAX_TEXT_CHARS: usize = 4_000;
/// A voice frame older than this is not used to compute time-to-final.
const LATENCY_HORIZON: Duration = Duration::from_secs(10);
/// Once the backoff budget is spent, a network outage is retried at this pace for as long as
/// it lasts: the session never gives up on a recoverable error.
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(5);

pub struct SessionOptions {
    pub source: SourceKind,
    pub session_id: String,
    pub force_endpoint_ms: Option<u32>,
    pub policy: RecoveryPolicy,
}

#[derive(Default)]
struct Shared {
    last_voice: Mutex<Option<Instant>>,
    dropped_chunks: AtomicU64,
}

/// Capture-side handle: a `FrameSink` that forwards frames without ever blocking the worker.
pub struct SttSink {
    frames: mpsc::Sender<Vec<i16>>,
    shared: Arc<Shared>,
}

impl FrameSink for SttSink {
    fn push(&mut self, _source: SourceKind, frame: &[i16], _index: u64) {
        if self.frames.try_send(frame.to_vec()).is_err() {
            self.shared.dropped_chunks.fetch_add(1, Ordering::Relaxed);
        }
    }
}

pub fn spawn(
    runtime: &Handle,
    provider: Arc<dyn SttProvider>,
    options: SessionOptions,
    emit: Emit,
) -> SttSink {
    let shared = Arc::new(Shared::default());
    let (frames_tx, frames_rx) = mpsc::channel(FRAME_QUEUE);
    let (audio_tx, audio_rx) = mpsc::channel(AUDIO_QUEUE);
    let (events_tx, events_rx) = mpsc::unbounded_channel();
    let (status_tx, status_rx) = mpsc::unbounded_channel();

    let feeder = Feeder {
        audio: audio_tx,
        chunker: Chunker::default(),
        vad: EnergyVad::new(options.force_endpoint_ms.unwrap_or(DEFAULT_END_SILENCE_MS)),
        sent_samples: 0,
        force_endpoint: options.force_endpoint_ms.is_some(),
        shared: shared.clone(),
    };
    runtime.spawn(feeder.run(frames_rx));
    runtime.spawn(supervise(
        provider.clone(),
        audio_rx,
        events_tx,
        status_tx,
        options.policy,
        shared.clone(),
    ));
    runtime.spawn(
        Mapper {
            source: options.source,
            session_id: options.session_id,
            status: SttStatus {
                source: options.source,
                provider: provider.kind(),
                state: SttState::Connecting,
                error: None,
                last_final_latency_ms: None,
                dropped_chunks: 0,
            },
            emit,
            shared: shared.clone(),
            connection: 0,
        }
        .run(events_rx, status_rx),
    );

    SttSink {
        frames: frames_tx,
        shared,
    }
}

struct Feeder {
    audio: mpsc::Sender<AudioMsg>,
    chunker: Chunker,
    vad: EnergyVad,
    sent_samples: u64,
    force_endpoint: bool,
    shared: Arc<Shared>,
}

impl Feeder {
    async fn run(mut self, mut frames: mpsc::Receiver<Vec<i16>>) {
        let started = Instant::now();
        let silence = [0i16; FRAME_SAMPLES];
        let mut ticker = tokio::time::interval(Duration::from_millis(20));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                frame = frames.recv() => match frame {
                    Some(frame) => self.process(&frame),
                    None => break,
                },
                _ = ticker.tick() => {
                    let mut missing = padding_needed(started.elapsed(), self.sent_samples);
                    while missing > 0 {
                        let n = missing.min(FRAME_SAMPLES as u64) as usize;
                        self.process(&silence[..n]);
                        missing -= n as u64;
                    }
                }
            }
        }
        self.flush_padded();
    }

    /// Sends the partial chunk padded with silence to a full 80 ms. Providers reject tiny
    /// messages (AssemblyAI: "Input Duration Violation: 10.0 ms. Expected between 20 and 1000
    /// ms"), and what follows detected end-of-speech is silence anyway.
    fn flush_padded(&mut self) {
        if let Some(mut rest) = self.chunker.flush() {
            rest.resize(CHUNK_SAMPLES * 2, 0);
            self.send(AudioMsg::Chunk(rest));
        }
    }

    fn process(&mut self, samples: &[i16]) {
        self.sent_samples += samples.len() as u64;
        let event = self.vad.push(samples);
        if self.vad.last_frame_was_speech()
            && let Ok(mut last) = self.shared.last_voice.lock()
        {
            *last = Some(Instant::now());
        }
        for chunk in self.chunker.push(samples) {
            self.send(AudioMsg::Chunk(chunk));
        }
        if self.force_endpoint && matches!(event, Some(VadEvent::SpeechEnd { .. })) {
            self.flush_padded();
            self.send(AudioMsg::ForceEndpoint);
        }
    }

    fn send(&self, message: AudioMsg) {
        if self.audio.try_send(message).is_err() {
            self.shared.dropped_chunks.fetch_add(1, Ordering::Relaxed);
        }
    }
}

type StatusUpdate = (SttState, Option<SttError>);

async fn supervise(
    provider: Arc<dyn SttProvider>,
    mut audio: mpsc::Receiver<AudioMsg>,
    events: EventSender,
    status: mpsc::UnboundedSender<StatusUpdate>,
    policy: RecoveryPolicy,
    shared: Arc<Shared>,
) {
    let mut attempt = 0;
    loop {
        let began = Instant::now();
        match provider.stream(&mut audio, &events).await {
            Ok(()) => {
                let _ = status.send((SttState::Closed, None));
                return;
            }
            Err(error) => {
                // A connection that streamed for a while earns a fresh retry budget.
                if began.elapsed() > Duration::from_secs(30) {
                    attempt = 0;
                }
                attempt += 1;
                let delay = if error.is_recoverable() {
                    policy
                        .delay(attempt.min(policy.max_attempts))
                        .map(|d| d.min(MAX_RECONNECT_DELAY))
                } else {
                    None
                };
                let Some(delay) = delay else {
                    let _ = status.send((SttState::Error, Some(error)));
                    return;
                };
                let _ = status.send((SttState::Reconnecting, Some(error)));
                tokio::time::sleep(delay).await;
                // Audio queued while disconnected is stale: drop it instead of replaying it.
                while audio.try_recv().is_ok() {
                    shared.dropped_chunks.fetch_add(1, Ordering::Relaxed);
                }
                if audio.is_closed() {
                    let _ = status.send((SttState::Closed, None));
                    return;
                }
            }
        }
    }
}

struct Mapper {
    source: SourceKind,
    session_id: String,
    status: SttStatus,
    emit: Emit,
    shared: Arc<Shared>,
    /// Incremented on every accepted connection; namespaces turn ids.
    connection: u64,
}

impl Mapper {
    async fn run(
        mut self,
        mut events: mpsc::UnboundedReceiver<ProviderEvent>,
        mut statuses: mpsc::UnboundedReceiver<StatusUpdate>,
    ) {
        self.publish_status();
        loop {
            tokio::select! {
                Some(event) = events.recv() => self.on_event(event),
                Some((state, error)) = statuses.recv() => {
                    self.status.state = state;
                    self.status.error = error;
                    self.publish_status();
                }
                else => break,
            }
        }
    }

    fn on_event(&mut self, event: ProviderEvent) {
        match event {
            ProviderEvent::Ready => {
                self.connection += 1;
                self.status.state = SttState::Streaming;
                self.status.error = None;
                self.publish_status();
            }
            ProviderEvent::Partial {
                turn,
                text,
                start_ms,
                end_ms,
            } => {
                let segment = self.segment(turn, text, start_ms, end_ms, false, None);
                (self.emit)(PipelineEvent::TranscriptPartial { segment });
            }
            ProviderEvent::Final {
                turn,
                text,
                start_ms,
                end_ms,
                confidence,
                ..
            } => {
                let segment = self.segment(turn, text, start_ms, end_ms, true, confidence);
                (self.emit)(PipelineEvent::TranscriptFinal { segment });
                let latency = self
                    .shared
                    .last_voice
                    .lock()
                    .ok()
                    .and_then(|last| *last)
                    .map(|at| at.elapsed())
                    .filter(|elapsed| *elapsed < LATENCY_HORIZON);
                if let Some(latency) = latency {
                    self.status.last_final_latency_ms = Some(latency.as_millis() as u32);
                    self.publish_status();
                }
            }
        }
    }

    fn segment(
        &self,
        turn: u64,
        text: String,
        start_ms: u64,
        end_ms: u64,
        is_final: bool,
        confidence: Option<f32>,
    ) -> TranscriptSegment {
        let source = source_id(self.source);
        TranscriptSegment {
            id: format!("{source}-{}-{turn}", self.connection),
            session_id: self.session_id.clone(),
            source_id: source.to_string(),
            start_ms,
            end_ms: end_ms.max(start_ms),
            text: text.chars().take(MAX_TEXT_CHARS).collect(),
            normalized_text: None,
            is_final,
            confidence: confidence.map(|c| c.clamp(0.0, 1.0)),
            speaker_id: None,
        }
    }

    fn publish_status(&mut self) {
        self.status.dropped_chunks = self.shared.dropped_chunks.load(Ordering::Relaxed);
        (self.emit)(PipelineEvent::SttStatus(self.status.clone()));
    }
}

fn source_id(source: SourceKind) -> &'static str {
    match source {
        SourceKind::System => "system",
        SourceKind::Microphone => "microphone",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stt::types::{ProviderKind, SttErrorCode};
    use futures_util::future::BoxFuture;
    use std::collections::VecDeque;

    /// Scripted provider: each connection plays one script entry against the received audio.
    struct FakeProvider {
        scripts: Mutex<VecDeque<Script>>,
        connections: AtomicU64,
        received: Mutex<Vec<String>>,
        chunk_bytes: Mutex<Vec<usize>>,
    }

    #[derive(Clone)]
    enum Script {
        /// Echo "turn N" partials/finals; a ForceEndpoint closes the current turn.
        Transcribe,
        /// Accept, then fail after receiving this many chunks.
        FailAfter(usize, SttErrorCode),
    }

    impl FakeProvider {
        fn new(scripts: Vec<Script>) -> Arc<Self> {
            Arc::new(Self {
                scripts: Mutex::new(scripts.into()),
                connections: AtomicU64::new(0),
                received: Mutex::new(Vec::new()),
                chunk_bytes: Mutex::new(Vec::new()),
            })
        }
    }

    impl SttProvider for FakeProvider {
        fn kind(&self) -> ProviderKind {
            ProviderKind::Soniox
        }

        fn stream<'a>(
            &'a self,
            audio: &'a mut mpsc::Receiver<AudioMsg>,
            events: &'a EventSender,
        ) -> BoxFuture<'a, Result<(), SttError>> {
            Box::pin(async move {
                self.connections.fetch_add(1, Ordering::Relaxed);
                let script = self
                    .scripts
                    .lock()
                    .unwrap()
                    .pop_front()
                    .unwrap_or(Script::Transcribe);
                let _ = events.send(ProviderEvent::Ready);
                let (mut turn, mut chunks) = (0, 0);
                while let Some(message) = audio.recv().await {
                    match message {
                        AudioMsg::Chunk(bytes) => {
                            chunks += 1;
                            self.chunk_bytes.lock().unwrap().push(bytes.len());
                            let loud = bytes
                                .as_chunks::<2>()
                                .0
                                .iter()
                                .any(|b| i16::from_le_bytes(*b) != 0);
                            self.received
                                .lock()
                                .unwrap()
                                .push(if loud { "voice" } else { "silence" }.into());
                            if let Script::FailAfter(n, code) = &script
                                && chunks >= *n
                            {
                                return Err(SttError::new(*code, "scripted failure"));
                            }
                            if loud {
                                let _ = events.send(ProviderEvent::Partial {
                                    turn,
                                    text: format!("turn {turn}"),
                                    start_ms: 0,
                                    end_ms: 80,
                                });
                            }
                        }
                        AudioMsg::ForceEndpoint => {
                            self.received.lock().unwrap().push("force".into());
                            let _ = events.send(ProviderEvent::Final {
                                turn,
                                text: format!("turn {turn} final"),
                                start_ms: 0,
                                end_ms: 80,
                                confidence: Some(0.9),
                                language: Some("fr".into()),
                            });
                            turn += 1;
                        }
                    }
                }
                Ok(())
            })
        }
    }

    fn recorder() -> (Emit, Arc<Mutex<Vec<PipelineEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        (Arc::new(move |e| sink.lock().unwrap().push(e)), events)
    }

    fn options(force: Option<u32>) -> SessionOptions {
        SessionOptions {
            source: SourceKind::System,
            session_id: "s1".into(),
            force_endpoint_ms: force,
            policy: RecoveryPolicy {
                initial_delay: Duration::from_millis(10),
                max_attempts: 2,
            },
        }
    }

    fn voice() -> Vec<i16> {
        (0..FRAME_SAMPLES)
            .map(|i| ((i as f32 * 0.3).sin() * 8_000.0) as i16)
            .collect()
    }

    async fn feed(sink: &mut SttSink, frame: &[i16], count: usize) {
        for _ in 0..count {
            sink.push(SourceKind::System, frame, 0);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    fn transcripts(events: &Mutex<Vec<PipelineEvent>>) -> Vec<(String, String, bool)> {
        events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                PipelineEvent::TranscriptPartial { segment }
                | PipelineEvent::TranscriptFinal { segment } => {
                    Some((segment.id.clone(), segment.text.clone(), segment.is_final))
                }
                _ => None,
            })
            .collect()
    }

    fn last_status(events: &Mutex<Vec<PipelineEvent>>) -> Option<SttStatus> {
        events.lock().unwrap().iter().rev().find_map(|e| match e {
            PipelineEvent::SttStatus(s) => Some(s.clone()),
            _ => None,
        })
    }

    async fn settle() {
        tokio::time::sleep(Duration::from_millis(150)).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn partial_updates_share_one_id_and_the_vad_closes_the_turn() {
        let provider = FakeProvider::new(vec![Script::Transcribe]);
        let (emit, events) = recorder();
        let mut sink = spawn(
            &Handle::current(),
            provider.clone(),
            options(Some(300)),
            emit,
        );

        feed(&mut sink, &voice(), 20).await; // 400 ms of speech
        // The device goes quiet (loopback sends nothing): padding must carry the silence.
        tokio::time::sleep(Duration::from_millis(600)).await;
        drop(sink);
        settle().await;

        let segments = transcripts(&events);
        assert!(segments.len() >= 2, "{segments:?}");
        assert!(
            segments.iter().all(|(id, ..)| id == "system-1-0"),
            "{segments:?}"
        );
        assert_eq!(
            segments.last().unwrap(),
            &("system-1-0".to_string(), "turn 0 final".to_string(), true)
        );

        let received = provider.received.lock().unwrap().clone();
        assert!(
            received.contains(&"force".to_string()),
            "VAD never forced the endpoint: {received:?}"
        );
        assert!(
            received.iter().filter(|r| *r == "silence").count() >= 3,
            "no padding: {received:?}"
        );
        assert_eq!(last_status(&events).unwrap().state, SttState::Closed);
        assert!(
            last_status(&events)
                .unwrap()
                .last_final_latency_ms
                .is_some()
        );
    }

    // Regression: the partial chunk flushed before ForceEndpoint was 10 ms long and AssemblyAI
    // closed the session ("Input Duration Violation"), losing the sentence.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn every_audio_message_is_a_full_80_ms_chunk_even_around_forced_endpoints() {
        let provider = FakeProvider::new(vec![Script::Transcribe]);
        let (emit, _events) = recorder();
        let mut sink = spawn(
            &Handle::current(),
            provider.clone(),
            options(Some(300)),
            emit,
        );

        // 13 voice frames = 260 ms: not a multiple of 80 ms, so the endpoint flush is partial.
        feed(&mut sink, &voice(), 13).await;
        tokio::time::sleep(Duration::from_millis(600)).await;
        feed(&mut sink, &voice(), 7).await;
        drop(sink);
        settle().await;

        let sizes = provider.chunk_bytes.lock().unwrap().clone();
        assert!(
            provider
                .received
                .lock()
                .unwrap()
                .contains(&"force".to_string())
        );
        assert!(!sizes.is_empty());
        assert!(sizes.iter().all(|&n| n == CHUNK_SAMPLES * 2), "{sizes:?}");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn reconnects_after_a_network_failure_with_new_ids_and_no_replay() {
        let provider = FakeProvider::new(vec![
            Script::FailAfter(3, SttErrorCode::Network),
            Script::Transcribe,
        ]);
        let (emit, events) = recorder();
        let mut sink = spawn(
            &Handle::current(),
            provider.clone(),
            options(Some(200)),
            emit,
        );

        feed(&mut sink, &voice(), 30).await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        drop(sink);
        settle().await;

        assert_eq!(provider.connections.load(Ordering::Relaxed), 2);
        let ids: Vec<String> = transcripts(&events)
            .into_iter()
            .map(|(id, ..)| id)
            .collect();
        assert!(ids.iter().any(|id| id.starts_with("system-1-")));
        assert!(
            ids.iter().any(|id| id.starts_with("system-2-")),
            "second connection reuses no id: {ids:?}"
        );
        let states: Vec<SttState> = events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                PipelineEvent::SttStatus(s) => Some(s.state),
                _ => None,
            })
            .collect();
        assert!(states.contains(&SttState::Reconnecting), "{states:?}");
        assert_eq!(states.last(), Some(&SttState::Closed));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_network_outage_longer_than_the_backoff_budget_is_still_retried() {
        // The test policy allows 2 attempts; the outage lasts 3 connections.
        let provider = FakeProvider::new(vec![
            Script::FailAfter(1, SttErrorCode::Network),
            Script::FailAfter(1, SttErrorCode::Network),
            Script::FailAfter(1, SttErrorCode::Network),
            Script::Transcribe,
        ]);
        let (emit, events) = recorder();
        let mut sink = spawn(&Handle::current(), provider.clone(), options(None), emit);

        feed(&mut sink, &voice(), 40).await;
        settle().await;

        assert_eq!(provider.connections.load(Ordering::Relaxed), 4);
        assert_eq!(last_status(&events).unwrap().state, SttState::Streaming);
        drop(sink);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_authorization_failure_is_reported_and_not_retried() {
        let provider = FakeProvider::new(vec![Script::FailAfter(1, SttErrorCode::Unauthorized)]);
        let (emit, events) = recorder();
        let mut sink = spawn(&Handle::current(), provider.clone(), options(None), emit);

        feed(&mut sink, &voice(), 10).await;
        settle().await;

        assert_eq!(provider.connections.load(Ordering::Relaxed), 1);
        let status = last_status(&events).unwrap();
        assert_eq!(status.state, SttState::Error);
        assert_eq!(
            status.error.map(|e| e.code),
            Some(SttErrorCode::Unauthorized)
        );
        drop(sink);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn without_a_forced_endpoint_the_provider_decides() {
        let provider = FakeProvider::new(vec![Script::Transcribe]);
        let (emit, _events) = recorder();
        let mut sink = spawn(&Handle::current(), provider.clone(), options(None), emit);

        feed(&mut sink, &voice(), 10).await;
        tokio::time::sleep(Duration::from_millis(900)).await;
        drop(sink);
        settle().await;

        assert!(
            !provider
                .received
                .lock()
                .unwrap()
                .contains(&"force".to_string())
        );
    }
}
