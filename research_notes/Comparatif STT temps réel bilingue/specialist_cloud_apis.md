# Specialist cloud streaming STT APIs (Deepgram, AssemblyAI, Speechmatics, Gladia, Soniox) for a French-first, FR/EN code-switching interview copilot. State as of 2026-10-28

Notes compiled 2026-10-28. Most facts come from official docs and pricing pages fetched that day. Many doc pages carry no date. Where a page shows a date, it is given. Items marked "(via search summary)" were seen only in search-engine summaries, not on a fetched page. Treat them as lower confidence. Vendor benchmark numbers are marketing claims unless noted otherwise.

Use-case reminders: French main language with English tech terms inside the same sentence, plus fully English questions. Input is 16 kHz mono PCM16 in 20 ms frames from two sources (loopback = interviewer, mic = candidate). Target is about 2 s from end of question to useful answer, so time-to-final and endpointing matter most.

---

## Q1. Does real-time streaming support French, and FR+EN code-switching within one stream/utterance? Documented limitations?

### Takeaway

All five providers stream French. Four document intra-sentence FR/EN code-switching in streaming:

- Deepgram Flux Multilingual and Nova-3 `multi`
- AssemblyAI Universal-3.6 Pro Realtime
- Soniox stt-rt-v5
- Speechmatics Melia 1 (Realtime only since 2026-10-13, with big feature gaps)

Gladia's live `code_switching` is documented as per-utterance language detection. Speechmatics Enhanced/Standard have no French-English bilingual pack.

### At-a-glance matrix (details and sources in the bullets below)

| Provider / model (streaming)                                           | French                                                | FR+EN intra-utterance switching                     | Price / hr (list)                   | EU endpoint                   | Short-lived client tokens                                | Official Rust SDK                              |
| ---------------------------------------------------------------------- | ----------------------------------------------------- | --------------------------------------------------- | ----------------------------------- | ----------------------------- | -------------------------------------------------------- | ---------------------------------------------- |
| Deepgram Flux Multilingual `flux-general-multi` (GA 2026-04-15)        | Yes (1 of 10 langs)                                   | Yes, documented "native"                            | $0.468 PAYG / $0.408 Growth (promo) | `api.eu.deepgram.com`         | Yes, JWT via `/v1/auth/grant` (30 s default, max 3600 s) | Community crate `deepgram` 0.14.0 (2026-10-20) |
| Deepgram Nova-3 `language=multi`                                       | Yes (listed)                                          | Yes (`language=multi`)                              | $0.348 PAYG / $0.30 Growth (promo)  | same                          | same                                                     | same                                           |
| AssemblyAI Universal-3.6 Pro Realtime `universal-3-6-pro` (2026-09-29) | Yes (1 of 32)                                         | Yes, "native in a single stream"                    | $0.45                               | `streaming.eu.assemblyai.com` | Yes, `/v3/token` (1–600 s)                               | Not found                                      |
| AssemblyAI `universal-streaming-multilingual`                          | Yes (1 of 6)                                          | Per turn only, not intra-utterance                  | $0.15                               | same                          | same                                                     | Not found                                      |
| Speechmatics Enhanced/Standard RT                                      | Yes (fixed `fr`)                                      | No FR-EN pack                                       | $0.80 / $0.45                       | `eu.rt.speechmatics.com`      | Yes, temp JWT (60–86400 s)                               | `speechmatics-rs` (official org, WIP)          |
| Speechmatics Melia 1 RT (2026-10-13)                                   | Implied (55+ langs, French not named in sources read) | Yes ("code-switching across 55+ languages")         | RT price not found                  | same                          | same                                                     | same (Melia support unverified)                |
| Gladia Solaria-1 live                                                  | Yes                                                   | `code_switching` = per-utterance language detection | €0.68                               | `region=eu-west`              | Init call returns a session WebSocket URL with `token`   | Not found                                      |
| Soniox stt-rt-v5                                                       | Yes (60+ langs)                                       | Yes, "mixed within a single sentence"               | $0.12                               | `stt-rt.eu.soniox.com`        | Yes, temporary API key                                   | Not found                                      |

### Cited Findings

**Deepgram**

- Flux Multilingual (`flux-general-multi`) covers 10 languages, including `fr`: "English, Spanish, French, German, Hindi, Russian, Portuguese, Japanese, Italian, and Dutch". `flux-general-en` is English-only. — [Deepgram Models & Languages Overview](https://developers.deepgram.com/docs/models-languages-overview)
- Flux Multilingual "handles code-switching natively" (`model=flux-general-multi` on `/v2/listen`). Optional `language_hint` biases detection but does not restrict it. Detected languages come back per `TurnInfo` in a `languages` field. — [Deepgram Multilingual Code-Switching docs](https://developers.deepgram.com/docs/multilingual-code-switching)
- Flux Multilingual GA is dated 2026-04-15 in the changelog. The press release is dated 2026-04-29 and says it handles mid-sentence switches without reconnection. — [Deepgram changelog 2026-04-15](https://developers.deepgram.com/changelog/2026/4/15); [BusinessWire 2026-04-29](https://secure.businesswire.com/news/home/20260429043418/en/Deepgram-Launches-Flux-Multilingual-The-Worlds-First-Multilingual-Conversational-Speech-Recognition-Model)
- Nova-3 code-switching uses `language=multi` and works in streaming over WebSocket. The page tags it "Specific languages only" and says "Nova-3 multilingual supports 55 languages". For code-switching it recommends `endpointing=100`. Nova-3 accepts repeated `language_hint` params (ignored unless `language=multi`). Response fields include `languages`, `languages_hinted` and per-word `words[].language`. — [Deepgram Multilingual Code-Switching docs](https://developers.deepgram.com/docs/multilingual-code-switching)
- The models overview lists French in the Nova-3 multilingual row and in the monolingual row (`fr`, `fr-CA`). Nova-2 `multi` is Spanish+English only. — [Deepgram Models & Languages Overview](https://developers.deepgram.com/docs/models-languages-overview)
- Self-hosted availability of `flux-general-multi` is reported inconsistently across secondary sources (via search summary). Not relevant for cloud use. — [implicator.ai](https://www.implicator.ai/deepgram-launches-flux-multilingual-speech-model-with-10-language-mid-call-switching/)

**AssemblyAI**

- `universal-3-6-pro` (Universal-3.6 Pro Realtime) supports 32 languages, French listed. Code-switching is "Native across its supported languages in a single stream". Launched September 2026. — [AssemblyAI Models reference (llms/models.md)](https://www.assemblyai.com/llms/models.md)
- The launch post is dated 2026-09-29: "serves 32 languages from one model with automatic language detection", same latency as 3.5 Pro, $0.45/hr. The post itself does not list all 32 languages. — [AssemblyAI blog: Universal-3.6 Pro Realtime](https://www.assemblyai.com/blog/universal-3-6-pro-realtime)
- `universal-streaming-multilingual` covers 6 languages: English, Spanish, German, French, Portuguese, Italian. Code-switching is per turn, "not intra-utterance like `universal-3-6-pro`". `universal-streaming-english` is English only. — [AssemblyAI Models reference](https://www.assemblyai.com/llms/models.md)
- The streaming API reference lists `speech_model` values `universal-3-6-pro` (default), `universal-3-5-pro`, `universal-streaming-english` and `universal-streaming-multilingual`. Other parameters: `language_detection` (3.6 Pro, 3.5 Pro, Multilingual) and `language_codes` (steers output toward the listed languages). — [AssemblyAI Streaming API reference](https://www.assemblyai.com/docs/api-reference/streaming-api/streaming-api)
- Doc inconsistency: one FAQ line in the models reference says 3.6 "covers 18 languages", which conflicts with the "32 total" figure elsewhere. — [AssemblyAI Models reference](https://www.assemblyai.com/llms/models.md)

**Speechmatics**

- French (`fr`, "Global French") is a supported transcription language. No French-English bilingual pack exists. The packs are `ar_en`, `en_ms`, `cmn_en`, `cmn_en_ms_ta`, `en_ta`, `tl` and Spanish-English (`es` + `domain: bilingual-en`). Language `auto` is "Currently supported with Batch transcription only". — [Speechmatics Languages docs](https://docs.speechmatics.com/speech-to-text/languages)
- Melia 1 is "automatic multilingual transcription for 55+ languages" and handles mid-conversation switching. You select it in a Realtime `StartRecognition` with `"model": "melia-1"` and `"language": "multi"`. — [Speechmatics Models docs](https://docs.speechmatics.com/speech-to-text/models)
- Changelog 2026-10-13 (Realtime SaaS): "Melia 1 ... is now available in Realtime" in the US, EU and AU. — [Speechmatics release notes](https://speechmatics.featurebase.app/en/changelog)
- Melia 1 Realtime does not yet support:
  - speaker diarization or identification (channel diarization only)
  - custom dictionary
  - silence-based end of utterance ("rejects `end_of_utterance_silence_trigger`")
  - `max_delay` / `max_delay_mode`
  - translation and audio events

  The client must detect end of speech and send `ForceEndOfUtterance`. — [Speechmatics Models docs](https://docs.speechmatics.com/speech-to-text/models); [Speechmatics Turn detection docs](https://docs.speechmatics.com/speech-to-text/realtime/turn-detection)

- The Melia launch article (2026-06-17) said batch-only preview, "Real-time is on the way". It also says "When the lowest possible word error rate is what matters, Enhanced is still the model to choose", and that Melia accuracy is "on par with Standard". This article is now outdated on RT availability. — [Speechmatics: Introducing Melia](https://speechmatics.com/company/articles-and-news/introducing-melia-multilingual-speech-to-text-model); [Speechmatics Models docs](https://docs.speechmatics.com/speech-to-text/models)

**Gladia**

- Live transcription supports only `solaria-1`. The example config is `languages: ["fr"]`, `code_switching: false`. — [Gladia Live STT getting started](https://docs.gladia.io/chapters/live-stt/getting-started)
- The `language_config.code_switching` default is `false`: "When true, language is detected per utterance. Ignored if one language is set." `languages` defaults to `[]` (auto-detect). — [Gladia POST /v2/live API reference](https://docs.gladia.io/api-reference/v2/live/init)
- Gladia marketing describes Solaria-1 as having "native code-switching" across 100+ languages (via search summary, vendor blog). — [Gladia blog](https://www.gladia.io/blog/code-switching-in-contact-centers-why-customer-calls-fail-transcription)

**Soniox**

- French (`fr`) is supported. "All languages are available in both" Real-time and Async. — [Soniox Supported languages](https://soniox.com/docs/stt/concepts/supported-languages)
- The model "handles multilingual speech seamlessly, even when multiple languages are mixed within a single sentence or conversation". `language_hints` bias the model but "do not restrict recognition". A separate "Language restrictions" feature exists but was not read. — [Soniox Language hints](https://soniox.com/docs/stt/concepts/language-hints)
- The current RT model is `stt-rt-v5`. stt-rt-v4 retired 2026-06-30 and v4 requests now route to v5 (secondary source). Optional `enable_language_identification` adds language tags. — [Soniox Real-time transcription](https://soniox.com/docs/stt/rt/real-time-transcription); [voiceaispace.com article on Soniox v5](https://www.voiceaispace.com/articles/soniox-v5-real-time-follows-a-live-conversation-not-just-the-words)

### Inferences

- The best documented fits for "French sentence with English words inside" are Deepgram `flux-general-multi` (or Nova-3 `multi`), AssemblyAI `universal-3-6-pro` and Soniox `stt-rt-v5`. All three describe word-level or intra-sentence switching. Speechmatics Melia 1 RT also claims it, but has been live for only about two weeks and lacks custom dictionary and server-side endpointing.
- Gladia's per-utterance language detection is a risk for this use case. A French utterance with English nouns would likely be tagged French as a whole. Whether English terms survive is undocumented, so test it.
- Speechmatics Enhanced with fixed `fr` is a fallback. It relies on the French model (plus custom dictionary) to render English loanwords, which is not true code-switching.
- With a fixed-language model, fully English questions would degrade. That argues for an auto/multi mode, or a per-session language choice.

### Gaps

- The official list of all 32 `universal-3-6-pro` languages was not read directly (the models reference says French is included).
- The full language list for Deepgram Nova-3 `multi` (55 languages) was not read. French is in the multilingual row of the overview page.
- No explicit statement was found on whether Speechmatics Melia 1 lists French by name. "55+ languages" and "all the languages we support" imply it.
- Gladia: no doc found describing intra-utterance switching behavior for live.

---

## Q2. Streaming protocol, partial vs final messages, endpointing / turn detection options and timeouts, and reported latency

### Takeaway

All five use WebSocket with JSON text messages and binary audio.

Fastest documented turn-end signals:

- Deepgram Flux: about 260 ms end-of-turn (claim). `eot_threshold` plus `eot_timeout_ms` (500–60000, default 5000).
- AssemblyAI U3.6 Pro: median end-of-turn 537 ms (vendor). Controlled by `min_turn_silence` and `max_turn_silence`, plus a `ForceEndpoint` message.
- Soniox: `<end>` token, `max_endpoint_delay_ms` 500–3000 (default 2000), plus manual `finalize`.
- Gladia: `endpointing` 0.01–10 s (default 0.05 s).
- Speechmatics Enhanced/Standard: `end_of_utterance_silence_trigger` 0–2 s. Melia 1 requires client-side VAD plus `ForceEndOfUtterance`.

### Cited Findings

**Deepgram, Flux (`/v2/listen`)**

- URL `wss://api.deepgram.com/v2/listen?model=flux-general-en` (or `flux-general-multi`). "Using `/v1/listen` will not work with Flux." — [Deepgram Flux quickstart](https://developers.deepgram.com/docs/flux/quickstart)
- Turn parameters: `eot_threshold` 0.5–1.0 (default 0.7), `eager_eot_threshold` 0.3–0.9 (default none), `eot_timeout_ms` 500–60000 (default 5000). They are changeable mid-stream with `Configure`. Setting `eot_threshold=1.0` disables natural end-of-turn, so turns end only via `ForceEndTurn`. — [Deepgram Flux quickstart](https://developers.deepgram.com/docs/flux/quickstart)
- Events: `Connected`, `TurnInfo` (transcripts and words), `EagerEndOfTurn` / `TurnResumed` (only with `eager_eot_threshold`) and `EndOfTurn`. — [Deepgram Flux quickstart](https://developers.deepgram.com/docs/flux/quickstart)
- Latency claim: "~260ms end-of-turn detection". Using EagerEndOfTurn can raise LLM calls by 50–70%. — [Deepgram Flux quickstart](https://developers.deepgram.com/docs/flux/quickstart)

**Deepgram, Nova-3 (`/v1/listen`)**

- `interim_results` gives partials. `speech_final=true` marks a VAD endpoint after a silence set by `endpointing`.
- `UtteranceEnd` (via `utterance_end_ms`, "1000 ms or higher", requires `interim_results=true`) is based on word timings and ignores noise. The message looks like `{"type":"UtteranceEnd","channel":[0,2],"last_word_end":3.1}`.
- Recommended logic: act on `speech_final=true`, and fall back to UtteranceEnd if no `speech_final` arrives.

  — [Deepgram end-of-speech detection](https://developers.deepgram.com/docs/understanding-end-of-speech-detection)

- Endpointing is "enabled by default and set to 10 milliseconds", is an integer in ms (examples 300 and 500), and `endpointing=false` disables it. — [Deepgram Endpointing](https://developers.deepgram.com/docs/endpointing)
- For code-switching (`language=multi`) the recommended value is `endpointing=100`. — [Deepgram Multilingual Code-Switching](https://developers.deepgram.com/docs/multilingual-code-switching)

**AssemblyAI (v3 streaming)**

- WebSocket `wss://streaming.assemblyai.com/v3/ws`. Server messages: `Begin`, `Turn` (with `transcript`, `end_of_turn`, `turn_is_formatted`, `end_of_turn_confidence`, `words`), `Termination`. Client sends `Terminate` to close. "Expect several partial updates before each `end_of_turn: true`." — [AssemblyAI Universal Streaming quickstart](https://www.assemblyai.com/docs/speech-to-text/universal-streaming)
- Turn parameters:
  - `min_turn_silence`: 50–10000 ms. Default 400 ms on Universal Streaming, mode-dependent on 3.5 Pro.
  - `max_turn_silence`: default 1280 ms on Universal Streaming, 1536 ms on 3.5 Pro (768 ms with `speaker_labels`).
  - `end_of_turn_confidence_threshold`: 0–1, default 0.4 (Universal Streaming only).
  - `mode`: `max_accuracy` / `min_latency` / `balanced` (3.5 Pro only).
  - `vad_threshold`, and `include_partial_turns` (default true).
- Other server messages: `SpeechStarted`, `SpeakerRevision`, `Heartbeat`.
- Client messages: `UpdateConfiguration` (mid-session changes to keyterms, silences, mode, `language_codes` and more), `ForceEndpoint`, `KeepAlive`.

  — [AssemblyAI Streaming API reference](https://www.assemblyai.com/docs/api-reference/streaming-api/streaming-api)

- U3.6 Pro latency (vendor): "P50 ~150ms post-VAD, P90 ~240ms post-VAD", "P50 time-to-complete-transcript ~250ms", "median end-of-turn latency 537 ms". Latency is "slightly higher than Universal-Streaming English at P75/P90/P95". — [AssemblyAI Models reference](https://www.assemblyai.com/llms/models.md)
- Launch post (vendor): median endpoint latency 537 ms, Pipecat benchmark 307 ms median time to first speech, end-of-turn precision 77.9% and recall 92.8%. — [AssemblyAI blog 2026-09-29](https://www.assemblyai.com/blog/universal-3-6-pro-realtime)

**Speechmatics (RT v2)**

- Messages:
  - `AddPartialTranscript` (interim)
  - `AddTranscript` (final)
  - `EndOfUtterance` (with `start_time`, `end_time`, optional `channel`, `forced`)
  - Client `ForceEndOfUtterance`, with optional `timestamp` and `channel`. The server answers `AddTranscript` and `EndOfUtterance` with `forced: true`.

  — [Speechmatics Turn detection](https://docs.speechmatics.com/speech-to-text/realtime/turn-detection)

- `conversation_config.end_of_utterance_silence_trigger`: 0–2 s (0 disables). Recommended 0.5–0.8 s for voice AI and 0.8–1.2 s for dictation. Must be lower than `max_delay`. Supported by Enhanced/Standard, rejected by Melia 1. — [Speechmatics Turn detection](https://docs.speechmatics.com/speech-to-text/realtime/turn-detection)
- `max_delay`: range 0.7–4 s, default 4 s. `max_delay_mode`: `fixed` / `flexible` (default flexible; flexible can exceed `max_delay` when an entity is detected). (Via search summary of the official docs page. An older beta page lists 2–20 s, but the 2024-07-10 changelog lowered the minimum to 0.7.) — [Speechmatics Realtime latency](https://docs.speechmatics.com/features/realtime-latency); [Speechmatics changelog 2024-07-10](https://speechmatics.featurebase.app/changelog/2024-07-10-real-time-saas-4)
- 2026-10-09: "ForceEndOfUtterance can now be sent without a timestamp in external turn detection mode" (Agent STT). 2026-10-27: "Fixed word timings drifting after a forced end of utterance" (Realtime). — [Speechmatics release notes](https://speechmatics.featurebase.app/en/changelog)
- No RT latency figures are given on the turn-detection or models pages. — [Speechmatics Models docs](https://docs.speechmatics.com/speech-to-text/models)

**Gladia (v2 live)**

- Two-step flow:
  1. `POST https://api.gladia.io/v2/live` with header `x-gladia-key` returns `id` and `url` (`wss://api.gladia.io/v2/live?token=...`).
  2. Stream audio. Stop with a `stop_recording` message or close code 1000. Full results via `GET /v2/live/:id`. Sessions are capped at 3 hours.

  — [Gladia Live STT getting started](https://docs.gladia.io/chapters/live-stt/getting-started)

- Partials vs finals: events for the same utterance share `data.id`, and `is_final` distinguishes them. Partials need `messages_config.receive_partial_transcripts: true` (default `false`). — [Gladia Live STT getting started](https://docs.gladia.io/chapters/live-stt/getting-started); [Gladia /v2/live reference](https://docs.gladia.io/api-reference/v2/live/init)
- `endpointing`: default 0.05 s, range 0.01–10 s ("Silence duration that ends an utterance"). `maximum_duration_without_endpointing`: default 5 s, range 5–60 s. `pre_processing.speech_threshold`: default 0.6. — [Gladia /v2/live reference](https://docs.gladia.io/api-reference/v2/live/init)
- Docs give no streaming latency figure ("neither establishes streaming latency"). — [Gladia Live STT getting started](https://docs.gladia.io/chapters/live-stt/getting-started)
- Vendor blogs give conflicting latency claims for Solaria-1: about 270 ms average in one post and 120–180 ms in another (via search summary). — [Gladia blog (ElevenLabs vs Gladia)](https://www.gladia.io/blog/elevenlabs-vs-gladia-speech-to-text-comparison-for-voice-ai-builders); [Gladia blog (contact centers)](https://www.gladia.io/blog/code-switching-in-contact-centers-why-customer-calls-fail-transcription)

**Soniox**

- WebSocket `wss://stt-rt.soniox.com/transcribe-websocket`. The first message is a JSON config (`model`, `audio_format`, `sample_rate`, `num_channels`, `language_hints`, `enable_endpoint_detection`, `enable_speaker_diarization`, `enable_language_identification`, `context`, `translation`).
- Responses stream tokens with `is_final`. Non-final tokens can change. Final tokens "will never change in future responses". Progress fields: `final_audio_proc_ms` and `total_audio_proc_ms`. Manual finalization: `{"type":"finalize"}`.

  — [Soniox Real-time transcription](https://soniox.com/docs/stt/rt/real-time-transcription)

- Endpoint detection returns an always-final `<end>` token per segment. Controls:
  - `max_endpoint_delay_ms`: 500–3000, default 2000
  - `endpoint_latency_adjustment_level`: 0–3, default 0
  - `endpoint_sensitivity`: −1.0 to 1.0, default 0

  Voice-AI starting point: level 2, sensitivity 0.3, max delay 1500 ms. "If speaker diarization matters more than latency", do not use endpoint detection. — [Soniox Endpoint detection](https://soniox.com/docs/stt/rt/endpoint-detection)

- Secondary source: stt-rt-v4 had 1.25% semantic WER and 249 ms median time to final segment on Pipecat's smart-turn dataset (1,000 English samples). No v5 figures were found. — [voiceaispace.com on Soniox v5](https://www.voiceaispace.com/articles/soniox-v5-real-time-follows-a-live-conversation-not-just-the-words)

### Inferences

- For the 2 s budget, every provider can produce a final segment within about 0.3–1.5 s of speech end if tuned:
  - Deepgram Flux `eot_threshold` around 0.7 with a lower `eot_timeout_ms` (e.g. 1000–1500 as silence fallback), or Nova-3 `endpointing=100–300` plus `utterance_end_ms=1000`
  - AssemblyAI `min_turn_silence` / `max_turn_silence` lowered
  - Soniox `max_endpoint_delay_ms=500–1500` plus latency level 2
  - Speechmatics silence trigger 0.5–0.8 s
  - Gladia endpointing 0.3–0.8 s (the 0.05 s default will over-segment natural French pauses)

  Remaining budget then goes to the LLM.

- Interview questions contain hesitation pauses, so a purely silence-based endpoint will split questions. Semantic or confidence-based turn models (Flux EOT, AssemblyAI turn confidence, Soniox endpoint model) are better suited. The app can also send manual finalize/force-endpoint messages from its own VAD or hotkey (Deepgram `Finalize`/`ForceEndTurn`, AssemblyAI `ForceEndpoint`, Soniox `finalize`, Speechmatics `ForceEndOfUtterance`).
- Almost all published latency figures are vendor-run and English-centric. None of the providers publish French-specific latency.

### Gaps

- Deepgram Flux Multilingual-specific latency vs Flux English: not found.
- AssemblyAI `universal-3-6-pro` default values for `min_turn_silence` and `max_turn_silence`: the reference gives 3.5 Pro and Universal Streaming defaults only.
- Speechmatics: no official RT latency numbers found. Melia 1 RT latency not found.
- Gladia: no doc-level latency figures. Vendor blog numbers conflict.
- Deepgram `Finalize` control message for Nova-3 was not read directly this session.

---

## Q3. Accepted input encodings and sample rates (16 kHz linear16?), mono/multichannel, two channels in one stream?

### Takeaway

All five accept raw 16 kHz signed 16-bit little-endian mono PCM. Gladia documents multichannel live (up to 8 channels, billed per channel) and Soniox accepts `num_channels` 2. Speechmatics has RT channel diarization. AssemblyAI documents no multichannel streaming, so plan two sessions. Deepgram Flux docs don't mention multichannel. Deepgram's `UtteranceEnd` message carries a channel index (`[0,2]`), which implies Nova-3 multichannel streaming.

### Cited Findings

- **Deepgram Flux:**
  - Raw encodings: `linear16`, `linear32`, `mulaw`, `alaw`, `opus`, `ogg-opus`, with `encoding` and `sample_rate` required.
  - Sample rates: 8000, 16000, 24000, 44100, 48000. 16000 is recommended for raw audio.
  - "80ms audio chunks strongly recommended" (2560 bytes at 16 kHz linear16).

  — [Deepgram Flux quickstart](https://developers.deepgram.com/docs/flux/quickstart)

- **Deepgram Nova-3:** the `UtteranceEnd` message includes `channel: [A,B]` (channel index and total channels), which indicates multichannel streaming on `/v1/listen`. — [Deepgram end-of-speech detection](https://developers.deepgram.com/docs/understanding-end-of-speech-detection)
- **AssemblyAI:** `encoding` is `pcm_s16le` (default), `pcm_mulaw`, `opus`, `ogg_opus` or `aac`. `sample_rate` is 8000–96000 (default 16000). Multichannel is not mentioned. — [AssemblyAI Streaming API reference](https://www.assemblyai.com/docs/api-reference/streaming-api/streaming-api)
- **AssemblyAI** quickstart: "mono 16-bit PCM by default". — [AssemblyAI quickstart](https://www.assemblyai.com/docs/speech-to-text/universal-streaming)
- **Speechmatics RT:** raw `type: "raw"` with encoding `pcm_f32le`, `pcm_s16le` or `mulaw` plus integer `sample_rate` (examples 44100, 16000). File formats are also accepted. Send audio after `RecognitionStarted`. — [Speechmatics RT input](https://docs.speechmatics.com/speech-to-text/realtime/input)
- **Speechmatics:** the `EndOfUtterance` metadata has `channel` "(multichannel only)", and Melia 1 RT supports "channel diarization only". This implies multichannel RT exists. — [Speechmatics Turn detection](https://docs.speechmatics.com/speech-to-text/realtime/turn-detection); [Speechmatics Models](https://docs.speechmatics.com/speech-to-text/models)
- **Gladia:**
  - `encoding` is `wav/pcm` (default), `wav/alaw` or `wav/ulaw`. `bit_depth` is 8/16/24/32 (default 16).
  - `sample_rate` is 8000/16000/32000/44100/48000 (default 16000). `channels` is 1–8 (default 1). WAV headers are not required.

  — [Gladia /v2/live reference](https://docs.gladia.io/api-reference/v2/live/init)

- **Gladia:** "Live sessions support up to 8 channels". Billing is total duration × number of channels. Each transcript has a `channel` field mapped to input track order. — [Gladia Live STT getting started](https://docs.gladia.io/chapters/live-stt/getting-started)
- **Soniox:** raw formats need `audio_format`, `sample_rate` and `num_channels`. The example is `pcm_s16le`, 16000, 1. `num_channels` accepts 1 or 2. Containers (aac, flac, mp3, ogg, wav, webm…) are accepted with `"audio_format":"auto"`. — [Soniox Real-time transcription](https://soniox.com/docs/stt/rt/real-time-transcription)

### Inferences

- The 20 ms PCM16 frames can be sent directly. Batching to 40–100 ms per WebSocket message reduces overhead (80 ms is required-ish for Deepgram Flux).
- For the two sources, the safest portable design is two independent WebSocket sessions (interviewer loopback and candidate mic). That works for every provider, lets each stream have its own endpointing, and attributes speakers without diarization.
- Multichannel in one stream (Gladia, Speechmatics, Soniox, Deepgram Nova-3) is usually billed per channel too, so it saves no money. It mainly saves a connection.
- The candidate mic stream could use a cheaper model, or be transcribed only when needed, because the interviewer stream drives answer generation.

### Gaps

- Deepgram: official multichannel streaming parameters (`multichannel`, `channels`) and Flux multichannel support were not read directly this session.
- Soniox: semantics of `num_channels: 2` (separate per-channel transcripts vs downmix) not documented in pages read.
- AssemblyAI: no statement found on multichannel streaming (absence, not a confirmed "no").
- Speechmatics: exact RT multichannel config (`channel_diarization_labels`) not read.

---

## Q4. Custom vocabulary / keyterm prompting / boosting: limits, French streaming?

### Takeaway

- Deepgram keyterms: Nova-3 mono and multi plus Flux, 500-token cap, plain terms, updatable mid-stream, paid add-on.
- AssemblyAI `keyterms_prompt`: up to 100 terms, included on U3.6 Pro and Multilingual, updatable mid-stream.
- Soniox `context`: up to 8,000 tokens, with `terms` and free text.
- Gladia `custom_vocabulary`: with intensity.
- Speechmatics custom dictionary: ≤1000 recommended, 20,000 hard cap. Not available on Melia 1.

None of the providers explicitly states that keyterms work for French streaming, though none excludes it.

### Cited Findings

- **Deepgram:** "Keyterm Prompting is available for both monolingual and multilingual transcription using the Nova-3 Models, as well as Flux". "Key Terms are limited to 500 tokens per request". Advice is to focus on "the most important 20-50 terms". No weights or intensifiers. Mid-stream replacement via `Configure` with a `keyterms` array; over-limit returns `CONFIGURE_KEYTERM_LIMIT_EXCEEDED`. `flux-general-multi` is not mentioned explicitly. — [Deepgram Keyterm Prompting](https://developers.deepgram.com/docs/keyterm)
- **Deepgram** keyterm add-on (streaming): $0.0013/min PAYG, $0.0012/min Growth. — [Deepgram Pricing](https://deepgram.com/pricing)
- **AssemblyAI** `keyterms_prompt`: max 100 terms. `prompt` takes up to 1750 characters (listed as Universal-3.5 Pro only). `agent_context` takes up to 1750 characters (3.6 Pro and 3.5 Pro). All can be updated via `UpdateConfiguration`. — [AssemblyAI Streaming API reference](https://www.assemblyai.com/docs/api-reference/streaming-api/streaming-api)
- **AssemblyAI** pricing: keyterms included for U3.6 Pro Realtime ("up to 100 terms, updatable mid-stream") and for Multilingual. English streaming charges +$0.04/hr. — [AssemblyAI Pricing](https://www.assemblyai.com/pricing)
- **AssemblyAI** Multilingual got keyterms "added Dec 2025". — [AssemblyAI Models reference](https://www.assemblyai.com/llms/models.md)
- **Speechmatics** custom dictionary: "no more than 1000 words or phrases (per job)" recommended. "SaaS on Cloud rejects jobs whose custom dictionary exceeds 20,000 items". Optional `sounds_like`. Available in Batch, Realtime and Agent STT. Languages are not listed. — [Speechmatics Custom dictionary](https://docs.speechmatics.com/speech-to-text/features/custom-dictionary)
- **Speechmatics Melia 1 RT** does not support custom dictionary. — [Speechmatics Models](https://docs.speechmatics.com/speech-to-text/models)
- **Gladia** `realtime_processing.custom_vocabulary` (bool) takes `custom_vocabulary_config.vocabulary` (strings or entry objects) and `default_intensity` (0–1). There is also `custom_spelling`. No count limit is stated. — [Gladia /v2/live reference](https://docs.gladia.io/api-reference/v2/live/init)
- **Gladia** pricing lists custom vocabulary as included on Self-serve. — [Gladia Pricing](https://www.gladia.io/pricing)
- **Soniox** `context`: `general` (key-value), `text` (free text), `terms` (array) and `translation_terms`. "Maximum 8,000 tokens (~10,000 characters)". Over the limit returns `invalid_request`. RT use and mid-stream updates are not stated on that page, though the RT page shows `context` in the config. — [Soniox Context](https://soniox.com/docs/stt/concepts/context); [Soniox Real-time transcription](https://soniox.com/docs/stt/rt/real-time-transcription)

### Inferences

- For an interview copilot, a keyterm list built per job posting (e.g. "Vue", "state management", "Kubernetes", "caching", "backend") is the main lever for English tech-term accuracy inside French speech. Soniox's free-text context (job description, CV) and AssemblyAI's prompt/agent_context allow richer conditioning than a bare term list.
- Mid-stream keyterm updates (Deepgram, AssemblyAI) would let the app adjust terms between interview topics without reconnecting.

### Gaps

- No provider explicitly documents keyterm effectiveness for French or for code-switched streaming.
- Deepgram keyterm support for `flux-general-multi` specifically is unconfirmed.
- Gladia custom vocabulary limits and language behavior are not found.

---

## Q5. Speaker diarization in streaming (French)?

### Takeaway

Streaming diarization exists at:

- AssemblyAI: `speaker_labels`, +$0.12/hr, inline on U3.6 Pro.
- Deepgram: v1 streaming diarizer, +$0.0020/min.
- Soniox: `enable_speaker_diarization`.
- Speechmatics Enhanced/Standard.

It is not available on Speechmatics Melia 1 RT or Gladia live (pricing says diarization is "Async only"). No source documents French-specific diarization quality. With two separate audio sources, the app can mostly avoid diarization.

### Cited Findings

- **Deepgram:** streaming uses the v1 diarizer (`diarize_model=latest` resolves to v1). `diarize_model=v2` is "not supported on streaming". Flux is not mentioned. — [Deepgram Diarization](https://developers.deepgram.com/docs/diarization)
- **Deepgram** diarization add-on (streaming): $0.0020/min PAYG, $0.0017/min Growth. — [Deepgram Pricing](https://deepgram.com/pricing)
- **AssemblyAI:** `speaker_labels` (default false) and `max_speakers` 1–10. `speaker_labels_revision_interval_ms` is 0 or ≥120000. `SpeakerRevision` server messages. — [AssemblyAI Streaming API reference](https://www.assemblyai.com/docs/api-reference/streaming-api/streaming-api)
- **AssemblyAI** pricing: +$0.12/hr on all streaming models that support it. — [AssemblyAI Pricing](https://www.assemblyai.com/pricing)
- **AssemblyAI:** U3.6 Pro diarization is "Inline `speaker_labels: true`. No post-processing required". — [AssemblyAI Models reference](https://www.assemblyai.com/llms/models.md)
- **Speechmatics:** Enhanced/Standard support speaker and channel diarization. Melia 1 RT supports channel diarization only. — [Speechmatics Models](https://docs.speechmatics.com/speech-to-text/models)
- **Speechmatics** pricing lists diarization for both Enhanced and Standard real-time. — [Speechmatics Pricing](https://www.speechmatics.com/pricing)
- **Gladia** pricing comparison marks speaker diarization "Async only". Live multichannel gives per-channel attribution instead. — [Gladia Pricing](https://www.gladia.io/pricing); [Gladia Live getting started](https://docs.gladia.io/chapters/live-stt/getting-started)
- **Soniox:** `enable_speaker_diarization` is an RT config field. The endpoint doc advises against endpoint detection when diarization matters more than latency. A secondary source says v5 speaker labeling was "rebuilt from scratch" (vendor claim). — [Soniox Real-time transcription](https://soniox.com/docs/stt/rt/real-time-transcription); [Soniox Endpoint detection](https://soniox.com/docs/stt/rt/endpoint-detection); [voiceaispace.com](https://www.voiceaispace.com/articles/soniox-v5-real-time-follows-a-live-conversation-not-just-the-words)

### Inferences

- Because loopback (interviewer) and mic (candidate) are separate sources, diarization is unnecessary in the base design. That avoids the add-on cost and the endpoint/diarization trade-off Soniox documents.
- Diarization only matters on the loopback stream when there are multiple interviewers (panel interviews). Even then, the copilot mostly needs "question vs not", not who asked.

### Gaps

- No provider documents French-specific diarization accuracy in streaming.
- Deepgram diarization with Flux (`/v2/listen`) is not documented in the page read.

---

## Q6. Streaming pricing per hour and per minute, free tier, concurrency

### Takeaway

List prices per audio hour, as of 2026-10-28:

| Provider / model        | Per hour | Per minute | Notes                     |
| ----------------------- | -------- | ---------- | ------------------------- |
| Soniox v5               | $0.12    | ≈$0.002    |                           |
| AssemblyAI Multilingual | $0.15    |            |                           |
| Deepgram Nova-3 Multi   | $0.348   | $0.0058    | PAYG, promotional         |
| AssemblyAI U3.6 Pro     | $0.45    |            | Billed on connection time |
| Speechmatics Standard   | $0.45    |            |                           |
| Deepgram Flux Multi     | $0.468   | $0.0078    | PAYG, promotional         |
| Gladia                  | €0.68    | €0.0113    |                           |
| Speechmatics Enhanced   | $0.80    |            |                           |

Free credits: Deepgram $200, Speechmatics $100, AssemblyAI $50, Gladia €50.

Concurrency:

- Deepgram: 150 PAYG
- Speechmatics: 50 (2 on free)
- Gladia: 30
- Soniox: 10 by default, raisable
- AssemblyAI: limited by new-stream rate (5/min free, 100/min PAYG)

### Cited Findings

- **Deepgram** streaming per minute:

  | Model               | PAYG    | Growth  |
  | ------------------- | ------- | ------- |
  | Flux English        | $0.0065 | $0.0057 |
  | Flux Multilingual   | $0.0078 | $0.0068 |
  | Nova-3 Monolingual  | $0.0048 | $0.0042 |
  | Nova-3 Multilingual | $0.0058 | $0.0050 |

  These are marked "Limited-time promotional rates on streaming" with no end date. $200 free credit, which doesn't expire until used. WSS concurrency: up to 150 (PAYG) and 225 (Growth). Add-ons as in Q4/Q5. — [Deepgram Pricing](https://deepgram.com/pricing)

- **Deepgram:** Flux Multilingual has the same price as Flux English, per launch coverage (via search summary). The current pricing page shows a slightly higher Multilingual rate ($0.0078 vs $0.0065), so the pricing page is authoritative. — [Deepgram Pricing](https://deepgram.com/pricing); [letsdatascience.com](https://letsdatascience.com/news/deepgram-expands-flux-to-ten-languages-adds-mid-call-switchi-4ef7b7e6)
- **AssemblyAI:** `universal-3-6-pro` $0.45/hr. `universal-streaming-english` $0.15/hr. `universal-streaming-multilingual` $0.15/hr. Diarization +$0.12/hr. Billing is on WebSocket session duration, idle time included. $50 free credits with no card. Free tier: 5 new streams per minute. PAYG: 100 new streams per minute. Enterprise concurrency is higher by contract. — [AssemblyAI Pricing](https://www.assemblyai.com/pricing)
- **AssemblyAI** sessions auto-close after 3 hours. The default limit is 5 new sessions on free accounts. — [AssemblyAI quickstart](https://www.assemblyai.com/docs/speech-to-text/universal-streaming)
- **Speechmatics** real-time:

  | Model    | List     | With model-training discount (opt-in, 33% off) |
  | -------- | -------- | ---------------------------------------------- |
  | Enhanced | $0.80/hr | $0.54/hr                                       |
  | Standard | $0.45/hr | $0.30/hr                                       |

  "$100 in free credits, no card required". Concurrency: "50 (2 on Free)" self-serve. Credits last 12 months. — [Speechmatics Pricing](https://www.speechmatics.com/pricing)

- **Speechmatics Melia 1:** "as low as $0.129 per hour, with 10 hours per month free". This is from the batch-era launch article dated 2026-06-17, so the RT price is not confirmed. The "10 h/month free" figure also conflicts with the pricing page's "$100 credits". — [Speechmatics Melia article](https://speechmatics.com/company/articles-and-news/introducing-melia-multilingual-speech-to-text-model)
- **Gladia:** real-time €0.68/hr (€0.0113/min). Self-serve or Enterprise only, no RT volume tiers. €50 credits "valid for one year" (about 73 h of RT). Self-serve concurrency is 30 real-time; over the limit returns 429. — [Gladia Pricing](https://www.gladia.io/pricing)
- **Gladia** multichannel is billed duration × channels. — [Gladia Live getting started](https://docs.gladia.io/chapters/live-stt/getting-started)
- **Soniox:** "Real-time streaming is $0.12/hour". Token pricing: $2/1M input audio tokens and $4/1M output text tokens (1 h audio ≈ 30k input tokens, ≈15k output tokens). Async is $0.10/hr. "Updated June 2026". No free credit is mentioned on the page. — [Soniox Pricing](https://soniox.com/pricing)
- **Soniox** RT limits: 10 concurrent WebSocket connections, 300 minutes per session (fixed), 100 requests/min. Concurrency is raisable in the Console (via search summary). — [Soniox RT limits & quotas](https://soniox.com/docs/stt/rt/limits-and-quotas)

### Inferences

- With two sources streaming for a 1-hour interview (2 stream-hours), list cost per interview is approximately:

  | Model                 | Cost per interview                   |
  | --------------------- | ------------------------------------ |
  | Soniox                | $0.24                                |
  | AssemblyAI U3.6 Pro   | $0.90 (connection time, idle counts) |
  | Deepgram Nova-3 Multi | $0.70 (plus $0.16 if keyterms)       |
  | Deepgram Flux Multi   | $0.94                                |
  | Speechmatics Enhanced | $1.60                                |
  | Gladia                | €1.36                                |

  Using a cheaper model for the candidate mic, or not streaming it continuously, roughly halves this.

- Deepgram's "promotional" streaming rates could rise. Flag this in any cost model.
- Soniox's default 10-connection limit is fine for a single desktop user. It matters only if many users share one key via a backend.

### Gaps

- Soniox free trial credits: not found on the pricing page.
- Speechmatics Melia 1 Realtime price: not found.
- Gladia: whether the live price differs by feature (code-switching, custom vocabulary): not found. It appears flat.

---

## Q7. Data handling: retention, training default and opt-out, EU residency/endpoints, DPA, SOC 2/ISO

### Takeaway

- Most privacy-friendly by default: Soniox (no RT storage, "never used" for training, EU region, SOC 2 Type 2 + ISO 27001) and Speechmatics (RT not stored or trained on unless opt-in, ISO 27001:2022 + SOC 2 Type II, EU endpoint).
- Deepgram trains by default (Model Improvement Program). `mip_opt_out=true` per request gives zero retention, and the EU endpoint is GA.
- AssemblyAI gives zero retention for streaming only if opted out of training. Opting out needs a paid plan. EU streaming endpoint exists.
- Gladia's training default is contradictory between its pages.

### Cited Findings

**Deepgram**

- STT requests (including streaming) are in the Model Improvement Program by default: "Retained to improve our models". Retained data may be processed outside your region. Opt out with `mip_opt_out=true` per request: "Opting out gives you zero data retention". Metadata and usage logs are still kept. A full in-region guarantee "requires both a regional endpoint" and `mip_opt_out=true`. — [Deepgram: Your Data at Deepgram](https://developers.deepgram.com/trust-security/your-data)
- MIP participation is described as voluntary, and opt-out must be added "as a query parameter of all API requests". — [Deepgram MIP docs](https://developers.deepgram.com/docs/the-deepgram-model-improvement-partnership-program)
- Certifications: SOC 2 Type 1 and Type 2 (certificates on request), PCI compliant, "GDPR ready", CCPA, HIPAA BAA on request. ISO is not mentioned. A general DPA is not described on that page (an AU DPA is mentioned). — [Deepgram Data Privacy Compliance](https://developers.deepgram.com/docs/data-privacy-compliance)
- EU endpoint `api.eu.deepgram.com` is GA (changelog 2025-12-03; blog 2026-01-10) for `/v1/listen` and `/v2/listen` (not Whisper). Hosting is within the EU, but "the specific country may change". The "Custom Endpoints" page still says Early Access, which is stale (via search summary). — [Deepgram EU endpoint GA blog](https://deepgram.com/learn/deepgram-eu-endpoint-now-generally-available); [Deepgram Regional endpoints](https://developers.deepgram.com/reference/regional-endpoints)
- Flux Multilingual EU URL: `wss://api.eu.deepgram.com/v2/listen` (via search summary of launch coverage). — [implicator.ai](https://www.implicator.ai/deepgram-launches-flux-multilingual-speech-model-with-10-language-mid-call-switching/)

**AssemblyAI**

- "If you opt out of model training ... we offer zero data retention of audio and transcripts for our Streaming product". Some metadata is stored. Files are excluded from training if covered by a BAA, if they use European servers, or if opted out. SOC 2 Type 1 and Type 2. A DPA is linked. ISO 27001 is not mentioned. GDPR is "principles top of mind" (no certification claimed). — [AssemblyAI Data retention and model training](https://assemblyai.com/docs/data-retention-and-model-training)
- Data Controls has a training opt-out toggle. A paid plan is needed, and "free users cannot opt out". Changes apply going forward only. A BAA auto-opts out (via search summary). — [AssemblyAI Data controls](https://assemblyai.com/docs/data-controls)
- EU streaming: `streaming.eu.assemblyai.com`, "for data residency within the EU", same price. — [AssemblyAI Pricing](https://www.assemblyai.com/pricing)

**Speechmatics**

- "Real-time inputs and outputs are not stored unless you opt in to the model-training program". "Data is not used to improve models unless you opt in". The training discount is "Off by default". — [Speechmatics Pricing](https://www.speechmatics.com/pricing)
- ISO/IEC 27001:2022, SOC 2 Type II, HIPAA and GDPR. Hosted on Azure. DPA and EU hosting are not stated on the page; the Trust Center is speechmatics.safebase.us. — [Speechmatics Security](https://www.speechmatics.com/security)
- RT endpoints: `eu.rt.speechmatics.com` (EU1), `us.rt…`, `au.rt…` and `global.rt…`. The global endpoint "may route to any region", so use the regional one for residency. — [Speechmatics Authentication docs](https://docs.speechmatics.com/get-started/authentication)

**Gladia**

- Pricing page: Self-serve model training is on by default, with free opt-out. Enterprise has no training by default and "Zero data retention". — [Gladia Pricing](https://www.gladia.io/pricing)
- Compliance hub contradicts this: data is "never used for model training unless explicitly agreed", and there is a "Zero Data Retention policy for paid plans". EU and US workloads run on separate region-specific infrastructure. DPAs are available for EU and US customers. Badges: GDPR, HIPAA, SOC 2 Type 2, ISO 27001. — [Gladia Compliance hub](https://gladia.io/compliance-hub)
- The older General Terms say SOC 2 Type 1 achieved, with Type 2 and ISO 27001 in progress, and that Free-plan audio may be used for training (via search summary; likely outdated). — [Gladia General Terms / security](https://gladia.io/security)
- Live API `region` query param: `us-west` or `eu-west`. — [Gladia /v2/live reference](https://docs.gladia.io/api-reference/v2/live/init)

**Soniox**

- "Soniox does not store your audio or transcript data unless explicitly requested". Data is "never used to improve Soniox models or services". Certifications: SOC 2 Type 2, ISO/IEC 27001:2022, GDPR and HIPAA, with docs via the Console. Async data is auto-deleted after 30 days. A DPA is not mentioned. — [Soniox Security & privacy](https://soniox.com/docs/security-and-privacy)
- Regions are US, EU, JP and IN. The EU real-time host is `stt-rt.eu.soniox.com`. "Your content (audio + transcripts) never leaves the region you choose". Residency is per project and requires a region-access request in the Console; regional projects get their own API keys. — [Soniox Data residency](https://soniox.com/docs/data-residency)

### Inferences

- For a French/EU product handling interview audio (personal data under GDPR), the defaults that need no extra action are Soniox (EU project) and Speechmatics (EU RT endpoint).
- Deepgram is acceptable only if every connection sets `mip_opt_out=true` and uses `api.eu.deepgram.com`. Forgetting the flag means audio is retained for training.
- AssemblyAI requires a paid plan plus a training opt-out and the EU host.
- With Gladia, verify the training setting in the account or DPA, given the conflicting pages.
- Note that the candidate's own voice (mic) and the interviewer's voice (loopback) are both recorded. The interviewer is a third party who hasn't consented, which is a legal/ethical consideration beyond STT choice.

### Gaps

- DPA availability is not confirmed on fetched pages for Deepgram (general), Speechmatics or Soniox. Each references compliance docs on request.
- Deepgram ISO 27001 status: not found.
- AssemblyAI ISO 27001 status: not found.
- Gladia: actual current training default for self-serve, given the contradictory pages.

---

## Q8. Official Rust SDK? Auth method and short-lived tokens for a desktop app

### Takeaway

No provider ships a first-class, stable official Rust SDK.

- Deepgram has the `deepgram` crate (0.14.0, 2026-10-20). It is labeled "Community owned", lives in the deepgram GitHub org, supports Flux and temp tokens, and is pre-1.0.
- Speechmatics has `speechmatics-rs` in its official GitHub org, marked WIP.
- AssemblyAI, Gladia and Soniox: no Rust SDK found. Use raw `tokio-tungstenite`.

All five offer a short-lived credential mechanism, so the desktop app need not embed the master key:

- Deepgram: JWT via `/v1/auth/grant`
- AssemblyAI: `/v3/token`
- Speechmatics: temp keys, though Enterprise must contact support
- Soniox: temporary API key
- Gladia: the backend-side `POST /v2/live` returns a tokenized WebSocket URL

### Cited Findings

- **Deepgram** crate `deepgram` 0.14.0, released 2026-10-20. "This SDK is currently Community owned but is moving to a stable `1.0` version soon". Repo `deepgram/deepgram-rust-sdk`. The `listen` feature covers "pre-recorded REST, live WebSocket streaming, Flux STT". Temp-token auth via `Deepgram::with_temp_token(...)`. Notes on TLS roots behind inspecting proxies. — [docs.rs deepgram crate](https://docs.rs/crate/deepgram/latest)
- **Deepgram** auth: API key header `Authorization: Token <key>`. Temp tokens come from `POST https://api.deepgram.com/v1/auth/grant` (the key needs Member permission). The JWT TTL defaults to 30 s, max 3600 s (`ttl_seconds`), and is used as `Authorization: Bearer <JWT>`. The token "only needs to be valid when the WebSocket connection is opened". `/listen` WebSocket is supported. Flux is not mentioned explicitly. — [Deepgram Token-based authentication](https://developers.deepgram.com/guides/fundamentals/token-based-authentication)
- **AssemblyAI** auth: key in the `Authorization` header ("no `Bearer` prefix"), or a `token` query param. — [AssemblyAI quickstart](https://www.assemblyai.com/docs/speech-to-text/universal-streaming); [AssemblyAI Streaming API reference](https://www.assemblyai.com/docs/api-reference/streaming-api/streaming-api)
- **AssemblyAI** temp tokens:
  - Endpoint `https://streaming.assemblyai.com/v3/token`. The prose says POST but the code samples use GET.
  - `expires_in_seconds` 1–600. `max_session_duration_seconds` 60–10800 (default 3 h).
  - Not single-use: a token is reusable, including for concurrent sessions, until expiry.
  - Passed as `token` query param. Usage is billed to the parent key.
  - EU variant not stated on the page.

  — [AssemblyAI Temporary token docs](https://www.assemblyai.com/docs/streaming/authenticate-with-a-temporary-token)

- **Speechmatics** auth: `Authorization: Bearer <key>`. Temp keys via `POST https://mp.speechmatics.com/v1/api_keys?type=rt`, `ttl` 60–86400 s (required), passed as `?jwt=<temp>` on the RT WebSocket. RT temp keys are not region-bound. "Enterprise customers must contact support or their account manager to use temporary keys". — [Speechmatics Authentication](https://docs.speechmatics.com/get-started/authentication)
- **Speechmatics** Rust: official GitHub `speechmatics/speechmatics-rs`. Feature-gated (`realtime` uses tokio and tokio-tungstenite). The README calls it work in progress, "subject to significant change", with error handling lacking. The crates.io crate is `speechmatics`; one aggregator saw 0.3.3, about a year old (via search summary). — [GitHub speechmatics-rs](https://github.com/speechmatics/speechmatics-rs); [docs.rs speechmatics](https://docs.rs/speechmatics)
- **Gladia** auth: `x-gladia-key` header on `POST /v2/live`. The returned WebSocket URL carries a `token` param. Temporary tokens are not documented separately. — [Gladia Live getting started](https://docs.gladia.io/chapters/live-stt/getting-started)
- **Soniox** temp keys: `POST https://api.soniox.com/v1/auth/temporary-api-key` with `usage_type: "transcribe_websocket"` and `expires_in_seconds` (examples use 60; range not given). The key is only needed to open the WebSocket. Optional `client_reference_id` for attribution. Rate limits apply to key creation. — [Soniox Direct stream guide](https://soniox.com/docs/stt/guides/direct-stream)

### Inferences

- For a Rust/Tauri-style Windows app, the lowest-risk path for every provider is a raw `tokio-tungstenite` client. Each protocol is small (JSON config plus binary PCM frames plus JSON results). SDK maturity is not a differentiator.
- The Deepgram crate is the most complete Rust option (Flux plus temp tokens, updated 2026-10-20). It is still pre-1.0 and community-labeled.
- Recommended architecture: a tiny backend (or serverless function) holds the master key and mints per-session tokens. The desktop app connects directly to the provider WebSocket with them, which avoids proxying audio and the latency that adds.
  - Deepgram: 30–60 s JWT
  - AssemblyAI: ≤600 s
  - Soniox: about 60 s
  - Speechmatics: ≥60 s
  - Gladia: the backend calls `POST /v2/live` and returns the `wss` URL

### Gaps

- Deepgram docs don't explicitly confirm temp-token use on `/v2/listen` (Flux). The Rust crate supports both, but that is not a protocol guarantee.
- The exact WebSocket header form for Deepgram JWT (Authorization header vs `Sec-WebSocket-Protocol`) was not shown on the fetched page. Native Rust clients can set headers, so this is low risk.
- Soniox `expires_in_seconds` allowed range: not found.
- Gladia: the expiry of the session URL token is not documented.

---

## Q9. Published French WER numbers or independent benchmarks (French / code-switching)

### Takeaway

No independent, streaming, French-specific or FR/EN code-switching benchmark covering these five providers was found. The only French code-switching streaming number is AssemblyAI's own: EN↔FR 6.13% WER for U3.6 Pro. Its pooled mean across 5 European pairs is 7.20%, vs Deepgram Nova-3 Multi 17.13% in the same vendor test. All other figures are vendor-run, and most are English or async.

### Cited Findings

- **AssemblyAI** (vendor, 2026-09-29): U3.6 Pro has "the lowest word error rate of any streaming system tested on speech that switches mid-sentence". Mean WER across 20 public sets covering five English-to-European pairs:

  | Model        | Mean WER |
  | ------------ | -------- |
  | U3.6 Pro     | 7.20%    |
  | U3.5 Pro     | 8.65%    |
  | Nova-3 Multi | 17.13%   |
  | Flux EN      | 52.86%   |
  | Nova-3 EN    | 54.71%   |

  EN↔FR alone: 6.13%. Competitors were run at default settings via public streaming APIs. Soniox, Speechmatics, Gladia and Deepgram Flux Multilingual are not listed in the extracted comparison. — [AssemblyAI blog: Universal-3.6 Pro Realtime](https://www.assemblyai.com/blog/universal-3-6-pro-realtime)

- **AssemblyAI** (vendor, async) code-switching test, normalized WER across five pairs: U3.5 Pro 7.69%, ElevenLabs Scribe v2 8.77%, Soniox v5 Async 10.01%, Deepgram Nova-3 Multilingual 12.22% (via search summary; the exact page was not fetched). — [AssemblyAI blog (search result)](https://www.assemblyai.com/blog/how-accurate-speech-to-text)
- **Gladia** (vendor) FLEURS table: French row shows Deepgram Nova-3 at about 10.6% WER. The extraction was garbled, so other columns could not be mapped (via search summary). — [Gladia blog: real-time multilingual ASR with code-switching](https://www.gladia.io/blog/building-real-time-multilingual-asr-with-code-switching)
- **Speechmatics** (vendor) Melia on FLEURS (read speech, batch): fewer errors than Deepgram, Microsoft and AssemblyAI "on most languages". Win share: 91% vs Deepgram, 91% vs Microsoft, 77% vs AssemblyAI. No French figure. — [Speechmatics Melia article](https://speechmatics.com/company/articles-and-news/introducing-melia-multilingual-speech-to-text-model)
- **Soniox** v4 (secondary source): 1.25% semantic WER and 249 ms median time-to-final on Pipecat smart-turn (English). Soniox's comparison pages show per-language WER vs competitors, with unverified methodology (via search summary). — [voiceaispace.com](https://www.voiceaispace.com/articles/soniox-v5-real-time-follows-a-live-conversation-not-just-the-words); [Soniox compare-stt](https://soniox.com/compare-stt?lang=cs)
- **Anecdotal:** an HN commenter judged Soniox STT v5 "definitely the best", while finding Gemini's transcription slightly lower latency. This is one person's test. — [Hacker News](https://news.ycombinator.com/item?id=49474353)
- **Deepgram:** a third-party Spanish-language article claims about 34% relative batch WER reduction for Nova-3 Multilingual in a March 2026 update. Unverified; not found on Deepgram pages read (via search summary).
- The Recall.ai `codeswitching-benchmark` repo is only a harness. It "does not include real benchmark results", implements only OpenAI, and its example pair is EN–ES. — [GitHub recallai/codeswitching-benchmark](https://github.com/recallai/codeswitching-benchmark)
- Coval's 2026 guide notes that competition now centers on streaming latency, end-of-turn, multilingual depth and code-switching, and that vendor benchmarks don't predict performance on your own traffic (via search summary). — [Coval blog](https://coval.ai/blog/best-speech-to-text-providers-in-2026-independent-benchmarks-and-how-to-choose)
- Aggregate WER can hide switch-point failures. One cited example: overall WER improved 1.69% while switch-point errors worsened 6.17% (via search summary; original source not identified). — [Coval blog](https://coval.ai/blog/best-speech-to-text-providers-in-2026-independent-benchmarks-and-how-to-choose)

### Inferences

- Vendor evidence favors AssemblyAI U3.6 Pro on FR/EN code-switched streaming. However, that test didn't include Soniox v5 RT, Deepgram Flux Multilingual or Speechmatics Melia RT, which are the most relevant rivals.
- A short in-house bake-off is the only reliable way to decide. Suggested test set: 30–60 recorded French tech-interview questions with embedded English terms plus fully English questions, captured through the real loopback path at 16 kHz.
- Measure:
  - WER, and separately the English-term recall inside French sentences
  - time from end of speech to final (or `<end>` / EndOfTurn) on each provider
  - false turn splits on hesitations

### Gaps

- No independent French or FR/EN code-switching streaming WER for any of the five providers.
- No published French figures for Soniox v5 RT, Deepgram Flux Multilingual, Speechmatics Melia RT or Gladia Solaria-1 live, beyond garbled or vendor tables.
- The exact source pages for the AssemblyAI async code-switching table and the Gladia FLEURS table were not fetched. Numbers come from search summaries.
