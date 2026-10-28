# Audio pipeline

Status: Phase 2 done; since Phase 3, frames go to an `SttSink` when transcription is enabled
(see [TRANSCRIPTION.md](TRANSCRIPTION.md)), and to a `DiscardSink` otherwise.

## Flow

```
 WASAPI device ──(real-time thread)──► data callback ──► rtrb ring buffer (lock-free, ~2 s)
                                         │                    │
                                         └─ errors ─► mpsc ──►│
                                                              ▼
                                                     worker thread (one per source)
                                   downmix → FFT resample to 16 kHz → level meter
                                   → 20 ms PCM16 frames → FrameSink (Phase 3: STT)
                                                              │ snapshot
                                                              ▼
                                       reporter thread (100 ms) ─► `pipeline:event`
                                       audio.diagnostics + derived session.status
```

| Piece                            | File                                                                       | Responsibility                                                                                                                               |
| -------------------------------- | -------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `CaptureBackend` / `CpalBackend` | `src-tauri/src/audio/backend.rs`                                           | Device enumeration and opening a paused stream. An output device opened as input is WASAPI loopback. cpal errors are mapped to stable codes. |
| Worker                           | `src-tauri/src/audio/worker.rs`                                            | Owns the stream on its own thread: play/pause/mute, draining, conversion, metering, recovery                                                 |
| Converter                        | `src-tauri/src/audio/convert.rs`                                           | Downmix, rubato `Fft` resampler (band-limited), PCM16, 20 ms frames                                                                          |
| Engine                           | `src-tauri/src/audio/engine.rs`                                            | Session lifecycle across sources, 100 ms diagnostics, session status derivation                                                              |
| Commands                         | `src-tauri/src/audio/commands.rs`                                          | `audio_list_devices`, `audio_start`, `audio_pause`, `audio_resume`, `audio_set_muted`, `audio_stop`, `open_microphone_privacy_settings`      |
| Contracts                        | `src-tauri/src/events.rs` ↔ `src/domain/audio.ts`, `src/domain/events.ts`  | Checked from both sides against `contracts/fixtures/*.json`                                                                                  |
| Live controller                  | `src/services/liveAudioSession.ts`, `sessionRouter.ts`, `createSession.ts` | Demo/live routing, Rust events into the store, live mute                                                                                     |
| UI                               | `AudioStatusStrip.vue`, `AudioSettings.vue`                                | Level meter and state per source, one-click mute, errors with their fix, source/device selection                                             |

## Sources

- **System audio** (on by default) captures what the computer plays, i.e. the other participants.
  It uses WASAPI loopback on the default render device (or a chosen one) and needs **no
  microphone permission**.
- **Microphone** is off by default. It opens only after the user enables it **and** presses
  Démarrer.
- `deviceId: null` means "system default". In cpal 0.18, default-device streams follow
  default changes where WASAPI allows it. When Windows invalidates the stream instead, the worker
  rebuilds it (see recovery).

## Behaviour under failure

| Condition                                                                         | Code                 | What happens                                                                                                               |
| --------------------------------------------------------------------------------- | -------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| Device unplugged / stream invalidated                                             | `device-lost`        | Stream dropped; reopened with backoff 0.5 → 1 → 2 → 4 → 8 → 16 s (6 attempts). The session shows "Reconnexion…" meanwhile. |
| Device in use by another app                                                      | `device-busy`        | Same retry policy                                                                                                          |
| No device / selected device absent                                                | `device-not-found`   | Same retry policy, then error                                                                                              |
| Microphone blocked in Windows privacy settings                                    | `permission-denied`  | Not retried; the UI offers **Ouvrir les paramètres** (`ms-settings:privacy-microphone`)                                    |
| Unsupported sample format                                                         | `unsupported-format` | Not retried                                                                                                                |
| Nothing enabled at start                                                          | `no-source`          | Start refused; shown in the strip                                                                                          |
| Ring buffer full (worker starved)                                                 | —                    | Whole callback buffers are dropped (never half a frame, which would swap channels); counted in `droppedFrames`             |
| WASAPI discontinuity (`Xrun`), default-device reroute, real-time priority refused | —                    | Informational only; capture continues                                                                                      |

One working source keeps the session `listening`. The session becomes `error` only when every
source has failed.

### Windows specifics worth knowing

- cpal reports `E_ACCESSDENIED` (the microphone privacy block) as a generic backend error, with a
  message in the system language. We detect it by its numeric code (`os error -2147024891`), which
  works on French Windows.
- Loopback delivers **no packets** while nothing is playing. The UI shows "Aucun son joué", not
  an error.
- On the development machine, a 0.05-amplitude test tone came back at about 0.28. Something on
  the output side boosts it, probably a Realtek audio enhancement, since loopback sees the
  processed mix. Meters therefore reflect what is played, not the app's own volume.

## Privacy (PRD §11)

- Capture starts only from an explicit action (Démarrer or Ctrl+Shift+P). Pause, mute and stop
  take effect immediately (the device stream is paused or dropped).
- A red, pulsing dot and the per-source strip are visible whenever capture runs.
- Raw audio is never written anywhere. Frames are metered and discarded until Phase 3 sends them
  to the STT provider.
- On every webview (re)load the app calls `audio_stop`, so a capture can never keep running
  without a UI showing it.

## Tests

- Unit tests (`cargo test`): conversion accuracy, anti-aliasing (a 10 kHz tone doesn't alias to
  6 kHz), streaming consistency, metering, error mapping, worker lifecycle (pause/mute/recovery/
  retry budget/permission), engine status derivation, Rust ↔ TS contract fixtures.
- Hardware smoke tests (ignored by default):

  ```text
  cargo test --manifest-path src-tauri/Cargo.toml hardware -- --ignored --nocapture --test-threads=1
  ```

  These list the real devices, open the microphone for about 2 s, and play a quiet 440 Hz tone
  for 1.5 s to check that loopback captures it.
