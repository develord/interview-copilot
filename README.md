# Interview Copilot

Real-time desktop copilot for technical and professional conversations. It listens to an
authorized audio source, detects questions (French, English and mixed French-English technical
speech), and streams concise answers into a stable floating overlay. Every question gets its own
card, and answers never overwrite each other.

**Status:** Phase 3. Native audio capture plus streaming transcription (AssemblyAI or Soniox,
EU endpoints), with a live transcript panel. Question detection and answers are still simulated
(Phase 4 and 5). See [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md).

## Requirements (Windows)

- Node 22 and pnpm 10
- Rust stable (MSVC) and the Visual Studio 2022 C++ Build Tools
- WebView2 (included with Windows 11)

## Commands

| Command            | What it does                                                               |
| ------------------ | -------------------------------------------------------------------------- |
| `pnpm install`     | Install dependencies                                                       |
| `pnpm tauri dev`   | Run the native overlay (always-on-top, global shortcuts)                   |
| `pnpm dev`         | Run the UI in a browser at http://localhost:1420 (native calls are no-ops) |
| `pnpm check`       | Lint + type-check + unit/component tests                                   |
| `pnpm test:rust`   | Rust unit tests                                                            |
| `pnpm tauri build` | Build the Windows installer (NSIS)                                         |

Press **Démarrer** (or `Ctrl+Shift+P`) to start the session. In the desktop app this captures
**system audio** by default (the microphone is off until you enable it in Réglages; nothing is
recorded to disk). Switch to **Démo** in Réglages to replay the scripted interview instead.

**Transcription:** in Réglages → Audio, enable "Transcrire en direct", pick a provider and save
its API key (stored in the Windows Credential Manager). The benchmark harness and setup are in
[docs/TRANSCRIPTION.md](docs/TRANSCRIPTION.md).

Hardware smoke tests (they open the microphone for about 2 s and play a quiet tone) are opt-in:

```bash
cargo test --manifest-path src-tauri/Cargo.toml hardware -- --ignored --nocapture --test-threads=1
```

## Shortcuts

| Shortcut           | Action                                                 |
| ------------------ | ------------------------------------------------------ |
| `Ctrl+Shift+Space` | Show / hide the overlay (global)                       |
| `Ctrl+Shift+P`     | Start, pause or resume the session (global)            |
| `Ctrl+Shift+Enter` | Focus the manual question field, or submit it (global) |
| `Ctrl+Shift+↑`     | Enlarge the active answer (global)                     |
| `Esc`              | Close the enlarged answer or the settings              |

If another application already owns a shortcut, `Ctrl+Alt+…` is used instead. The settings panel
shows which bindings are active.

## Layout

```
src/
  domain/        event contracts (zod), card lifecycle state machine, rich-text parsing
  stores/        Pinia: session (validated dispatch) and overlay settings
  services/      SessionController contract; mock pipeline (Phase 1)
  components/    overlay (header, active question, footer, settings) and cards
  composables/   sticky scroll, shortcuts
  platform/      Tauri window wrapper and window geometry
src-tauri/       Rust: global shortcuts, window state, (later) audio and providers
docs/            PRD, architecture, UI state machine, implementation status
```
