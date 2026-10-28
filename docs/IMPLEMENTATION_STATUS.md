# Implementation status

Last updated: 2026-10-28 · Current phase: **Phase 3 implemented; AssemblyAI validated on the
synthetic smoke corpus** (Soniox and the real recorded corpus still pending). Next: record the
real corpus, then Phase 4 (intent classification).

## Environment

| Item          | Value                                                                                        |
| ------------- | -------------------------------------------------------------------------------------------- |
| OS            | Windows 11 Home 10.0.26300 (x64), display scale 200 %                                        |
| Node / pnpm   | 22.14.0 / 10.12.1                                                                            |
| Rust          | 1.99.1 stable-x86_64-pc-windows-msvc (Rustup 1.29.1, installed via winget on 2026-10-28)     |
| C++ toolchain | Visual Studio 2022 Build Tools 17.14 — VCTools workload (installed via winget on 2026-10-28) |
| WebView2      | 156.0.4314.50                                                                                |
| Repository    | Started from an empty folder; `git init` done, **no commits yet**, no remote                 |

`cargo` is installed in `%USERPROFILE%\.cargo\bin`. Shells opened before the install may need
`PATH` refreshed (open a new terminal).

### Pinned versions and why

- Tauri 2.12 (crate 2.12.5, CLI/API 2.12.4), plugins `global-shortcut` 2.4.1, `window-state` 2.5.0.
- Vue 3.5.44, Pinia 4.1.0 (setup stores), Vite 8.3.5, Vitest 5.0.5, zod 4.6.7.
- **TypeScript 6.0.3**, not 7.x: `typescript-eslint` supports `<6.1`, and the official
  `create-tauri-app` vue-ts template pins `~6.0`.
- **jsdom 29.1**, not 30.x: jsdom 30 needs Node ≥ 22.22 (its undici 8 crashes on Node 22.14).
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
```

## How to run

```text
pnpm install
pnpm tauri dev      # native overlay; Démarrer (or Ctrl+Shift+P) starts live capture
pnpm dev            # browser only, http://localhost:1420 (demo mode; Tauri calls are no-ops)
pnpm check          # lint + typecheck + tests
pnpm test:rust      # Rust unit tests (hardware tests are opt-in, see AUDIO_PIPELINE.md)
```

## Next concrete step

1. **You:** move the AssemblyAI account to a paid plan and opt out of model training before
   streaming real conversations. So far only synthetic audio has been sent. Optionally, create
   the Soniox EU project for the fallback comparison.
2. Record the real corpus: about 150 utterances over loopback and microphone, every speaker
   consenting, with hesitations and accents. Re-run the harness to tune the fast endpoint
   (hesitations versus latency) and to compare Soniox.
3. Phase 4: intent classification on `transcript.final` segments (two-stage, structured JSON,
   follow-up context, duplicate and debounce logic), replacing the mock question detection.
