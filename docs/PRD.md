# Interview Copilot — Product Requirements & Technical Plan

Version 1.0 · AI-powered real-time desktop interview assistant
Target: Windows first, macOS later · Vue 3 + TypeScript + Pinia · Tauri 2 + Rust

> Reference copy of the specification provided by the product owner. Implementation progress is
> tracked in [IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md).

## 1. Product vision

A desktop application that listens to an **authorized** audio source during a conversation,
detects meaningful questions in real time, generates context-aware answers with an LLM, and shows
them in a persistent floating overlay. It supports technical interviews, coding/algorithm
questions, architecture and system design, behavioral/HR questions, greetings and conversational
questions, in French, English and mixed French-English technical speech, including explicit,
indirect, incomplete or reformulated questions. It is not a transcription tool: its job is to turn
conversational audio into useful, readable, context-aware answers.

**Core principles**

1. Never lose a detected question or its answer.
2. Never replace an existing answer with an unrelated answer.
3. Do not answer every sentence.
4. Understand French speech containing English technical terminology.
5. Respond to ordinary conversational questions, not only technical ones.
6. Preserve conversational context across questions.
7. Keep the overlay stable, readable and responsive.
8. Never fabricate information when context is insufficient.
9. Keep API credentials out of the frontend; protect captured audio and transcripts.

## 2. Primary user experience

**Startup:** main window; configure audio sources, language, AI provider, profile; request
microphone/system-audio permissions; start the session only after explicit user action; open the
overlay. Default configuration targets Windows.

**Session pipeline:** capture audio → detect speech/utterances → incremental transcription →
interpret mixed-language terminology → decide whether a response is needed → classify and
retrieve context → generate a concise answer → show it as a new card → keep all previous cards.

**Example:** “Bonjour, comment ça va aujourd'hui ?” → short greeting; “Can you introduce yourself
and explain your experience with Vue.js?” → profile-based answer; “Et comment tu gères la
performance sur une grosse application Vue ?” → technical answer. Expected: three separate cards,
none disappears. Personal answers come from the configured profile, never fabricated.

## 3. Speech recognition (most critical component)

- French main language; English technical terms inside French sentences; full English questions;
  code-switching; framework/library names and acronyms; accents, moderate noise, interruptions,
  partial sentences and self-corrections. Do **not** translate everything into one language.
- **Audio architecture (Windows):** WASAPI loopback for system output; optional independent
  microphone; independent selection and mute. Native capture in Rust. No microphone access required
  if only system audio is used.
- **Processing:** capture frames → resample/format → VAD → send to STT → partial/final segments →
  timestamps + source ids → finalized utterances to the understanding engine.
- Provider interface (`SpeechRecognitionProvider`: `start`, `stop`, `onPartialTranscript`,
  `onFinalTranscript`, `onError`) — adapt to the chosen SDK.
- **Technical vocabulary dictionary:** canonical spelling, pronunciation variants, acronyms,
  related concepts, aliases (JS/TS, Vue/React/Nuxt/Node, async, REST/GraphQL/WebSocket/gRPC,
  SQL/PostgreSQL/MongoDB/Redis, Docker/Kubernetes/AWS/CI-CD, Kafka/Elasticsearch/microservices,
  algorithms/Big O, SOLID/patterns/system design). Correct terms only when supported by the
  acoustic transcript and context — never global similarity replacement.
- **Transcript segment:** id, start/end timestamps, source id, partial/final, original text,
  optional normalized text, optional confidence, optional speaker id. Never treat a partial as a
  finalized question; partials may prepare classification but must not create duplicate answers.

## 4. Question and intent detection

Intents: `TECHNICAL_QUESTION`, `CODING_PROBLEM` (approach + code + complexity),
`SYSTEM_DESIGN` (requirements, architecture, trade-offs, scaling, failures),
`EXPERIENCE_QUESTION` (from verified profile only), `BEHAVIORAL_QUESTION` (STAR, placeholders for
missing facts), `GREETING_OR_SMALL_TALK` (short natural reply), `CLARIFICATION_OR_FOLLOW_UP` (use
previous Q/A), `NON_QUESTION_STATEMENT` (no answer unless clearly expected), `AMBIGUOUS` (wait for
more context; no repeated speculative answers).

- Classification output is structured JSON (`intent`, `requiresResponse`, `question`, `language`,
  `domain`, `urgency`, `confidence`, `contextReference`), validated at runtime; invalid output must
  not crash the session. Confidence is only trusted after calibration on a test set.
- **Two stages:** a lightweight first stage (completed utterances, non-speech/duplicate filtering,
  obvious greetings/question patterns, technical vocabulary) and an LLM second stage (ambiguous
  intent, whether an answer is expected, reference resolution, domain/response type). The first
  stage is never the only question detector.
- **Avoid false positives/negatives:** VAD/transcription completion signals, short configurable
  debounce, semantic classification, follow-up detection, duplicate detection, finalization
  timeout. Revisions of a partial update the in-progress interpretation instead of creating a
  second card.

## 5. Answer generation

- Relevant, concise, natural when spoken, accurate, consistent, personalized from verified facts,
  in the conversation's language (FR → FR, EN → EN, mixed → dominant language keeping English
  technical terms; user-overridable).
- **Templates:** technical (direct answer, principle, example, caveats); coding (interpretation,
  algorithm, code, time & space complexity, edge cases); behavioral (short answer, optional STAR,
  verified personalization); greeting (one short reply, optional return question); follow-up
  (resolve context, answer only the new point).
- **Two lengths:** primary (readable in seconds, ~2–5 sentences; for code, approach first with code
  in an expandable section) and expanded (details, code, alternatives, complexity, trade-offs).
- **Prompts:** versioned files for classification, generation, summarization, optional
  normalization. Generation must answer the actual question, use the verified profile, never invent
  experience/metrics, keep technical terms, prefer spoken style, use context for follow-ups, state
  uncertainty, return validated structured data.
- **Context:** recent final segments, recent Q/A pairs, current question, profile, optional role and
  job description, rolling summary. Bounded windows; never send the whole transcript every time.

## 6. Floating overlay UI

- Native always-on-top window (handle OS limits gracefully), draggable, resizable, adjustable
  opacity, compact/expanded modes, configurable position, multi-monitor, persistent history,
  scrollable cards, loading/error states, global shortcuts, start/pause/stop.
- **Never overwrite unrelated answers:** card-based history, one card per question id.
  Card model: `id`, `questionId`, `question`, `answer`, `status` (`pending` | `streaming` |
  `completed` | `failed`), timestamps, `intent`, `language` (strict enums in production).
- **Card update rules:** (1) new question → new pending card, never mutate others; (2) stream
  tokens into the card with the same question id; (3) completed → `completed`; (4) revisions update
  only the same unfinished utterance, otherwise an explicit correction policy; (5) follow-up → new
  card linked by context reference; (6) a new question while another generates → new card
  immediately, cancel/deprioritize only under an explicit policy; (7) failure → card stays visible
  with a retry action.
- **Layout stability:** fixed outer window by default with a bounded scrollable card area; window
  position stable when answers arrive; virtualize when history is large; keep newest visible when
  appropriate; never force-scroll a reader of older answers; preserve scroll position on updates of
  older cards; no horizontal overflow from code; expandable long answers; limited automatic growth;
  option to clear history. Layout: header (status, controls) · pinned active question · cards
  (newest first or last, configurable) · scrollable history · footer (text input, quick actions).
  Stable Vue keys from card ids.
- **Active question panel** and **answer history** are independent; never reuse the same reactive
  object for unrelated questions (mandatory).
- **Shortcuts (configurable):** `Ctrl+Shift+Space` show/hide, `Ctrl+Shift+P` pause/resume,
  `Ctrl+Shift+Enter` manual question, `Ctrl+Shift+Up` enlarge active answer, `Esc` close detail.
  Register through Tauri APIs; detect conflicts and provide alternatives.

## 7. Technical architecture

Tauri 2 + Rust, Vue 3 Composition API, TypeScript, Vite, Pinia; native Windows audio, VAD,
pluggable streaming STT; TypeScript AI orchestration with streaming, structured-output
validation, classification and generation; SQLite for optional history/settings with memory-only
sessions by default; Vitest, Vue Test Utils, Rust unit tests, Playwright, audio fixture replay.

Pipeline: UI → Pinia session store → orchestrator → native audio (Rust) → STT → transcript
normalizer → intent classifier → context manager → LLM answer generator → card lifecycle manager →
stable overlay. Provider interfaces for audio capture, STT, classification, generation and
persistence (DI/registry). Start with one validated STT and one validated LLM provider. The UI
must not call an LLM with secret credentials.

Typed events: `transcript.partial`, `transcript.final`, `question.detected`, `answer.started`,
`answer.delta`, `answer.completed`, `answer.failed` — validated at boundaries, every card event
carries a stable question id; request ids and cancellation tokens prevent stale responses from
updating the wrong card.

## 8. Data models

Session (id, timestamps, status, audio/language config, optional role/JD, profile ref) ·
TranscriptSegment (id, session, source, timestamps, original/normalized text, partial/final,
confidence) · DetectedQuestion (id, session, source segment ids, utterance, normalized question,
intent, language, domain, context ref, classification status) · Answer (id, question id, text,
optional code, status, timestamps, model metadata without secrets, error) · UserProfile (name,
verified summary, skills, user-provided experience and projects, preferred response/programming
language, optional JD). Never store inferred professional facts as verified.

## 9. Performance targets (initial, to be measured)

Non-blocking overlay; partial transcripts shown immediately; classification median < 500 ms after
finalization (excluding provider delays); first useful answer text median ≤ 2 s after finalization
when conditions allow; progressive streaming; no duplicate cards; no lost events under concurrency;
no blocking UI. Instrument capture delay, finalization delay, classification time, LLM first-token
latency, total answer time, errors/retries, detected vs. expected questions.

## 10. Error handling and recovery

Missing mic permission, system-audio failure, device disconnection, STT timeout, network loss, LLM
rate limits, invalid structured output, malformed/duplicate/out-of-order events, suspend/resume,
unsupported formats. Bounded retries with exponential backoff; no infinite replay or duplicate
answers after reconnection. Status messages: Listening, Paused, Transcribing, Analyzing question,
Generating answer, Reconnecting, Error. AI failures preserve the transcript and all cards.

## 11. Privacy and security

Explicit permission before capture; visible capture status; immediate pause/stop; never secretly
activate the microphone; no raw audio retention by default; configurable transcript persistence;
API keys in the OS credential store; no credentials in source control; minimal context sent to
providers; documented provider data handling; protected local history; clear-session action. The
application must not implement mechanisms intended to bypass interview-platform monitoring or
security controls.

## 12. Testing strategy

- **Unit:** classification parsing, normalization, duplicate detection, question ids, answer
  lifecycle transitions, context references, card updates, cancellation/retry, store consistency.
- **Speech fixtures:** FR-only, EN-only, mixed technical, accented, acronyms, greetings,
  non-answer statements, incomplete, self-corrections, follow-ups, similar consecutive questions —
  with expected transcripts, intents and acceptable variants.
- **UI regression (mandatory):** (1) question A → answer A; (2) question B while A generates;
  (3) both cards visible and associated; (4) late tokens for A after B starts; (5) they update only
  A; (6) scroll to an old answer and receive a new question; (7) no unexpected scroll jump;
  (8) resize and check layout; (9) long code blocks; (10) network loss during generation;
  (11) resume without duplicate cards; (12) manual question while speech recognition is active.
- **Metrics:** question recall, false-positive/negative rates, intent accuracy, WER, technical-term
  accuracy, relevance, factuality, first-answer latency, duplicate-card rate, lost/misassociated
  answer rate — per language and intent; thresholds set after building the corpus.

## 13. Implementation phases

- **Phase 0 — Repository & architecture:** Tauri 2 + Vue 3 + TS project, Pinia, lint/format,
  Vitest, Rust tests, typed event contracts, architecture docs. _Accept:_ app launches; CI runs
  tests; UI runs without audio or LLM.
- **Phase 1 — Floating overlay:** native always-on-top window, drag/resize, opacity,
  compact/expanded, shortcuts, card list with stable ids, active question panel, manual entry,
  mocked events. _Accept:_ multiple cards coexist; long answers don't break layout; updating one
  card never changes another; window doesn't jump when cards are added.
- **Phase 2 — Audio capture:** Windows system audio, optional mic, permissions/devices,
  pause/resume, audio provider abstraction, diagnostics. _Accept:_ reliable capture of the
  selected source; visible errors; responsive UI.
- **Phase 3 — Streaming transcription:** STT integration, partial/final handling, VAD,
  mixed-language config, vocabulary dictionary, diagnostics transcript panel. _Accept:_ no
  duplicate questions from partials; FR/EN/mixed cases evaluated; transcript available.
- **Phase 4 — Intent classification:** taxonomy, structured classifier, greetings, technical and
  coding detection, statement suppression, follow-up context, duplicate/debounce. _Accept:_ short
  greeting answers; statements rarely trigger answers; follow-ups reference the right question;
  incomplete utterances don't re-trigger generation.
- **Phase 5 — LLM response engine:** streaming provider, intent prompts, verified profile,
  formatting, structured code, errors, summarization. _Accept:_ progressive answers; late events
  hit the right card; no invented personal facts; failures keep transcript and history.
- **Phase 6 — Integration & robustness:** full pipeline, latency instrumentation, E2E tests,
  recovery/cancellation, secure credentials, privacy docs, Windows packaging. _Accept:_ audio to
  card end-to-end; survives provider/network failures; critical UI/concurrency tests pass.
- **Phase 7 — macOS:** only after Windows is stable (native capture, permissions, windows,
  packaging/signing, regression tests).

## 14. Repository structure (suggested; adapt to real constraints)

`apps/desktop` (Vue app + `src-tauri`), `packages/*` (domain, event contracts, engines,
providers, profile, fixtures), `docs/*` (PRD, ARCHITECTURE, AUDIO_PIPELINE, QUESTION_DETECTION,
UI_STATE_MACHINE, TEST_PLAN, PRIVACY), `tests/{integration,e2e}`, `.env.example` (placeholder names
only), `README.md`. Do not create empty packages or unnecessary abstractions.

## 15. Engineering rules

Inspect before modifying; report OS/toolchains/status; plan before coding; one phase at a time;
run tests after significant changes; strict TypeScript; small typed modules; no giant components or
monolithic services; provider SDKs behind adapters; no hardcoded credentials; never assume an SDK
API exists without checking its docs; one AI provider until the end-to-end path works; no mocked
assertions that cannot detect regressions; a test for every bug fix; maintain
`docs/IMPLEMENTATION_STATUS.md`; don't claim a phase complete before verifying acceptance criteria;
don't rewrite unrelated modules; ask for input only for real product/security decisions.

## 16. MVP definition (Windows)

Launch → explicit start of an authorized session → capture → FR/EN/mixed transcription →
detection of technical, coding, greeting and follow-up questions → irrelevant statements filtered →
concise context-aware answers → streamed into the correct card → new question = new card →
concurrent generations cannot corrupt each other → readable, stable overlay → cards survive
provider/network failures → tests cover concurrency, language and UI requirements → packaged app
runs outside the dev environment.

## 17. First task

Inspect the repository, report environment, propose a sequence, initialize Tauri 2 + Vue 3 + TS,
implement **Phase 1 only** with mocked events, build and test the card lifecycle manager (two
answers streaming concurrently without overwriting), run tests, update
`docs/IMPLEMENTATION_STATUS.md`, and stop with the exact next step before audio capture.
