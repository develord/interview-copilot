# Architecture

Status: Phase 3 (overlay + native audio capture + streaming STT; questions/answers still simulated). See
[IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md) for what is verified.

## Guiding rule

> The understanding engine decides **when** to answer, the generation engine decides **what** to
> answer, and the UI guarantees that every answer stays readable.

Everything between the pipeline and the screen is a stream of **typed, validated events**. Every
event that touches an answer carries a stable `questionId`, and every streamed token also carries
the `requestId` of the generation that produced it. That single rule is what prevents a slow
answer from overwriting a newer one.

## Layers

```
┌──────────────────────────── Vue 3 overlay (src/) ─────────────────────────────┐
│ components/  Header · ActiveQuestionPanel · AnswerCardList/AnswerCard · Footer │
│              SettingsPanel · FocusView                (render only, no logic)  │
│ composables/ useStickyScroll · useShortcuts                                    │
│ stores/      session (Pinia)  ← dispatch(unknown) — validates, then applies    │
│              overlaySettings (Pinia, persisted, self-healing schema)           │
│ domain/      events.ts (zod contracts) · cardLifecycle.ts (pure state machine) │
│              richText.ts (safe text/code splitting) · types.ts · ids.ts        │
│ services/    SessionRouter → MockPipeline (demo) | LiveAudioSession (Rust)     │
│ platform/    overlayWindow.ts, audioBackend.ts (no-ops outside Tauri)          │
└────────────────────────────────────────────────────────────────────────────────┘
                 ▲ PipelineEvents                     │ commands
                 │                                    ▼
┌────────────────────── Rust / Tauri 2 (src-tauri/) ──────────────────────────────┐
│ shortcuts.rs   global shortcuts (with fallbacks + conflict report)              │
│ window-state   restores size/position per monitor                              │
│ audio/ → stt/  WASAPI loopback + mic → 16 kHz → AssemblyAI | Soniox (EU WSS)   │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Data flow (one question)

1. The pipeline emits `question.detected` → the store validates it, the lifecycle manager creates
   a **new** `pending` card, and the active-question panel receives a **copy** of the question.
2. The generator emits `answer.started { questionId, requestId }` → card becomes `streaming`.
3. `answer.delta { questionId, requestId, seq, section, delta }` events append text to that card
   only. Duplicates (same `seq`) are dropped; out-of-order deltas are buffered and applied in
   order; deltas from any other `requestId` are rejected as stale.
4. `answer.completed` (terminal, immutable) or `answer.failed` (terminal, retryable).
5. A retry is simply a new `answer.started` with a new `requestId`; the old request is retired so
   its late tokens can never leak into the new answer.

The rules are documented in [UI_STATE_MACHINE.md](UI_STATE_MACHINE.md) and enforced by
`src/domain/cardLifecycle.ts`.

## Key decisions

| Decision                                                                                   | Why                                                                                                                                                                                                                |
| ------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Card lifecycle is pure functions over a plain object, wrapped in `reactive()` by the store | Testable without Vue; Vue's fine-grained reactivity means a delta for card A re-renders card A only (verified by a render-count test).                                                                             |
| `store.dispatch(input: unknown)` validates with zod                                        | Events will come from Rust/IPC; malformed or unknown events are counted in diagnostics, never crash the session.                                                                                                   |
| Commands (`SessionController`) and results (events) are separate channels                  | The UI never awaits an answer; it only reacts to events. Swapping the mock for the real orchestrator does not change any component.                                                                                |
| Mock pipeline emits exactly the production event shapes                                    | Phase 1 exercises the same code paths that the real pipeline will use: concurrency, revisions, follow-ups, failures, retries, cancellation.                                                                        |
| Global shortcuts registered in Rust                                                        | They must work while the overlay is hidden; registration conflicts fall back to an alternative binding and are reported to the UI. Escape is local only.                                                           |
| No `v-html` anywhere; answers are parsed into text/code blocks rendered as text nodes      | LLM output is untrusted. Combined with a strict CSP and least-privilege capabilities.                                                                                                                              |
| Single overlay window in Phase 1                                                           | Each Tauri window is a separate JS context. A separate settings/main window would need state sync through Rust; this is deferred until the orchestrator lives in Rust (it then broadcasts events to every window). |
| Memory-only session state                                                                  | Nothing is persisted except overlay preferences (`localStorage`). Transcript/history persistence will be opt-in (PRD §11).                                                                                         |

## Security posture (Phase 1)

- CSP: `default-src 'self' ipc: http://ipc.localhost; style-src 'self' 'unsafe-inline'; img-src 'self' data:`.
- Capabilities (`src-tauri/capabilities/overlay.json`): `core:default` plus only the window setters
  the overlay uses (drag, resize, size, position, always-on-top, minimize, close).
- No credentials exist yet. When providers are added, API keys live in the OS credential store and
  are used from Rust only — the webview never sees them.
- The app does not implement anything intended to hide from screen capture or bypass interview
  platform monitoring (PRD §11).

## What changes in later phases

- **Phase 2 (done):** Rust audio capture behind `CaptureBackend`; `audio.diagnostics` and
  `session.status` events from Rust; demo/live routing. Details: [AUDIO_PIPELINE.md](AUDIO_PIPELINE.md).
- **Phase 3 (done, provider validation pending):** `SttProvider` + `Codec` adapters fed by a
  `FrameSink`; `transcript.partial/final` and `stt.status` events; keys in the Credential
  Manager; consent step. Details: [TRANSCRIPTION.md](TRANSCRIPTION.md).
- **Phase 4–5:** classifier and answer engine. The orchestrator moves to Rust (or a Rust-hosted
  service) and emits `PipelineEvent`s through Tauri events; a `TauriSessionController` replaces
  `MockPipeline` in `src/main.ts`. No component changes are expected.
- **History growth:** the list is not virtualized yet (fine for a session's worth of cards). Add
  virtualization when measured render cost requires it; card keys and scroll policy already
  support it.
