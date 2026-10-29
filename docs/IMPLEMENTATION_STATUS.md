# Implementation status

Last updated: 2026-10-29 · Current phase: **Phase 6 implemented and tested without paid
APIs; the user's end-of-development test is next.**

- **Phase 6:** answers start while the classifier decides; long interviews get a rolling
  summary; transcription reconnects for as long as a network outage lasts; latency is shown
  per card; Windows installer. Since then, only technical questions get a card and an answer
  (setting on by default).
- **Phase 5:** answers stream from Claude Sonnet 5.5 into the question's card. A verified
  profile is imported from a CV PDF and reviewed before use. `answer-bench` is not run yet.
- **Phase 4:** measured. On the hold-out corpus: 94 % recall, no false positive, about 1.2 s per
  classification.

Out of scope by decision (2026-10-29): profile encryption, a privacy document, and Phase 7
(macOS).

## Environment

| Item          | Value                                                                                        |
| ------------- | -------------------------------------------------------------------------------------------- |
| OS            | Windows 11 Home 10.0.26300 (x64), display scale 200 %                                        |
| Node / pnpm   | 22.14.0 / 10.12.1                                                                            |
| Rust          | 1.99.1 stable-x86_64-pc-windows-msvc (Rustup 1.29.1, installed via winget on 2026-10-28)     |
| C++ toolchain | Visual Studio 2022 Build Tools 17.14 — VCTools workload (installed via winget on 2026-10-28) |
| WebView2      | 156.0.4314.50                                                                                |
| Repository    | Branch `feat/overlay-audio-transcription`, no remote                                         |

`cargo` is installed in `%USERPROFILE%\.cargo\bin`. Shells opened before the install may need
`PATH` refreshed (open a new terminal).

### Pinned versions and why

- Tauri 2.12 (crate 2.12.5, CLI/API 2.12.4), plugins `global-shortcut` 2.4.1, `window-state` 2.5.0.
- Vue 3.5.44, Pinia 4.1.0 (setup stores), Vite 8.3.5, Vitest 5.0.5, zod 4.6.7.
- **TypeScript 6.0.3**, not 7.x: `typescript-eslint` supports `<6.1`, and the official
  `create-tauri-app` vue-ts template pins `~6.0`.
- **jsdom 29.1**, not 30.x: jsdom 30 needs Node ≥ 22.22 (its undici 8 crashes on Node 22.14).
- Question detection (Phase 4): `reqwest` 0.13 with `native-tls` (schannel, as for the
  WebSockets; no rustls/aws-lc build), `json`, `http2`, `system-proxy`. Raw HTTP to the Claude
  Messages API: there is no official Rust SDK.
- Transcription (Phase 3): `tokio-tungstenite` 0.30 (native TLS / schannel), `keyring` 4.2
  (Windows Credential Manager, v1 `Entry` API), `hound` 3.5 (benchmark harness only, behind
  the `bench` feature).
- Audio (Phase 2): `cpal` 0.18.2 (WASAPI, loopback), `rubato` 5.0.1 (`Fft` resampler, via
  `audioadapter` buffers), `rtrb` 0.4.1 (lock-free SPSC ring buffer). APIs were checked against the
  crate sources, not assumed.

## Phase 0 — repository & architecture: ✅ (CI not exercised)

- Tauri 2 + Vue 3 + TS project at the repository root (single app; no empty packages).
- Strict TypeScript (`strict`, `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`).
- ESLint (flat config, vue + typescript) and Prettier; Vitest + Vue Test Utils; Rust unit tests.
- Typed, runtime-validated event contracts: `src/domain/events.ts`.
- Docs: [ARCHITECTURE.md](ARCHITECTURE.md), [UI_STATE_MACHINE.md](UI_STATE_MACHINE.md), [PRD.md](PRD.md).
- CI workflow `.github/workflows/ci.yml` (frontend checks + `cargo test` on `windows-latest`).
  **Not run yet** — there is no remote.

## Phase 1 — floating overlay with mocked events

### Deliverables

| Deliverable                                                                                                                        | Where                                                       | Verified how                                                                                                                                   |
| ---------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| Card lifecycle manager (rules 1–7, request ids, seq dedupe/reorder)                                                                | `src/domain/cardLifecycle.ts`                               | 22 unit tests incl. 200 seeded random interleavings of 3 concurrent streams; mutation-checked                                                  |
| Event validation                                                                                                                   | `src/domain/events.ts`                                      | 8 tests (unknown enums, malformed, garbage input never throws)                                                                                 |
| Session store (validated `dispatch`, diagnostics, separate active question)                                                        | `src/stores/session.ts`                                     | 5 tests                                                                                                                                        |
| Mock pipeline (bilingual scripted interview, streaming, revision, follow-up, failure + retry, pause/stop/cancel, manual questions) | `src/services/mock/`                                        | 10 integration tests with fake timers through the real store                                                                                   |
| Card list with stable keys, isolation of updates                                                                                   | `src/components/cards/AnswerCardList.vue`                   | Component test: while A streams, B's component re-renders **0 times** and keeps the same DOM node; mutation-checked (index keys, copied props) |
| Safe answer rendering (no `v-html`)                                                                                                | `src/domain/richText.ts`, `RichText.vue`                    | 7 parser tests + XSS rendering test                                                                                                            |
| Active question panel (separate object)                                                                                            | `ActiveQuestionPanel.vue`                                   | Store test + browser                                                                                                                           |
| Manual question entry, clear history (2-click confirm)                                                                             | `OverlayFooter.vue`                                         | 4 component tests (incl. Esc regression)                                                                                                       |
| Scroll policy: follow when pinned, preserve otherwise, "N nouvelle(s) réponse(s)" pill                                             | `useStickyScroll.ts`, `scrollPolicy.ts`                     | 3 unit tests + measured in browser (see below)                                                                                                 |
| Focus view (enlarged answer), settings sheet, compact mode                                                                         | `App.vue`, `FocusView.vue`, `SettingsPanel.vue`             | 2 App tests + browser                                                                                                                          |
| Native window: undecorated, transparent, always-on-top, resizable, min size                                                        | `src-tauri/tauri.conf.json`                                 | Launched with `pnpm tauri dev`; `WS_EX_TOPMOST` set                                                                                            |
| Drag region, resize grip, opacity, always-on-top toggle, corner presets, next monitor, compact window resize                       | `OverlayHeader.vue`, `App.vue`, `platform/overlayWindow.ts` | Pure geometry tested (5 tests); **native behaviour not yet tested by hand**                                                                    |
| Global shortcuts with fallbacks + conflict report                                                                                  | `src-tauri/src/shortcuts.rs`                                | 6 Rust tests; native: Ctrl+Shift+Space hid/showed the window, Ctrl+Shift+P started the session through the Rust → webview event                |
| Window size/position persistence (per monitor)                                                                                     | `tauri-plugin-window-state`                                 | Not verified across restarts                                                                                                                   |

### Acceptance criteria

| Criterion                                     | Status              | Evidence                                                                                                                                                                                                                                                                                                                                                                |
| --------------------------------------------- | ------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Multiple cards coexist                        | ✅                  | Tests; browser demo (9 cards); native demo                                                                                                                                                                                                                                                                                                                              |
| Long answers do not break the layout          | ✅ browser          | Long code line scrolls inside its block; card width unchanged; `overflow-wrap: anywhere` on prose                                                                                                                                                                                                                                                                       |
| Updating one card never changes another       | ✅                  | Snapshot equality tests, interleaving property test, render-count test                                                                                                                                                                                                                                                                                                  |
| The window does not jump when cards are added | ✅ list / ⚠️ native | Newest-last, reading an older card: `scrollTop` stayed at 120, anchor card offset unchanged, pill shown. Newest-first: native scroll anchoring moved scrollTop 400 → 526.5 so the anchor card kept its on-screen offset (3 px). The native window position is never changed by card events (no code path does it), but this was not observed over a long native session |

### Mandatory UI regression scenarios (PRD §12.3)

| #   | Scenario                                                                            | Status                                                                                               |
| --- | ----------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| 1–5 | A, then B while A streams; both visible and associated; late A tokens update only A | ✅ automated (`cardLifecycle.test.ts`, `mockPipeline.test.ts`, `AnswerCardList.test.ts`)             |
| 6–7 | Scroll to an old answer, receive a new question, no jump                            | ✅ measured in the browser; ⏳ automate with Playwright (Phase 6)                                    |
| 8   | Resize the overlay and check card layout                                            | ⏳ manual native check pending                                                                       |
| 9   | Long code blocks                                                                    | ✅ browser                                                                                           |
| 10  | Network loss during generation                                                      | ✅ simulated failure → failed card keeps its partial answer → retry completes, other cards unchanged |
| 11  | Resume without duplicate cards                                                      | ✅ pause/resume test; ⏳ real reconnection is N/A until providers exist                              |
| 12  | Manual question while "speech" is active                                            | ✅ automated                                                                                         |

### Bugs found while verifying (all fixed)

1. Escape in the empty question field was swallowed (`.stop`), so panels could not be closed →
   fixed; regression test in `OverlayFooter.test.ts`.
2. The enlarged view opened underneath the settings sheet → layers made mutually exclusive;
   regression test in `App.test.ts`.
3. In the native window the panel only filled the top of the window (`#app` had no height
   outside browser mode), leaving a transparent area that still captures clicks → fixed in
   `base.css`. CSS-only, so no jsdom test; add a Playwright layout check in Phase 6.
4. With a translucent default (92 %), content behind the overlay showed through noticeably →
   default background opacity is now 100 %; the slider goes down to 40 %.
5. The streaming caret sat on its own line → now inline with the last line of text.

### Still to check by hand in the native window

Drag by the header · resize from the grip and borders · compact mode shrinking from the top edge
and restoring the previous height · corner presets and "Écran suivant" on a multi-monitor setup ·
opacity slider · turning always-on-top off · Ctrl+Shift+Enter and Ctrl+Shift+Up natively · size
and position restored after a restart.

### Known limitations (deliberate for Phase 1)

- Single overlay window. The main/settings window from PRD §2.1 is deferred until the orchestrator
  lives in Rust and can send events to several windows.
- Shortcuts use built-in fallbacks (Ctrl+Alt+…) when a binding is taken. They cannot yet be
  rebound from the UI.
- The history list is not virtualized (fine for one session; revisit if render cost shows up in
  measurements).
- Intent/language guesses for manual questions are mock-only heuristics in `mockPipeline.ts`.
- `transcript.*` events are defined in the contract but not consumed yet (Phase 3).
- No `tauri build` / installer produced yet (Phase 6).

## Phase 2 — Windows audio capture: ✅

Design and failure behaviour: [AUDIO_PIPELINE.md](AUDIO_PIPELINE.md).

### Deliverables

| Deliverable                                                             | Where                                                           | Verified how                                                                                                                                                                                                      |
| ----------------------------------------------------------------------- | --------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| System audio capture (WASAPI loopback, default or chosen output device) | `src-tauri/src/audio/backend.rs`                                | Hardware test: a 440 Hz tone played on "Haut-parleurs (Realtek(R) Audio)" captured for 1.49 s, tone-to-signal ratio 0.99. Native app: meter rose during a played tone and returned to "Aucun son joué" afterwards |
| Optional microphone capture, independent of system audio                | same + `AudioSettings.vue`                                      | Hardware test: 1.02 s captured from "Réseau de microphones (Realtek(R) Audio)". **Not yet exercised through the UI**                                                                                              |
| Independent source enable, device selection, mute                       | `AudioSettings.vue`, `AudioStatusStrip.vue`, `createSession.ts` | Component + wiring tests; mute pauses the device stream (worker tests)                                                                                                                                            |
| Permission and device handling                                          | `backend.rs` (error mapping), `worker.rs` (recovery)            | Unit tests incl. French-locale `E_ACCESSDENIED`; retry budget; permission errors not retried; "Ouvrir les paramètres" action tested                                                                               |
| Pause and resume                                                        | `engine.rs`, `worker.rs`                                        | Unit tests; native: Ctrl+Shift+P toggled listening ↔ paused                                                                                                                                                       |
| Audio provider abstraction                                              | `CaptureBackend` trait + `FrameSink` trait                      | Workers/engine fully tested against a scriptable fake backend                                                                                                                                                     |
| Capture diagnostics                                                     | `audio.diagnostics` event, `AudioStatusStrip.vue`               | Level (RMS/peak dBFS), receiving, format, dropped frames, recoveries; contract fixtures checked from Rust and TS                                                                                                  |
| 16 kHz mono PCM16 in 20 ms frames                                       | `convert.rs`                                                    | Accuracy, anti-aliasing and chunking tests                                                                                                                                                                        |
| Demo / live session modes                                               | `sessionRouter.ts`, `liveAudioSession.ts`                       | Unit tests; live is the default in the desktop app, the demo is still available in settings                                                                                                                       |

### Acceptance criteria

| Criterion                                             | Status                                 | Evidence                                                                                                                                                                   |
| ----------------------------------------------------- | -------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| The application captures the selected source reliably | ✅ system audio / ⚠️ microphone via UI | Loopback hardware test + native meter; mic hardware test. Device switching and unplug/replug were tested with the fake backend only                                        |
| Capture errors are visible                            | ✅                                     | Per-source error line with a localized cause, a retry note while reconnecting, and a fix-it action for privacy blocks; start failures shown in the strip (component tests) |
| UI remains responsive                                 | ✅                                     | Device work runs off the UI thread (async commands, one thread per source, lock-free callback); diagnostics are 10 Hz snapshots; no stutter seen while capturing natively  |

### Bugs found while verifying (fixed, with regression tests)

1. **Loopback captured nothing.** cpal reports WASAPI's `DATA_DISCONTINUITY` flag as an `Xrun`
   error, and the stream keeps running afterwards. The catch-all mapping turned it into a fatal
   backend error, so the worker dropped a healthy stream. `Xrun` is now informational
   (`backend.rs` test `a_buffer_discontinuity_does_not_stop_capture`). Found by the hardware test.
2. The fake backend considered a dropped stream still "playing", so the engine's "device
   released" assertion was meaningless. The fake now counts live streams and decrements on drop.

### Still to check by hand

Enable the microphone in Réglages and capture with both sources · unplug/replug headphones
during capture (expect "Reconnexion…" and then capture again) · deny microphone access in
Windows privacy settings (expect the error and **Ouvrir les paramètres**) · choose a non-default
output device.

### Known limitations

- Frames are discarded (`DiscardSink`); no VAD yet (both arrive in Phase 3).
- Changing devices requires stopping the session (mute works live).
- Diagnostics are sent at 10 Hz even when nothing changes (cheap; can be throttled later).
- If the output device applies effects (Realtek enhancements, spatial sound), loopback captures
  the processed signal.

## Phase 3 — streaming transcription: implemented, AssemblyAI validated on the smoke corpus

Design, protocols and the harness are documented in [TRANSCRIPTION.md](TRANSCRIPTION.md). The
provider choice comes from [the STT comparison report](../reports/Comparatif%20STT%20temps%20réel%20bilingue.md).

### Deliverables

| Deliverable                                                               | Where                                                                  | Verified how                                                                                                                                                                                                                                                          |
| ------------------------------------------------------------------------- | ---------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Provider abstraction (`SttProvider`, pure `Codec`, shared WebSocket loop) | `stt/provider.rs`, `stt/ws.rs`                                         | Loop tested against a local fake WebSocket server: config, audio, force endpoint, graceful finish, close and HTTP 401 classification                                                                                                                                  |
| AssemblyAI Universal-3.6 Pro adapter (EU)                                 | `stt/assemblyai.rs`                                                    | 8 codec tests built from the documented v3 messages (Begin → UpdateConfiguration, Turn partial/final, Termination, errors, close codes)                                                                                                                               |
| Soniox stt-rt-v5 adapter (EU)                                             | `stt/soniox.rs`                                                        | 7 codec tests (token assembly, `<end>`/`<fin>`, finished, error codes, header auth, end-of-stream frame)                                                                                                                                                              |
| Partial/final handling with stable ids, no duplicates                     | `stt/session.rs`, `stores/session.ts`                                  | Rust: partial updates share one id; reconnection opens a new id namespace. TS: a partial becomes the final in place; a late partial never reopens a final (mutation-checked)                                                                                          |
| VAD integration (energy VAD + fast endpoint)                              | `stt/vad.rs`, `stt/session.rs`                                         | 5 VAD tests, including a regression for speech at stream start; session test: VAD forces the endpoint after silence padded during loopback quiet                                                                                                                      |
| Silence padding for loopback                                              | `stt/pacing.rs`                                                        | Unit tests + session test (silent chunks sent while the device delivers nothing)                                                                                                                                                                                      |
| Reconnection without replay                                               | `stt/session.rs`                                                       | Session test: network failure → reconnect → new ids, stale audio dropped; unauthorized → stop, no retry                                                                                                                                                               |
| Mixed-language configuration                                              | `stt/mod.rs`                                                           | FR main, EN hints; context text; per-provider limits                                                                                                                                                                                                                  |
| Technical vocabulary dictionary                                           | `contracts/vocabulary/technical-terms.json`, `stt/vocabulary.rs`       | Parses, unique canonical terms, user terms first, capped per provider                                                                                                                                                                                                 |
| Keys in the OS credential store                                           | `stt/credentials.rs`, `stt/commands.rs`, `TranscriptionSettings.vue`   | Validation tests; with the app launched by the user, the key was saved, read back and found again after an app restart (traces, 2026-10-28); the benchmark read it from a separate process. Keys saved in an agent-launched instance stay in the sandbox and are lost |
| Diagnostics transcript panel                                              | `TranscriptPanel.vue`                                                  | Browser check with injected events; store tests                                                                                                                                                                                                                       |
| STT status and errors in the strip                                        | `AudioStatusStrip.vue`                                                 | Browser check (reconnecting/network error shown)                                                                                                                                                                                                                      |
| Consent step before live capture of system audio                          | `ConsentDialog.vue`, `App.vue`                                         | 3 App tests (mutation-checked); native: Ctrl+Shift+P now opens the consent step                                                                                                                                                                                       |
| Benchmark harness + smoke corpus                                          | `src/bin/stt-bench.rs`, `stt/eval.rs`, `scripts/make-smoke-corpus.ps1` | Scoring unit tests; run against AssemblyAI EU, single session, 3 endpoint settings (results in TRANSCRIPTION.md). Soniox not run (no key)                                                                                                                             |
| Rust ↔ TS contracts for `transcript.final` and `stt.status`               | `contracts/fixtures/`                                                  | Asserted from both test suites                                                                                                                                                                                                                                        |

### Acceptance criteria

| Criterion                                                    | Status                            | Evidence                                                                                                                                                                                                                                  |
| ------------------------------------------------------------ | --------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Partial transcript updates do not create duplicate questions | ✅ for transcripts / ➖ questions | Questions don't exist until Phase 4. Transcript lines are updated in place by stable id (Rust + TS tests)                                                                                                                                 |
| French, English and mixed-language test cases are evaluated  | ✅ smoke / ⏳ real corpus         | AssemblyAI, 12 synthetic utterances: WER 0 % FR, 0 % EN, 1.6 % mixed; term recall 93 %; time-to-final p50 613 ms, p95 994 ms (700 ms endpoint). The real ~150-utterance corpus (recorded, consenting speakers) and Soniox are still to do |
| The source transcript remains available for debugging        | ✅                                | Live transcript panel (last 200 segments in memory, cleared with the history); per-source STT status                                                                                                                                      |

**Live test in the app (2026-10-28, run by the user):** AssemblyAI transcription of real
system audio in the native overlay, judged "nickel". Not yet checked by hand: the microphone
through the UI, unplug and replug, and a privacy-blocked microphone.

### Bugs found while building (fixed, with regression tests)

1. The VAD missed speech that started at stream start: the rolling noise floor had no history,
   so the voice became its own floor. The window is now seeded with a quiet prior
   (`detects_speech_from_the_very_first_frames`).
2. Without padding, loopback silence would have reached the provider as _no audio_, delaying or
   preventing the end of a turn. The feeder now pads to real time (session test asserts silent
   chunks).

### Known limitations

- Energy VAD (not Silero). The provider's semantic end-of-turn does the real work; the local VAD
  only speeds up closure and measures time-to-final. Silero VAD can be swapped in later.
- AssemblyAI v3 close codes are not documented; auth and quota failures are inferred from the
  code or reason text and might be classified as retryable network errors.
- Soniox EU requires a project created in the EU region, with access requested in its console.
- No transcript normalization yet (`normalizedText` is `null`); that belongs to Phase 4 with the
  classifier.
- Keys are personal (the Credential Manager of this Windows user). A distributed product would
  need a small token service: AssemblyAI `/v3/token`, Soniox temporary keys.

## Phase 4 — intent classification: implemented, measured (latency above the PRD target)

Design, privacy and results: [QUESTION_DETECTION.md](QUESTION_DETECTION.md).

### Deliverables

| Deliverable                                                                                                                             | Where                                                                                                                 | Verified how                                                                                                                                                                                                      |
| --------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Taxonomy (9 intents) shared Rust ↔ TS                                                                                                   | `intent/types.rs`, `domain/types.ts`, `domain/intent.ts`                                                              | Contract fixtures `question-detected`, `question-revised`, `utterance-classified`, `classifier-status`, asserted from both test suites                                                                            |
| Utterance assembly (cut turns merged, finalization timeout)                                                                             | `intent/assembler.rs`                                                                                                 | 6 unit tests (merge, timeout, partials extend up to the hard limit, per-source independence, flush)                                                                                                               |
| First stage: fillers, acknowledgements, greetings, bare follow-ups, explicit-question hints, technical-domain hints, cut-turn detection | `intent/rules.rs`, `intent/text.rs`                                                                                   | 10 unit tests over FR/EN phrases; accents kept where folding is ambiguous ("sûr" vs "sur")                                                                                                                        |
| Structured classifier: prompt v3, JSON schema (decisive fields first), runtime validation, partial-JSON reader                          | `intent/classifier.rs`, `prompts/classification.v3.md`                                                                | 10 tests: rendering, alias resolution, invalid JSON / unknown enums / extra fields are errors, out-of-range values repaired, partial JSON on every prefix, schema order survives serialization                    |
| Claude adapter (Haiku 5.5 default / Opus 5.5), streamed, with timings and token usage                                                   | `intent/claude.rs`                                                                                                    | Request-shape tests; SSE reassembly byte by byte (UTF-8 cut); stream errors, refusal and truncation; HTTP tests against a local fake API (headers, retry once on 529, no retry on 401)                            |
| Follow-up context, continuation → revision, repeat suppression, statement suppression, fallback to rules                                | `intent/detector.rs`                                                                                                  | 13 unit tests                                                                                                                                                                                                     |
| Engine: two tasks (assembly timers independent of the model), sequential classification, status                                         | `intent/engine.rs`, `audio/commands.rs`                                                                               | 4 tests on a paused clock: order and merge, rules-only without a key, a rejected key switches to rules, a cut turn at stop is still classified                                                                    |
| Claude key in the Credential Manager (shared `secrets` module)                                                                          | `secrets.rs`, `intent/credentials.rs`, `intent/commands.rs`                                                           | Validation tests; saved from the UI of the user-launched app (2026-10-28), read back by `intent-bench` in another process, accepted by the API (authentication passed; the call failed on billing: no credit yet) |
| Settings, transcript tags, status strip warning, consent text                                                                           | `QuestionSettings.vue`, `TranscriptPanel.vue`, `AudioStatusStrip.vue`, `ConsentDialog.vue`                            | Component tests; browser check with injected events (tags, merged cut turn, revision, « pas de clé Claude » warning)                                                                                              |
| Simulated answers for live questions (until Phase 5)                                                                                    | `mockPipeline.answerDetected`, `createSession.ts`                                                                     | Wiring test: one card, completed, a replayed `question.detected` does not answer twice                                                                                                                            |
| Labeled corpora + harness + latency probe                                                                                               | `contracts/intent-corpus.v1.json`, `contracts/intent-corpus-holdout.v1.json`, `intent/eval.rs`, `bin/intent-bench.rs` | Both corpora validated on load and checked disjoint; oracle model scores 100 %; rules-only floors in a unit test; bench stops at the first key or billing error                                                   |

### Acceptance criteria

| Criterion                                         | Status   | Evidence                                                                                                                                                                                |
| ------------------------------------------------- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Short greeting answers                            | ✅ rules | Greetings are decided by the rules at once, as `GREETING_OR_SMALL_TALK` cards; a greeting repeated within 2 min is one card. The answers themselves stay simulated until Phase 5        |
| Statements rarely trigger answers                 | ✅       | Hold-out: 11/11 statements without a card (Haiku, 2 runs); main corpus 28/28 with v3                                                                                                    |
| Follow-ups reference the right question           | ✅       | Hold-out: 3/3 (Haiku, 2 runs); main corpus 9/9 with v3; rules alone 0/3 and 2/9                                                                                                         |
| Incomplete utterances don't re-trigger generation | ✅       | Cut turns are merged before classification (corpus merge 1/1); continuations become `question.revised`, repeats create nothing (tests + oracle run); ambiguous fragments create no card |

### Bugs found while building (fixed, with regression tests)

1. "Oui, bien sûr." was taken for a cut turn: accent folding turned "sûr" into the preposition
   "sur". Continuation words are now matched with accents kept (`cut_turns_are_recognized`).
2. Small talk that asks something (« Est-ce que tu m'entends bien ? ») after a greeting was
   dropped as a repeated greeting. Found by the oracle run on the corpus
   (`a_greeting_gets_a_card_at_once_and_only_once`).

## Phase 5 — LLM response engine: implemented, not measured yet

Design, privacy and evaluation: [ANSWER_ENGINE.md](ANSWER_ENGINE.md).

### Deliverables

| Deliverable                                                                                                  | Where                                                        | Verified how                                                                                                                                                                                                                                             |
| ------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Shared Claude client: models, errors, SSE streaming, one retry before the first token, key                   | `llm/`                                                       | 9 tests: model settings (Sonnet `between_tools`, Haiku thinking off, fallbacks), HTTP error mapping, SSE byte-by-byte reassembly, refusals and truncation, headers, a retry before any text, no retry after text or on 401                               |
| Streaming answer engine: one task per question, request ids, abort on retry/restart/clear, follow-up context | `answer/engine.rs`                                           | 5 tests against a local fake API: both sections streamed with contiguous `seq`, a missing key fails retryably, a follow-up sees the earlier answer and the profile, manual question → detected → failed → retried → completed, clearing stops everything |
| Section splitter (`<<<DETAILS>>>` never shown, even when cut)                                                | `answer/sections.rs`                                         | 3 tests: every cut position, one character at a time, look-alike text                                                                                                                                                                                    |
| Answer prompt v1 (templates per intent, no invented facts, placeholders, language)                           | `prompts/answer.v1.md`, `answer/prompt.rs`                   | Request-shape and rendering tests; **quality measured by `answer-bench` (not run yet)**                                                                                                                                                                  |
| Verified profile: storage, validation, rendering                                                             | `profile/mod.rs`                                             | 3 tests: trimming and limits, rendering, disk round trip, unverified files ignored                                                                                                                                                                       |
| CV import: PDF → Claude Opus 5.5 → unverified draft                                                          | `profile/extract.rs`, `prompts/profile-extraction.v1.md`     | 3 tests: PDF validation (type, size, base64), request shape (document block, structured output), strict draft parsing. **Not yet run on a real CV**                                                                                                      |
| Answer and profile UI (model choice, drop zone, review form, delete with confirmation)                       | `AnswerSettings.vue`, `answerBackend.ts`                     | 3 component tests: dropped PDF → draft → edited → saved; non-PDF refused without a call; delete asks for confirmation                                                                                                                                    |
| Live answers come from Rust (no more simulated answers in live mode)                                         | `createSession.ts`, `liveAudioSession.ts`                    | Wiring tests: manual/retry/cancel go to Rust, the model is configured at start and on change, Rust answer events fill the card                                                                                                                           |
| Rust ↔ TS answer contract                                                                                    | `contracts/fixtures/answer-delta.json`, `answer-failed.json` | Asserted from both suites                                                                                                                                                                                                                                |
| Answer benchmark with an invented-facts judge                                                                | `contracts/answer-bench.v1.json`, `bin/answer-bench.rs`      | Builds; not run (costs about $0.25 per run)                                                                                                                                                                                                              |

### Acceptance criteria

| Criterion                            | Status             | Evidence                                                                                                                         |
| ------------------------------------ | ------------------ | -------------------------------------------------------------------------------------------------------------------------------- |
| Progressive answers                  | ✅ tests / ⏳ live | Deltas stream into the card as they arrive (engine and wiring tests)                                                             |
| Late events hit the right card       | ✅                 | Request ids per generation; aborted requests' tokens are stale for the lifecycle (Phase 1 property tests); engine test for retry |
| No invented personal facts           | ⏳                 | Prompt rule + verified profile only + placeholders; to be measured by the bench's judge                                          |
| Failures keep transcript and history | ✅                 | `answer.failed` keeps the partial answer and the card; retry; the transcript is untouched                                        |

### Known limitations

See [ANSWER_ENGINE.md](ANSWER_ENGINE.md#known-limitations). The two calls in a row and the
missing summary were addressed in Phase 6. The profile is not encrypted at rest (out of scope).

## Phase 6 — integration & robustness: implemented, tested without paid APIs

Tests use fake APIs only (local fake Claude server, scripted classifier and STT provider), to
spend no credit. The user tests everything live at the end of development.

### Deliverables

| Deliverable                                                                                       | Where                                                                             | Verified how                                                                                                                                                                                             |
| ------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Early start: the answer starts when the utterance goes to the classifier, held until the decision | `answer/engine.rs` (`speculate`, `Route`), `intent/engine.rs` (`on_escalate`)     | 4 tests: held output never shown, replayed into the card with one request (finished, and still streaming with contiguous `seq`), dropped for a statement; the engine announces escalated utterances only |
| « Réponse anticipée » switch (on by default)                                                      | `AnswerSettings.vue`, `overlaySettings.ts`, `answer_configure`                    | Wiring test: model and switch sent at start and on change                                                                                                                                                |
| Rolling summary of the lines that leave the context (Haiku 5.5)                                   | `answer/summary.rs`, `prompts/summary.v1.md`                                      | 2 tests: request shape; 24 lines → one summary call, then the summary is in the next answer                                                                                                              |
| Session wiring in one place (overlay first, then answers; detection; early start)                 | `pipeline.rs`, `audio/commands.rs`                                                | End-to-end test: two transcripts → a statement with no card, and a card with its streamed answer, two requests, nothing extra on screen                                                                  |
| Transcription never gives up on a network outage (backoff, then every 5 s)                        | `stt/session.rs`                                                                  | Test: an outage longer than the backoff budget is still recovered; an auth error is still not retried                                                                                                    |
| Latency per card: end of speech → first word, by stage                                            | `domain/latency.ts`, `stores/session.ts`, `AnswerCard.vue`, `TranscriptPanel.vue` | Store test with timed events (transcription 0.7 s, detection 1.2 s with Claude 1.1 s, answer 0.06 s); the card-isolation test still passes                                                               |
| Technical questions only (on by default): other questions get no card and no answer               | `intent/detector.rs`, `QuestionSettings.vue`, `overlaySettings.ts`                | Detector test: greeting, behavioral question and its follow-up left out; a technical question and its follow-up kept. Transcript tag « Ignoré · question non technique »                                 |
| Windows installer                                                                                 | `scripts/build.ps1` (`pnpm tauri build`, NSIS)                                    | Built: `src-tauri/target/release/bundle/nsis/Interview Copilot_0.1.0_x64-setup.exe` (2.5 MB). Not installed or run here                                                                                  |

### Acceptance criteria

| Criterion                              | Status             | Evidence                                                                                                                            |
| -------------------------------------- | ------------------ | ----------------------------------------------------------------------------------------------------------------------------------- |
| Audio to card end to end               | ✅ tests / ⏳ live | `pipeline.rs` test from transcripts to a filled card; the STT session has its own tests; live test by the user                      |
| Survives provider and network failures | ✅ tests           | STT: reconnects indefinitely; classifier: falls back to the rules; answers: one retry before the first token, then a retryable card |
| Critical UI and concurrency tests pass | ✅                 | 172 Rust tests, 124 front-end tests (card isolation, stale tokens, ordering)                                                        |
| Latency instrumentation                | ✅                 | Per-card badge with a stage-by-stage tooltip, and the median in the transcript bar                                                  |

### Known limitations

- The latency is measured in the webview, from event arrival times (the IPC adds a few ms). The
  end of speech comes from the STT session's own measure, which is missing when the provider
  sends no status right after the final.
- No Playwright end-to-end test of the native window; the layout checks stay manual.
- The installer is not code-signed: Windows SmartScreen warns on first run.

## Commands executed (key ones)

```text
# Phase 1
winget install Rustlang.Rustup                              # exit 0
winget install Microsoft.VisualStudio.2022.BuildTools ...    # VCTools workload, exit 0
git init
pnpm install
pnpm tauri dev                                              # native overlay launched, shortcuts registered

# Phase 2
cargo add cpal@0.18 rtrb@0.4 rubato@5                       # in src-tauri
cargo clippy --all-targets                                  # 0 warnings
cargo test                                                  # 40 passed, 3 ignored (hardware)
cargo test hardware -- --ignored --nocapture --test-threads=1   # 3 passed (after the Xrun fix)
pnpm format:check && pnpm lint && pnpm typecheck            # clean
pnpm test                                                   # 13 files, 91 tests passed
pnpm tauri dev                                              # live capture verified natively

# Phase 3
cargo add keyring@4 tokio-tungstenite@0.30 futures-util@0.3  # + tokio, hound (bench feature)
cargo clippy --all-targets --features bench                 # 0 warnings
cargo test                                                  # 81 passed, 4 ignored (hardware / OS store)
cargo test round_trips -- --ignored                         # round trip OK (inside the agent sandbox only)
pnpm test                                                   # 14 files, 104 tests passed
scripts/make-smoke-corpus.ps1                               # 12 synthetic WAVs + manifest
pnpm tauri dev                                              # app boots; consent step on Ctrl+Shift+P
stt-bench --provider assemblyai --fast-endpoint-ms 400|500|700   # smoke corpus, all pass except p50 (602-635 ms)
scripts/dev.ps1                                             # (user) live transcription test in the app: OK

# Phase 4
cargo add reqwest@0.13 (native-tls, json, http2, system-proxy)
cargo clippy --all-targets --features bench                 # 0 warnings
cargo test                                                  # 143 passed, 4 ignored (hardware / OS store)
pnpm check                                                  # lint + typecheck + 16 files, 116 tests passed
intent-bench --rules-only                                   # recall 93.2 %, statements 85.7 %, intents 78.0 %
intent-bench --model haiku                                  # prompt v2: recall 100 %, statements 100 %, intents 100 %,
                                                            #   follow-ups 8/9, p50 1 360 ms / p95 2 542 ms (fails latency)
intent-bench --probe 10                                     # first token: bare ~750 ms, full ~1 000-1 300 ms
intent-bench (v3, ×2) / intent-bench --holdout (×2)         # main: 0 errors; hold-out: recall 94 %, 0 false positive
curl (no key) to api.anthropic.com                          # connect 15-50 ms, rejected request 180-340 ms

# Phase 5
cargo test                                                  # 162 passed, 4 ignored
cargo clippy --all-targets --features bench                 # 0 warnings
pnpm check                                                  # lint + typecheck + 17 files, 123 tests passed

# Phase 6
cargo test                                                  # 172 passed, 4 ignored (after technical-only)
cargo clippy --all-targets --features bench                 # 0 warnings
pnpm check                                                  # lint + typecheck + 18 files, 124 tests passed
pnpm tauri build                                            # release build 5 min; Interview Copilot_0.1.0_x64-setup.exe, 2.5 MB
```

## How to run

```text
pnpm install
pnpm tauri dev      # native overlay; Démarrer (or Ctrl+Shift+P) starts live capture
pnpm dev            # browser only, http://localhost:1420 (demo mode; Tauri calls are no-ops)
pnpm check          # lint + typecheck + tests
pnpm test:rust      # Rust unit tests (hardware tests are opt-in, see AUDIO_PIPELINE.md)
.\scripts\build.ps1  # Windows installer: src-tauri\target\release\bundle\nsis\*-setup.exe
```

## Next concrete step

1. **You:** the end-of-development test, live:
   - install with the setup .exe (or `.\scripts\dev.ps1`);
   - import your CV in Réglages › Audio › Profil vérifié, review it and save it;
   - ask questions, typed or spoken (microphone switch on), and read the latency on each card.
2. **You (approval):** `answer-bench` with Sonnet 5.5 (about $0.25), to measure the answers'
   quality and the invented-facts rate.
3. **You:** before real interviews, move AssemblyAI to a paid plan, opt out of training, and
   check the Anthropic organization's retention settings.
