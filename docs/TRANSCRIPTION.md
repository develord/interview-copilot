# Streaming transcription

Status: Phase 3. AssemblyAI is validated end to end on the synthetic smoke corpus (results
below). Soniox has not been run yet (no key). The real, recorded corpus is still to be built.

Provider choice: [reports/Comparatif STT temps réel bilingue.md](../reports/Comparatif%20STT%20temps%20réel%20bilingue.md).
Evaluate **AssemblyAI Universal-3.6 Pro (EU)** first, with **Soniox stt-rt-v5 (EU)** as the
fallback.

## Flow

```
capture worker ──20 ms PCM16──► SttSink (FrameSink, never blocks the worker)
                                   │ bounded queue (5 s)
                                   ▼
                         feeder ── pads silence to real time (loopback sends nothing while
                           │       the computer is silent) · energy VAD · 80 ms chunks
                           │       · ForceEndpoint after 700 ms of silence ("fast endpoint")
                           ▼ bounded queue (~5 s)
                        supervisor ── provider connection (Codec + shared WebSocket loop),
                           │          reconnects 0.5→16 s, drops audio queued while
                           │          disconnected (never replays it)
                           ▼
                          mapper ── transcript.partial / transcript.final with stable ids
                                    `{source}-{connection}-{turn}`, stt.status (state, error,
                                    time-to-final, dropped chunks)
```

Code: `src-tauri/src/stt/` (`session.rs`, `ws.rs`, `assemblyai.rs`, `soniox.rs`, `vad.rs`,
`pacing.rs`, `vocabulary.rs`, `credentials.rs`, `eval.rs`) and `src-tauri/src/bin/stt-bench.rs`.

## Provider adapters (protocols checked against the docs on 2026-10-28)

|                 | AssemblyAI                                                                                                                               | Soniox                                                                                                                                                                |
| --------------- | ---------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Endpoint        | `wss://streaming.eu.assemblyai.com/v3/ws?speech_model=universal-3-6-pro&sample_rate=16000&encoding=pcm_s16le&include_partial_turns=true` | `wss://stt-rt.eu.soniox.com/transcribe-websocket` (needs an EU-region project key)                                                                                    |
| Auth            | `Authorization: <key>` header                                                                                                            | `Authorization: Bearer <key>` header (the `api_key` config field is deprecated and refused from 2027-01-15)                                                           |
| Setup           | `UpdateConfiguration` after `Begin`: `keyterms_prompt` (≤ 100), `agent_context`, `language_codes: ["fr","en"]`                           | start request: `stt-rt-v5`, `pcm_s16le`, 16 kHz, mono, `language_hints`, endpoint detection (level 2, sensitivity 0.3, max 1500 ms), `context.terms` + `context.text` |
| Partial / final | `Turn` with `end_of_turn=false` (all words shown) / `true` (formatted `utterance`)                                                       | final + non-final tokens / segment closed by `<end>` or `<fin>`                                                                                                       |
| Force endpoint  | `{"type":"ForceEndpoint"}`                                                                                                               | `{"type":"finalize"}`                                                                                                                                                 |
| End of stream   | `{"type":"Terminate"}` → `Termination`                                                                                                   | empty text frame → `finished: true`                                                                                                                                   |
| Errors          | Unauthorized and quota are inferred from the close code or reason (v3 close codes aren't documented); everything else is retried         | `error_code`: 401/403 → unauthorized, 402 → quota, 408/413/429/5xx → retried, 400 → protocol                                                                          |

Language: French with English hints. Vocabulary: user terms from the job description first,
then the built-in dictionary in `contracts/vocabulary/technical-terms.json` (~100 terms). It is
only used for biasing and never rewrites transcripts.

## Keys and privacy

- Keys are entered in Réglages → Audio → Transcription. They are stored in the **Windows
  Credential Manager** (`dev.interviewcopilot.desktop/stt`) and read only by Rust when a session
  starts. The webview can store, check for and delete a key, but never read one back.
- Use the provider's EU project or region. On AssemblyAI, a paid plan plus the training opt-out
  is required for zero retention.
- **Consent:** in live mode with system audio enabled, Start (and Ctrl+Shift+P) first shows a
  confirmation that the other party is informed, with a text to read to them. A confirmation
  stays valid for 30 minutes. This reflects art. 226-1 of the Code pénal (capturing _or
  transmitting_ private words without consent); see the report. It is product guidance, not
  legal advice.

## Benchmark harness

```text
powershell -ExecutionPolicy Bypass -File scripts/make-smoke-corpus.ps1   # synthetic 12-utterance smoke test
cd src-tauri
cargo run --features bench --bin stt-bench -- --provider assemblyai --manifest ../corpus/smoke/manifest.json
cargo run --features bench --bin stt-bench -- --provider soniox --manifest ../corpus/smoke/manifest.json
```

- WAV files are replayed in real time through the **same** `session` code as the app, so
  pacing, VAD, the forced endpoint and reconnection all apply. Results land in
  `corpus/**/bench-<provider>.json` (git-ignored).
- **Scores:** normalized WER per category (`mixed`, `fr`, `en`), term recall, and time-to-final
  (local end of speech to the last final) at p50 and p95.
- **Pass/fail thresholds** (from the report): WER mixed ≤ 10 %, FR/EN ≤ 8 %; term recall ≥ 90 %;
  p50 ≤ 600 ms; p95 ≤ 1 000 ms.
- **Options:** `--no-builtin-vocabulary` (baseline without keyterms), `--no-fast-endpoint`
  (provider-only endpointing), `--endpoint <wss-url>`.
- **Manifest format:** `{ "utterances": [{ "id", "file", "category", "reference", "terms": [] }] }`.
  The real corpus (~150 utterances recorded over loopback and microphone, every speaker
  consenting) uses the same format.

## First results — AssemblyAI Universal-3.6 Pro, EU endpoint (2026-10-28)

Smoke corpus: 12 synthetic utterances (Windows TTS; 6 French with English terms, 3 French, 3
English), streamed in one session from France. Synthetic speech is much cleaner than a real call,
so these numbers check the plumbing and the latency mechanics. They are not an accuracy verdict.

| Fast endpoint        | WER mixed | WER FR | WER EN | Term recall | Time-to-final p50 | p95    |
| -------------------- | --------- | ------ | ------ | ----------- | ----------------- | ------ |
| 400 ms               | 1.6 %     | 0 %    | 0 %    | 93 %        | 602 ms            | 694 ms |
| 500 ms               | 1.6 %     | 0 %    | 0 %    | 93 %        | 635 ms            | 892 ms |
| **700 ms (default)** | 1.6 %     | 0 %    | 0 %    | 93 %        | 613 ms            | 994 ms |

- **Only error:** "server side rendering" was transcribed as "serveur-side rendering" (a French
  voice reading English). Every other utterance was word-for-word correct, including
  `Promise.allSettled`, `CI/CD`, `GitHub Actions` and `PostgreSQL`.
- **Latency:** the median hardly moves with the fast endpoint, because AssemblyAI usually closes
  the turn itself about 600 ms after speech ends; the local endpoint trims the tail. 700 ms stays
  the default (it passes p95 ≤ 1 s). A lower value could split real questions at hesitations,
  which TTS does not produce. Tune it on the real corpus.
- **Not met:** p50 ≤ 600 ms is just missed (602–635 ms). Each configuration was run once, so
  variance is unknown.

### Problems found by running against the real service (all fixed)

1. **Undersized audio message (product bug).** The partial chunk flushed before `ForceEndpoint`
   could be 10 ms; AssemblyAI closed the session ("Input Duration Violation: 10.0 ms. Expected
   between 20 and 1000 ms") and the utterance was lost. Partial chunks are now padded with
   silence to a full 80 ms (regression test
   `every_audio_message_is_a_full_80_ms_chunk_even_around_forced_endpoints`).
2. **Connection rate limit.** Free AssemblyAI accounts allow about 5 new streaming connections
   per minute ([pricing](https://www.assemblyai.com/pricing),
   [support](https://support.assemblyai.com/articles/1700416323-what-are-my-concurrency-limits));
   the refusal reads "Unauthorized Connection: Too many concurrent sessions". The first harness
   opened one connection per utterance and lost several. It now streams the whole corpus through
   one session, as the app does. The codec now classifies that message as a retryable limit,
   distinct from a bad key or an empty balance (regression test).
3. **Corpus encoding.** The generator script lacked a UTF-8 BOM, so Windows PowerShell 5.1
   garbled the accented references and the synthesized speech. It is now saved with a BOM, and
   synthesized text (`say`) is separate from the reference.
4. **The WebSocket is now closed with a close frame** after the provider's final message, so the
   server releases the session at once.

### Keys: launch the app yourself

A key saved in an app instance started _by the agent's tools_ lands in that sandbox's isolated
credential view and is lost. With the app started from your own terminal, the key persisted
across restarts. `stt_set_key` now reads the key back right after writing it and reports an
error if the store did not keep it.

### Before real interviews

The account used for these runs has the free-tier limits. AssemblyAI only lets **paid** accounts
opt out of model training, so upgrade and opt out before streaming real conversations (see the
STT comparison report). So far only synthetic TTS audio has been sent.
