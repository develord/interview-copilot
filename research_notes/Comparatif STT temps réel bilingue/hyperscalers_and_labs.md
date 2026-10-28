# Real-time streaming STT from hyperscalers and AI labs (Azure, Google, AWS, OpenAI, ElevenLabs, Mistral) for a French-first, FR/EN code-switching interview copilot (state as of 2026-10-28)

Scope reminder: Windows desktop app, Rust backend, 16 kHz mono PCM16 in 20 ms frames, two sources (system loopback = interviewer, mic = candidate). Target is a useful answer about 2 s after the question ends. Research date: 2026-10-28. Several key offerings are less than 3 months old or in preview (flagged inline). Unless marked "secondary" or "vendor claim", facts come from official docs or pricing pages.

**Big changes since mid-2026 that a writer must not miss:**

- **Microsoft:** MAI-Transcribe-2-Streaming (public preview, docs dated 2026-09-30) uses an OpenAI-Realtime-like WebSocket. It covers 60 languages in multilingual auto-detect mode.
- **Google:** Gemini 3.5 Transcribe Live (preview Aug 2026, GA Oct 2026) claims 85+ languages with mid-session code-mixing.
- **OpenAI:** `gpt-live-transcribe` launched 2026-07-28 and is now the recommended realtime transcription model, replacing `gpt-4o-transcribe` as the primary choice for live use.
- **Mistral:** Voxtral Mini Transcribe Realtime 3 (`voxtral-mini-transcribe-realtime-3-0`, released 2026-10-20, public preview) added end-of-turn detection, context biasing and diarization in streaming.

---

## 1. Does real-time streaming support French, and FR+EN code-switching within one stream (auto multilingual / language ID vs fixed locale)? Documented limitations

### Takeaway

Every provider supports French in streaming. Mid-sentence FR/EN code-switching is a different matter. Only four products explicitly document or claim support for intra-utterance code-switching, and none of them has an independent French-English benchmark:

- Microsoft MAI-Transcribe-2(-Streaming): multilingual mode by default, "code switching" listed.
- Azure "Multilingual Post-Stream Refinement" (preview).
- Google Gemini 3.5 Transcribe Live: "mid-session code-mixing".
- AWS multi-language identification: language switches mid-conversation.

The classic Azure continuous LID explicitly does NOT handle language changes within a sentence.

### Cited Findings

**Microsoft Azure AI Speech, classic real-time STT (Speech SDK)**

- **Candidate languages:** LID uses candidate languages via `AutoDetectSourceLanguageConfig`. You can give up to 4 candidates for at-start LID or up to 10 for continuous LID. The service always returns one of the candidates, even if another language is spoken. — [MS Learn, Language identification (ms.date 2025-12-19, updated 2026-08-10)](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/language-identification)
- **At-start vs continuous:** At-start LID detects the language once, within the first few seconds, in less than 5 s. Continuous LID "can identify multiple languages during the audio" but "doesn't support changing languages within the same sentence". The doc's example: primarily Spanish with some English words inserted, and "it doesn't detect the language change per word". — [MS Learn, Language identification](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/language-identification)
- **Latency and SDK requirements:** "For speech recognition, the initial latency is higher with language identification." Continuous LID only works with continuous recognition. It is only supported in the Speech SDKs for C#, C++, Java, JavaScript and Python, and requires creating the SpeechConfig from the (v2) endpoint. — [MS Learn, Language identification](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/language-identification)
- **Multilingual Post-Stream Refinement (public preview, announced ~2026-07-23):**
  - Lets one real-time stream transcribe multiple languages with automatic detection and "no candidate list" (open-range `AutoDetectSourceLanguageConfig`). Code-switching within a single utterance is claimed.
  - Requires Speech SDK ≥1.50 and the property `SpeechServiceResponse_PostProcessingOption = "PostRefinement"`.
  - Regions: East US, West US, North Europe, Central India, Southeast Asia, Japan East.
  - Language coverage figures conflict ("15 Tier-1 locales" vs "25 auto-detected languages spanning 29 market locales"), and French is not explicitly named.
  - [Argon Systems republication of the Microsoft Foundry blog post, 2026-07-23 (secondary copy)](https://argonsys.com/microsoft-cloud/library/for-the-first-time-real-time-transcription-goes-multilingual/)
- **Fast transcription (not streaming):** With the multilingual model and locales omitted, fast transcription supports a fixed list of 15 locales, including fr-FR. — search summary of MS docs, [MS Learn LID/diarization config](https://learn.microsoft.com/en-my/Azure/ai-services/speech-service/configure-language-identification-diarization) (not fetched directly; verify)

**Microsoft MAI-Transcribe-2-Streaming (public preview)**

- **Languages and modes:** The model "operates in multilingual mode with language auto-detection" by default. It supports 60 languages, French (`fr`) and English (`en`) included. — [MS Learn, MAI-Transcribe-2-Streaming overview (ms.date 2026-09-30, updated 2026-10-07)](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/mai-transcribe-2-streaming)
- **Language hint:** `transcription.language` is optional; it starts as `null`, meaning auto-detection. "The current backend treats unrecognized values as unset." — [MS Learn, MAI-Transcribe-2-Streaming with the Realtime API](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/mai-transcribe-2-streaming-realtime)
- **Code switching (batch/fast sibling):** The MAI-Transcribe-2 doc lists "Code switching: Automatic … for commonly blended language pairs such as Hinglish and Spanglish, handles conversations that move between languages mid-utterance". FR/EN is not named. Forcing `locales` is "a very strong hint" and only one language can be given. — [MS Learn, MAI-Transcribe-2 (ms.date 2026-09-22)](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/mai-transcribe)
- **Preview status:** "Public preview … without a service-level agreement, and is not recommended for production workloads." — [MS Learn, MAI-Transcribe-2-Streaming overview](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/mai-transcribe-2-streaming)

**Google Cloud Speech-to-Text v2, Chirp 3 (`chirp_3`)**

- **Methods and French:** StreamingRecognize, Recognize and BatchRecognize are all supported. fr-FR and fr-CA are GA for transcription. — [Google Cloud docs, Chirp 3 (last updated 2026-10-26)](https://docs.cloud.google.com/speech-to-text/v2/docs/chirp_3-model)
- **Language detection:** Language-agnostic mode `language_codes=["auto"]` (GA) "infers the dominant language", and multiple codes such as `["en-US","fr-FR"]` can be given. The page's examples use Recognize. The streaming examples use a single code, and the page does not state whether `auto` or multiple codes work in streaming. Code-switching is not mentioned. — [Google Cloud docs, Chirp 3](https://docs.cloud.google.com/speech-to-text/v2/docs/chirp_3-model)

**Google Gemini 3.5 Transcribe Live (Vertex / "Gemini Enterprise Agent Platform")**

- **Models and status:** `gemini-3.5-transcribe-live` handles streaming and `gemini-3.5-transcribe` handles files. Preview variants (`…-preview`) came out in August 2026 and GA versions in October 2026. — [Google Cloud docs, Gemini 3.5 Transcribe (updated 2026-10-28)](https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/gemini/3-5-transcribe)
- **Languages:** "Auto-detection supports 85+ languages, including mid-session code-mixing". French is `fr-FR`/`fr-CA`, and many languages are marked "Experimental". — [Google Cloud docs, Gemini 3.5 Transcribe](https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/gemini/3-5-transcribe)
- **Session limit:** Live streaming accepts up to 10 minutes of audio per session. — [Google Cloud docs, Gemini 3.5 Transcribe](https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/gemini/3-5-transcribe)
- **Launch date:** Reported as 2026-08-26 (secondary). — [iPhone in Canada, 2026-08-27](https://www.iphoneincanada.ca/2026/08/27/google-rolls-out-gemini-3-5-transcribe-with-exceptional-accuracy/)

**AWS Transcribe streaming**

- **French support:** fr-FR is supported in streaming (data input "batch, streaming"). — [AWS docs, Supported languages](https://docs.aws.amazon.com/transcribe/latest/dg/supported-languages.html)
- **Single-language identification:** Streaming LID needs at least 2 language codes, with only one dialect per language (en-US + en-GB is not allowed). It needs at least 1 s of speech. An optional `preferred-language` "can speed up the language identification process". If nothing matches, the closest candidate is used. — [AWS docs, LID with streaming](https://docs.aws.amazon.com/transcribe/latest/dg/lang-id-stream.html)
- **Multi-language identification:** `identify-multiple-languages=true` "identifies all languages spoken in your stream … if speakers change languages mid-conversation … detects and transcribes each language correctly". The example is a bilingual en-US/hi-IN speaker. Only PCM is supported for multi-language ID. Streaming LID can't be combined with custom language models or redaction. — [AWS docs, LID with streaming](https://docs.aws.amazon.com/transcribe/latest/dg/lang-id-stream.html)

**OpenAI Realtime API transcription**

- **Models:** The recommended model is `gpt-live-transcribe`. It takes `audio.input.transcription.languages`, an array of ISO 639-1 codes used as multiple language hints, and it does NOT return detected-language predictions. `gpt-transcribe` used inside a Realtime session returns a `languages` array, but only starts after a committed turn. — [OpenAI docs, Realtime transcription](https://developers.openai.com/api/docs/guides/realtime-transcription)
- **Feature list:** The model page says it supports "tunable latency, unstructured context, keyword hints, and multiple language hints". It lists no supported-language table, so French is not explicitly listed there. — [OpenAI docs, gpt-live-transcribe model page](https://developers.openai.com/api/docs/models/gpt-live-transcribe)
- **Release date:** 2026-07-28 (secondary). — [Gigazine, 2026-07-29](https://gigazine.net/gsc_news/en/20260729-gpt-live-transcribe)

**ElevenLabs Scribe v2 Realtime (`scribe_v2_realtime`)**

- **Languages:** "90+ languages", French included. "Smart language detection" is listed only for Scribe v2 batch, not Realtime. — [ElevenLabs docs, Speech to Text capabilities](https://elevenlabs.io/docs/capabilities/speech-to-text)
- **Language parameters:** The WebSocket accepts `language_code` (ISO 639-1/639-3), `secondary_languages` (array) and `include_language_detection` (default false). Committed transcripts with timestamps carry a `language_code`. The semantics of `secondary_languages` (whether it enables FR+EN code-switching) are not described in the spec. — [ElevenLabs API reference, Realtime STT WebSocket](https://elevenlabs.io/docs/api-reference/speech-to-text/v-1-speech-to-text-realtime)

**Mistral Voxtral realtime**

- **Model:** `voxtral-mini-transcribe-realtime-3-0` (aliases `…-realtime-3`, `…-realtime-latest`), released 2026-10-20, Public Preview. — [Mistral docs, model page](https://docs.mistral.ai/models/voxtral-mini-transcribe-realtime-3-0)
- **Languages (vendor):** "sub-500ms latency in over 35 languages", with French included. Code-switching is not mentioned. — [Mistral news, Voxtral 3](https://mistral.ai/news/voxtral-3)
- **Previous model:** `voxtral-mini-transcribe-realtime-2602` (Feb 2026). Its realtime doc page documents no language parameter, and that page now sits under a "Deprecated" docs section. — [Mistral docs, realtime transcription (deprecated section)](https://docs.mistral.ai/studio/audio/speech_to_text/realtime_transcription)

### Inferences

- **Azure classic Speech SDK continuous LID is the wrong tool for this use case.** It is sentence-level, not word-level, so "le caching côté backend" inside a French sentence won't trigger a switch. Run it with a single fr-FR locale plus a phrase list of English tech terms, or use MAI-Transcribe-2-Streaming or the Post-Stream Refinement preview instead.
- **The strongest documented fits for intra-sentence FR/EN are:**
  - MAI-Transcribe-2-Streaming (multilingual by default; code-switching claimed for the model family)
  - Gemini 3.5 Transcribe Live ("mid-session code-mixing")
  - AWS multi-language ID (documented for mid-conversation switches, not explicitly mid-sentence)

  All of them should be tested on real FR/EN interview audio.

- **The "multilingual LLM-style" streaming models work without a fixed locale:** OpenAI `gpt-live-transcribe` (language hints `["fr","en"]`), ElevenLabs (`language_code` + `secondary_languages`) and Mistral. In practice they probably handle embedded English tech words in French speech. None of them documents this explicitly.

### Gaps

- No provider publishes an FR/EN intra-sentence code-switching evaluation.
- Whether Chirp 3 StreamingRecognize accepts `language_codes=["auto"]` or multiple codes is not stated in the docs.
- No streaming-specific language list was found for gpt-live-transcribe or Voxtral Realtime 3.
- Whether Azure Post-Stream Refinement covers fr-FR is not stated in the copy I read.
- The meaning of ElevenLabs `secondary_languages` is not documented.

---

## 2. Streaming protocol, partial vs final semantics, endpointing / VAD / turn detection, latency figures

### Takeaway

Every provider except Google Chirp (gRPC) and the classic Azure SDK (proprietary WebSocket wrapped by native SDKs) exposes a documented WebSocket.

Several new models drop server-side VAD and expect a client-side VAD that sends "commit": OpenAI gpt-live-transcribe and MAI-Transcribe-2-Streaming. That fits a Rust app that already frames audio and can run its own VAD.

Vendor latency claims:

- MAI-T2-Streaming: final about 0.13 s after speech stops (secondary).
- ElevenLabs: about 150 ms.
- Mistral: 80–2400 ms configurable.
- Gemini Live: "low-latency", no figure.

### Cited Findings

**Azure MAI-Transcribe-2-Streaming, Realtime API**

- **Connection:** WebSocket `wss://{resource}.services.ai.azure.com/mai/v1/realtime?intent=transcription`, using an "OpenAI Realtime API-like protocol". — [MS Learn, MAI-T2-Streaming Realtime API](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/mai-transcribe-2-streaming-realtime)
- **Server events:**
  - `…input_audio_transcription.delta` carries "newly finalized transcript".
  - `…intermediate` (MAI-specific) carries the full provisional suffix (partial).
  - `…completed` carries the full final text since the last commit.
- **No server VAD:** `turn_detection` and `noise_reduction` accept only `null`. "There is no server-side speech detection or automatic commit." The client should send `input_audio_buffer.commit` "at a natural pause in speech (such as identified by a VAD model)", which "produces completed transcription as soon as possible".
- **Session rules:** Sessions last up to 1 hour. Settings can't change after the first audio append.
- **Speech SDK route:** The Speech SDK route also exists (SDK ≥1.52.0, endpoint `wss://{region}.stt.speech.microsoft.com/speech/universal/v2`). — search snippet of [MS Learn, MAI-T2-Streaming with Speech SDK](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/mai-transcribe-2-streaming-speech-sdk) (not fetched in full)
- **Latency (secondary / vendor-derived):** First partial "just over 100 ms" after audio starts. Final text about 0.13 s after the speaker stops. — [pasqualepillitteri.it](https://pasqualepillitteri.it/en/news/19840/microsoft-mai-transcribe-2-streaming); [testingcatalog](https://testingcatalog.com/microsoft-launches-mai-transcribe-2-streaming)
- **Output fields via the Speech SDK (secondary):** Results reportedly do not include recognized language, confidence or word timestamps. — [testingcatalog](https://testingcatalog.com/microsoft-launches-mai-transcribe-2-streaming)

**Azure classic real-time (Speech SDK)**

- **Recognition modes:** Recognize-once or continuous recognition through the Speech SDK. Continuous LID needs continuous recognition. — [MS Learn, Language identification](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/language-identification)
- **Post-Stream Refinement latency:** Partial/first-token latency is unchanged, and the final "may add a small amount of latency to the final segment". — [Argon Systems republication, 2026-07-23](https://argonsys.com/microsoft-cloud/library/for-the-first-time-real-time-transcription-goes-multilingual/)

**Google Chirp 3**

- **Streaming endpointing:** Streaming endpointing sensitivity has three levels: STANDARD (default), SHORT and SUPERSHORT, "trading latency against accuracy". — [Google Cloud docs, Chirp 3](https://docs.cloud.google.com/speech-to-text/v2/docs/chirp_3-model)
- **Other streaming features:** Utterance-level timestamps are streaming-only, and a denoiser (`denoiser_audio`) is available. — [Google Cloud docs, Chirp 3](https://docs.cloud.google.com/speech-to-text/v2/docs/chirp_3-model)

**Google Gemini 3.5 Transcribe Live**

- **Protocol:** Uses the `BidiGenerateContent` (Live API). Interim and final transcripts arrive separately as `interim_input_transcription` and `input_transcription`. The client must wait for `setup_complete` before sending audio. — [Google Cloud docs, Gemini 3.5 Transcribe](https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/gemini/3-5-transcribe)
- **VAD:** Automatic VAD can be disabled (`automatic_activity_detection` disabled in `RealtimeInputConfig`). The client then marks turns with `activity_start`/`activity_end`. — [Google Cloud docs, Gemini 3.5 Transcribe](https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/gemini/3-5-transcribe)
- **Latency:** Described only as "low-latency" on the doc. Secondary coverage says "sub-second". — [smallbiztrends](https://smallbiztrends.com/google-unveils-gemini-3-5-transcribe-for-superior-speech-to-text-accuracy/)

**AWS Transcribe streaming**

- **Transports:** SDKs (preferred), HTTP/2 or WebSockets. — [AWS docs, Streaming](https://docs.aws.amazon.com/transcribe/latest/dg/streaming.html)
- **Chunking:** "Latency depends on the size of your audio chunks … set each chunk to between 50 ms and 200 ms". Use uniform chunk sizes and send zero-byte silence when there is no speech. — [AWS docs, Streaming](https://docs.aws.amazon.com/transcribe/latest/dg/streaming.html)
- **Session limit:** Maximum session duration is a hard limit; when it is reached, start a new session. — [AWS docs, Streaming](https://docs.aws.amazon.com/transcribe/latest/dg/streaming.html)

**OpenAI Realtime**

- **Connection:** WebSocket `wss://api.openai.com/v1/realtime?intent=transcription`. WebRTC is recommended for browsers. — [OpenAI docs, Realtime transcription](https://developers.openai.com/api/docs/guides/realtime-transcription)
- **Turn detection:** For `gpt-live-transcribe`, `turn_detection` must be `null` or omitted. The model "doesn't support server_vad or semantic_vad", so use client-side VAD and then `input_audio_buffer.commit`. — [OpenAI docs, Realtime transcription](https://developers.openai.com/api/docs/guides/realtime-transcription)
- **Delay setting:** `audio.input.transcription.delay` accepts `minimal | low | medium | high | xhigh`. The actual ms "varies by configuration", and no figures are given. — [OpenAI docs, Realtime transcription](https://developers.openai.com/api/docs/guides/realtime-transcription)
- **Events:** `…input_audio_transcription.delta` carries incremental text and `…completed` carries the final text on commit. Completion events from different turns "can arrive out of order", so match them by `item_id`. — [OpenAI docs, Realtime transcription](https://developers.openai.com/api/docs/guides/realtime-transcription)

**ElevenLabs Scribe v2 Realtime**

- **Endpoint:** WebSocket `GET /v1/speech-to-text/realtime`.
- **Commit strategies:** `commit_strategy` = `manual | vad | turn_prediction` ("turn_prediction is the default only for turn-predicting models").
- **VAD parameters:** `vad_threshold`, `vad_silence_threshold_secs`, `min_speech_duration_ms`, `min_silence_duration_ms`.
- **Messages:** Server sends `partial_transcript` ("interim text that may change") and `committed_transcript` ("stable text"). The client can also set `commit: true` on any `input_audio_chunk`. `keepalive_interval_ms` ranges 500–10000.
- [ElevenLabs API reference, Realtime STT](https://elevenlabs.io/docs/api-reference/speech-to-text/v-1-speech-to-text-realtime)
- **Latency (vendor):** "~150ms†"; the footnote is not shown. — [ElevenLabs docs, STT capabilities](https://elevenlabs.io/docs/capabilities/speech-to-text)
- **Conflicting vendor figure:** An ElevenLabs Agents blog post claims 30–80 ms. — [ElevenLabs blog](https://elevenlabs.io/blog/scribe-v2-realtime-in-elevenlabs-agents)

**Mistral Voxtral Realtime**

- **Realtime 3 (current):** Configurable latency from 80 ms to 2,400 ms, built-in end-of-turn detection, over WebSocket. — [Mistral docs, model page](https://docs.mistral.ai/models/voxtral-mini-transcribe-realtime-3-0)
- **Delay values:** The audio overview phrases the streaming delay as "80 to 1,200 ms, or 2,400 ms". — [Mistral docs, audio overview](https://docs.mistral.ai/studio/audio/overview)
- **End-of-turn detection:** Uses "both acoustic and semantic information". The FLEURS chart shows delay settings of 240/480/960/2400 ms. — [Mistral news, Voxtral 3](https://mistral.ai/news/voxtral-3)
- **2602 protocol (older model):**
  - Endpoint `wss://api.mistral.ai/v1/audio/transcriptions/realtime?model=…`, parameter `target_streaming_delay_ms` (examples 240, 1000, 2400).
  - Events: `TranscriptionStreamTextDelta`, then `TranscriptionStreamDone`.
  - No VAD or endpointing controls documented.
  - [Mistral docs, realtime transcription (deprecated section)](https://docs.mistral.ai/studio/audio/speech_to_text/realtime_transcription)
- **Third-party note on 2602:** A third-party integration page says the earlier model "does not include built-in turn detection". — [visionagents.ai](https://visionagents.ai/integrations/mistral)

### Inferences

- **For a 2 s answer budget, the app should own endpointing.** Run a local VAD on the 20 ms frames and send an explicit commit/flush to the provider. MAI-T2-Streaming and gpt-live-transcribe require this; ElevenLabs (`manual`), Gemini Live (`activity_end`) and Mistral 2602 support it. Time-to-final then becomes roughly VAD hangover (e.g. 300–600 ms) plus provider finalization (vendor claims of ~0.1–0.5 s). Both components are assumptions to be measured.
- **Two sources means two concurrent streams/sessions per interview** (one per channel) on every WebSocket provider except AWS. AWS can take dual-channel PCM with `ChannelIdentification` (see section 3).

### Gaps

- No vendor publishes p50/p95 "end-of-speech to final" latency under the conditions above. The 0.13 s MAI figure is secondary.
- The ms values of OpenAI `delay` levels are not documented.
- Azure classic SDK segmentation-silence settings were not fetched in this session.
- AWS partial-result stabilization (`EnablePartialResultsStabilization`/`PartialResultsStability`) exists in the API to my knowledge but was not verified against a fetched page this session.
- Gemini Live transport details (WebSocket vs gRPC) are not named on the model page.

---

## 3. Accepted input formats (16 kHz PCM16 mono? multichannel?)

### Takeaway

16 kHz mono PCM16 is natively accepted by Azure MAI-T2-Streaming, Google Chirp/Gemini Live, AWS, ElevenLabs (default `pcm_16000`) and Mistral. OpenAI Realtime `audio/pcm` is documented as 24 kHz only, so the Rust side must resample 16→24 kHz. Only AWS documents a dual-channel stream with per-channel transcription.

### Cited Findings

- **Azure MAI-T2-Streaming:**
  - Format: "Raw signed little-endian PCM16, mono, 16000 or 24000 samples/second", base64 in `input_audio_buffer.append`, no WAV header.
  - Chunking: "send audio in small chunks, such as 10-20 ms" to minimize latency.
  - [MS Learn, MAI-T2-Streaming Realtime API](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/mai-transcribe-2-streaming-realtime)
- **Google Gemini 3.5 Transcribe Live:** Examples use `"audio/pcm;rate=16000"` for live input. No full format list is given. — [Google Cloud docs, Gemini 3.5 Transcribe](https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/gemini/3-5-transcribe)
- **AWS Transcribe:**
  - Formats: streaming accepts FLAC, Ogg-Opus and PCM ("only signed 16-bit little-endian … does not include WAV").
  - Sample rate: "If possible, record at a sampling rate of 16,000 Hz".
  - Dual-channel PCM: supported, and each chunk must be a multiple of 4 bytes.
  - [AWS docs, Streaming](https://docs.aws.amazon.com/transcribe/latest/dg/streaming.html)
- **AWS dual-channel LID:** With two channels and `ChannelIdentification=true`, "each channel is transcribed separately" and the dominant language is identified per channel. The default is false, in which case only the first channel is transcribed. — [AWS docs, LID with streaming](https://docs.aws.amazon.com/transcribe/latest/dg/lang-id-stream.html)
- **OpenAI Realtime:**
  - The guide example uses `audio.input.format = {type:"audio/pcm", rate:24000}`, and audio is appended as base64 PCM16. — [OpenAI docs, Realtime transcription](https://developers.openai.com/api/docs/guides/realtime-transcription)
  - The API reference states only a 24 kHz sample rate is supported for `audio/pcm` (seen via search snippet of the official reference). — [OpenAI API reference, client secrets create](https://developers.openai.com/api/reference/resources/realtime/subresources/client_secrets/methods/create.md)
- **ElevenLabs:**
  - `audio_format` values: `pcm_8000, pcm_16000 (default), pcm_22050, pcm_24000, pcm_44100, pcm_48000, ulaw_8000`.
  - Each `input_audio_chunk` carries `audio_base_64`, `commit`, `sample_rate`, and optionally `previous_text` (only on the first chunk).
  - [ElevenLabs API reference, Realtime STT](https://elevenlabs.io/docs/api-reference/speech-to-text/v-1-speech-to-text-realtime)
- **Mistral (2602 doc):** `AudioFormat(encoding="pcm_s16le", sample_rate=16000)`, with demo sample rates 8000/16000/22050/44100/48000, mono, and chunks from 10 ms (demo) to 480 ms (example). — [Mistral docs, realtime transcription](https://docs.mistral.ai/studio/audio/speech_to_text/realtime_transcription)
- **Mistral Realtime 3:** The model page does not state audio formats. — [Mistral docs, model page](https://docs.mistral.ai/models/voxtral-mini-transcribe-realtime-3-0)

### Inferences

- **The 20 ms frames can be forwarded as-is** to MAI-T2-Streaming, ElevenLabs and Mistral. For AWS, aggregate them into 50–200 ms chunks, per AWS best practice. For OpenAI, resample to 24 kHz (e.g. with the `rubato` crate) and base64-encode.
- **AWS dual-channel option:** AWS could take loopback and mic interleaved as one 2-channel stream, which gives a single session and per-channel transcripts. Whether multi-language ID plus ChannelIdentification together are supported on one stream is not explicitly stated.

### Gaps

- Mistral Realtime 3 formats are undocumented on the model page.
- Gemini Live's accepted sample rates beyond the 16 kHz example are not listed.
- No WebSocket provider documents multichannel input.

---

## 4. Custom vocabulary / phrase lists / prompting / boosting in streaming, and whether it works for French

### Takeaway

All six have some streaming biasing:

- **Azure:** phrase lists (≤2,000 phrases, weight 0–2) in real-time SDK and fast transcription.
- **Google:** Chirp 3 adaptation (≤1,000 phrases); Gemini Live custom vocabulary (≤1,000 terms, best ≤100).
- **AWS:** custom vocabularies, fr-FR supported.
- **OpenAI:** `prompt` plus `keywords` in gpt-live-transcribe.
- **ElevenLabs:** ≤50 keyterms × 20 chars in realtime, paid add-on.
- **Mistral:** context biasing claimed for Realtime 3. The batch docs say biasing is "optimized for English; other languages experimental".

French-specific effectiveness is not documented by anyone.

### Cited Findings

- **Azure phrase list:**
  - Works with real-time transcription (Speech SDK/CLI/Studio), the Fast transcription API, LLM Speech and Voice Live. Not available in batch.
  - "Shouldn't have more than 2,000 phrases", and a "longer phrase list will impact quality and latency".
  - Weight ranges 0.0–2.0 (default 1.0) for real-time through the SDK.
  - Allowed characters are "locale-specific letters and digits".
  - [MS Learn, Phrase list (ms.date 2026-06-02, updated 2026-09-03)](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/improve-accuracy-phrase-list)
- **Azure MAI-Transcribe-2 (fast/batch REST):** Keyword biasing through `phraseList.phrases`; "Terms are hints, not forced output". — [MS Learn, MAI-Transcribe-2](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/mai-transcribe)
- **Azure MAI-T2-Streaming Realtime API:** The `session.update` schema shown contains no keyword/phrase field. — [MS Learn, MAI-T2-Streaming Realtime API](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/mai-transcribe-2-streaming-realtime)
- **Google Chirp 3:** Speech adaptation is GA with up to 1,000 phrases ("use as few as possible"). The example uses Recognize, and streaming support for adaptation is not explicitly listed. — [Google Cloud docs, Chirp 3](https://docs.cloud.google.com/speech-to-text/v2/docs/chirp_3-model)
- **Google Gemini 3.5 Transcribe:** Speech biasing takes "up to 1000 terms in both endpoints", with "best results … up to 100 terms". Supplying `language_codes` alongside custom vocabulary is recommended. — [Google Cloud docs, Gemini 3.5 Transcribe](https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/gemini/3-5-transcribe)
- **AWS:**
  - fr-FR supports custom-vocabulary tables (acronym column "batch, streaming") but no custom language models (CLM "no" for fr-FR; CLM only for en-*, de-DE, es-US, ja-JP, hi-IN batch). — [AWS docs, Supported languages](https://docs.aws.amazon.com/transcribe/latest/dg/supported-languages.html)
  - "If combining language identification with other features, you are limited to the languages supported with those features". — [AWS docs, LID with streaming](https://docs.aws.amazon.com/transcribe/latest/dg/lang-id-stream.html)
- **OpenAI gpt-live-transcribe:** `audio.input.transcription.prompt` is "a text description of the recording's setting". `audio.input.transcription.keywords` is an array of literal terms, with no `<`, `>`, CR or LF. The page documents no limit on the count. — [OpenAI docs, Realtime transcription](https://developers.openai.com/api/docs/guides/realtime-transcription)
- **ElevenLabs:**
  - Realtime keyterm prompting allows "up to 50 keyterms (20 characters each)"; batch Scribe v2 allows up to 1,000 × 50 chars. Keyterms "rely on context" to decide whether to transcribe a term. — [ElevenLabs docs, STT capabilities](https://elevenlabs.io/docs/capabilities/speech-to-text)
  - Realtime keyterms cost +$0.080/h. — [ElevenLabs API pricing](https://elevenlabs.io/pricing/api)
  - `previous_text` context is accepted on the first chunk. — [ElevenLabs API reference, Realtime STT](https://elevenlabs.io/docs/api-reference/speech-to-text/v-1-speech-to-text-realtime)
- **Mistral:**
  - Realtime 3 lists "context biasing" as supported, without stated limits. — [Mistral docs, model page](https://docs.mistral.ai/models/voxtral-mini-transcribe-realtime-3-0)
  - The batch docs allow up to 100 words/phrases and say biasing "is optimized for English; support for other languages is experimental" (search summary of the official page). — [Mistral docs, offline transcription](https://docs.mistral.ai/studio/audio/speech_to_text/offline_transcription)
  - Voxtral Mini Transcribe 3 batch shares a 65,536-token context between audio and vocabulary. — [Mistral docs, audio overview](https://docs.mistral.ai/studio/audio/overview)

### Inferences

- **For this use case, the biasing list is mostly English tech terms** inside French speech (Vue, state management, caching, backend, Kubernetes...). The candidates are:
  - Azure phrase lists with a fr-FR locale (a long-established feature).
  - OpenAI `keywords` + `prompt`, which allows a prompt like "Entretien technique en français avec termes anglais".
  - Gemini Live custom vocabulary with `language_codes` fr-FR + en-US.
  - ElevenLabs keyterms, though the 50-term cap is tight.
- **Mistral's own caveat** ("optimized for English") is a risk for French-context biasing.

### Gaps

- No provider publishes a measured effect of biasing on French or on code-switched speech.
- No keyword/phrase support is documented for the MAI-T2-Streaming Realtime API.
- Chirp 3 adaptation in StreamingRecognize is not explicitly confirmed.
- No count limit is documented for OpenAI `keywords`.

---

## 5. Streaming diarization availability for French

### Takeaway

Streaming diarization is mostly unavailable or unconfirmed:

- Google (Chirp 3 = batch only; Gemini Live = not supported), OpenAI (gpt-live-transcribe returns no speaker labels) and ElevenLabs (batch only) don't offer it.
- Mistral Realtime 3 claims diarization in-stream (new, preview).
- Azure classic real-time supports diarization (as a paid add-on).

For this product, diarization matters less because the two sources (loopback vs mic) already separate interviewer from candidate.

### Cited Findings

- **Google:**
  - Chirp 3 diarization is GA but "available only in BatchRecognize" and "Not supported in streaming". Diarization languages include fr-FR and fr-CA. — [Google Cloud docs, Chirp 3](https://docs.cloud.google.com/speech-to-text/v2/docs/chirp_3-model)
  - Gemini 3.5 Transcribe: diarization "Not supported in live streaming"; the file mode supports up to 8 speakers. — [Google Cloud docs, Gemini 3.5 Transcribe](https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/gemini/3-5-transcribe)
- **OpenAI:** gpt-live-transcribe "doesn't return detected-language predictions, word-level timestamps, speaker labels, or confidence scores". — [OpenAI docs, Realtime transcription](https://developers.openai.com/api/docs/guides/realtime-transcription)
- **ElevenLabs:** "Speaker diarization, up to 32 speakers" is listed only for Scribe v2 batch. — [ElevenLabs docs, STT capabilities](https://elevenlabs.io/docs/capabilities/speech-to-text)
  - However, the realtime timestamped words schema includes a `speaker_id` field. — [ElevenLabs API reference, Realtime STT](https://elevenlabs.io/docs/api-reference/speech-to-text/v-1-speech-to-text-realtime)
- **Mistral:**
  - Realtime 3 supports "Speaker diarization … within the same streaming session". — [Mistral docs, model page](https://docs.mistral.ai/models/voxtral-mini-transcribe-realtime-3-0)
  - The older 2602 realtime model "is currently not compatible with the diarize parameter". — [Mistral docs, realtime transcription (deprecated section)](https://docs.mistral.ai/studio/audio/speech_to_text/realtime_transcription)
- **Azure:** Diarization "is available for both real-time and batch API", disabled by default. — [MS Learn, Data, privacy and security for STT](https://learn.microsoft.com/en-us/azure/ai-foundry/responsible-ai/speech-service/speech-to-text/data-privacy-security)
  - Diarization is a separately priced real-time add-on. — [Azure Speech pricing](https://azure.microsoft.com/en-us/pricing/details/speech/)
- **AWS:** fr-FR is supported for real-time Call Analytics ("post-call, real-time"). — [AWS docs, Supported languages](https://docs.aws.amazon.com/transcribe/latest/dg/supported-languages.html)

### Inferences

- **Two physical sources make provider diarization unnecessary** for interviewer vs candidate attribution: use one stream per source, or AWS dual-channel. Diarization only matters if several interviewers share the loopback channel.

### Gaps

- Azure real-time diarization language coverage (fr-FR) and its latency impact were not fetched.
- AWS streaming `ShowSpeakerLabel` support for fr-FR was not verified this session.
- Mistral Realtime 3 diarization quality and latency are unpublished.

---

## 6. Pricing for real-time streaming (per hour/minute), free tier, quotas

### Takeaway

List prices per audio hour (USD) for streaming:

- **Mistral Realtime 3:** about $0.48.
- **ElevenLabs Scribe v2 Realtime:** $0.39, plus $0.08 with keyterms.
- **MAI-T2-Streaming:** about $0.54 intro price (secondary).
- **Google Chirp 3 (V2):** about $0.96 for the first 500k min/month.
- **OpenAI gpt-live-transcribe:** about $1.02, plus 10% for EU residency.
- **AWS:** conflicting ($0.60 vs $1.44).
- **Azure classic real-time:** the USD figure could not be extracted (JS-rendered page).

Each interview needs two streams (loopback + mic), so double these numbers per interview-hour.

### Cited Findings

- **OpenAI:**
  - gpt-live-transcribe $0.017/min, gpt-realtime-whisper $0.017/min, gpt-transcribe $0.0045/min, gpt-4o-transcribe $0.006/min, gpt-4o-mini-transcribe $0.003/min.
  - "Regional processing (data residency) endpoints are charged a 10% uplift for models released on or after March 5, 2026".
  - [OpenAI pricing](https://developers.openai.com/api/docs/pricing)
  - gpt-live-transcribe default rate limits: Build tier 2,000 RPM / 210k TPM. — [OpenAI model page](https://developers.openai.com/api/docs/models/gpt-live-transcribe)
- **ElevenLabs:**
  - Scribe v2 Realtime costs $0.39/h; keyterm prompting +$0.080/h; transcript editing +$0.120/h.
  - Billing is per second of audio sent, "including silence, with no silence discount".
  - Included realtime hours by plan (row mapping inferred by position): Free ≈2h30, Starter 15 h, Creator 56 h, Pro 254 h, Scale 767 h, Business 2,538 h.
  - [ElevenLabs API pricing](https://elevenlabs.io/pricing/api); [ElevenLabs STT capabilities](https://elevenlabs.io/docs/capabilities/speech-to-text)
- **Mistral:**
  - Voxtral Mini Transcribe Realtime 3: $0.008/min of input. — [Mistral docs, model page](https://docs.mistral.ai/models/voxtral-mini-transcribe-realtime-3-0); [Mistral news, Voxtral 3](https://mistral.ai/news/voxtral-3)
  - The previous Voxtral Realtime 2602 cost $0.006/min. — search summary of [Mistral news, Voxtral Transcribe 2](https://mistral.ai/fr/news/voxtral-transcribe-2)
- **Google:**
  - Official pricing page (seen via search snippet): V2 at $0.016/min for the first 500,000 min/month, then $0.01, $0.008 and $0.004; dynamic batch $0.003/min; rounded up to 1 s per request. — [Google Cloud STT pricing](https://cloud.google.com/speech-to-text/pricing) (direct fetch truncated; verify)
  - Gemini 3.5 Transcribe supports Standard PayGo, but the doc lists no rates. — [Google Cloud docs, Gemini 3.5 Transcribe](https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/gemini/3-5-transcribe)
  - A third-party review cites about $0.005/min for the live model (unverified). — [eesel.ai](https://eesel.ai/blog/gemini-3-5-transcribe-review)
- **AWS:**
  - The pricing page's example prices streaming at $0.01/min in US East. Billing is per second with no minimum. Language identification is included at no extra charge. The free tier is 60 min/month for 12 months. — [AWS Transcribe pricing](https://aws.amazon.com/transcribe/pricing/)
  - This conflicts with third-party guides listing $0.024/min T1 for standard transcription. — [costbench](https://costbench.com/software/ai-transcription-apis/aws-transcribe)
- **Azure:**
  - The global pricing page shows "$-" placeholders. F0 free tier: 5 audio hours/month of standard real-time, shared with Custom; batch is not on F0.
  - Continuous LID and diarization are per-hour real-time add-ons.
  - MAI-Transcribe-2 has a limited-time promo through 2026-12-31.
  - [Azure Speech pricing](https://azure.microsoft.com/en-us/pricing/details/speech/)
  - Azure China lists the continuous-LID and diarization add-ons at ¥3.05/h per feature. — [Azure China pricing](https://www.azure.cn/en-us/pricing/details/cognitive-services/)
  - MAI-T2-Streaming intro price: $0.54/audio hour through year-end (secondary). — [testingcatalog](https://testingcatalog.com/microsoft-launches-mai-transcribe-2-streaming); official pricing points to [Foundry Models pricing (Microsoft)](https://azure.microsoft.com/pricing/details/ai-foundry-models/microsoft/) (not fetched)

### Inferences

- **Approximate cost per 1-hour interview with 2 continuous streams:**

  | Provider                   | Cost                          |
  | -------------------------- | ----------------------------- |
  | ElevenLabs                 | ≈ $0.78 ($0.94 with keyterms) |
  | Mistral RT3                | ≈ $0.96                       |
  | MAI-T2-S intro             | ≈ $1.08                       |
  | Google Chirp 3             | ≈ $1.92                       |
  | OpenAI gpt-live-transcribe | ≈ $2.04 ($2.24 EU)            |

  Gating streams with a local VAD (only sending speech) would reduce this for per-second-billed providers. ElevenLabs bills silence too, so only if silence is not sent.

### Gaps

- Azure classic real-time USD rate and the official MAI-T2-Streaming price could not be read.
- AWS streaming tier rates for EU regions were not read.
- Official Gemini 3.5 Transcribe Live pricing was not found.
- ElevenLabs realtime concurrency limits per plan are not on the pricing page.
- Mistral free-tier limits for realtime were not found.

---

## 7. Data handling: retention, training use (default and opt-out), EU regions / EU data boundary, GDPR DPA, zero data retention

### Takeaway

**Strongest defaults:**

- **Azure real-time:** in memory only, "no data stored at rest". Not retained for real-time or fast transcription.
- **Google:** no data logging unless you opt in.
- **OpenAI:** no training by default, ZDR-eligible, and an EU residency endpoint that requires ZDR/MAM.

**Weakest defaults:**

- **AWS:** may store and use inputs to improve the service unless you opt out via an AWS Organizations policy.
- **ElevenLabs:** zero retention is enterprise-only.
- **Mistral:** EU-hosted, but ZDR does not list the realtime endpoint, and training opt-out depends on plan.

### Cited Findings

- **Azure:**
  - "For real-time speech to text, audio input is processed only on the Azure's server memory, and no data is stored at rest". Also: "When doing real-time speech to text, fast transcription … Microsoft does not retain or store the data provided by customers".
  - Diarization voice signals are discarded after processing.
  - [MS Learn, Data, privacy and security for STT (ms.date 2026-03-31)](https://learn.microsoft.com/en-us/azure/ai-foundry/responsible-ai/speech-service/speech-to-text/data-privacy-security)
  - MAI-T2-Streaming is served from Sweden Central (EU), Central US, East US 2 and Southeast Asia, "accessed globally" with routing. — [MS Learn, MAI-T2-Streaming overview](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/mai-transcribe-2-streaming)
  - The regions table marks swedencentral as "Global" deployment for MAI-T2-Streaming (search snippet). — [MS Learn, Speech regions](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/regions)
- **Google:**
  - "By default, Cloud Speech-to-Text does not log customer audio data or transcripts". Data logging is a per-project opt-in in exchange for discounted pricing, and Google keeps the models trained on that data. — [Google Cloud docs, Data logging](https://docs.cloud.google.com/speech-to-text/docs/data-logging)
  - Chirp 3 is GA in the `eu` multi-region. — [Google Cloud docs, Chirp 3](https://docs.cloud.google.com/speech-to-text/v2/docs/chirp_3-model)
  - Gemini 3.5 Transcribe endpoints exist in `global`, `us` and `eu`, with ML processing "multi-region for the United States and Europe". — [Google Cloud docs, Gemini 3.5 Transcribe](https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/gemini/3-5-transcribe)
- **AWS:**
  - "Amazon Transcribe may store and use voice inputs processed by the service solely to provide and maintain the service" and to improve it. Opt out through an AWS Organizations AI services opt-out policy.
  - Some content "may be stored in another Region for service improvement, unless you opt out".
  - The FAQ does not mention GDPR.
  - [AWS Transcribe FAQs](https://aws.amazon.com/transcribe/faqs/)
- **OpenAI:**
  - API data is not used for training by default. Abuse-monitoring retention is up to 30 days.
  - Table entries: `/v1/realtime` — training No, 30 days retention, ZDR-eligible Yes. `/v1/audio/transcriptions` — retention None, ZDR-eligible Yes.
  - EU data residency through `eu.api.openai.com` covers `/v1/realtime` and `/v1/realtime/transcription_sessions`, with regional storage and processing in the US and Europe. It "Requires Zero Data Retention, Modified Abuse Monitoring, Private Retention with PSP, or Safety Retention".
  - The eligible-model lists name gpt-realtime* and whisper-1, gpt-4o-transcribe, gpt-4o-mini-transcribe and gpt-transcribe. gpt-live-transcribe is not named.
  - "Tracing is not currently EU data residency compliant for /v1/realtime".
  - [OpenAI docs, Your data](https://developers.openai.com/api/docs/guides/your-data)
- **ElevenLabs:**
  - `enable_logging=false` (default true) "enables zero retention mode … for enterprise customers only". The EU residency WebSocket base is `wss://api.eu.residency.elevenlabs.io/`. — [ElevenLabs API reference, Realtime STT](https://elevenlabs.io/docs/api-reference/speech-to-text/v-1-speech-to-text-realtime)
  - ZRM covers all `/v1/speech-to-text/` endpoints, and STT audio and text are not logged. It is available "to select enterprise customers". — [ElevenLabs docs, Zero Retention Mode](https://elevenlabs.io/docs/developer-guides/zero-retention-mode)
  - The Enterprise plan offers "Custom terms & assurance around DPA/SLAs". — [ElevenLabs API pricing](https://elevenlabs.io/pricing/api)
  - Default training use for STT is not stated in the official pages I read. A competitor blog (Deepgram) and a reseller claim training is off by default for enterprise only (low reliability). — [Deepgram blog](https://deepgram.com/learn/how-elevenlabs-uses-your-audio-data)
- **Mistral:**
  - The Voxtral 3 post describes "Mistral's EU-hosted API" with European, US and global endpoints. Customers can "control whether any audio is retained", and self-deployment is possible. — [Mistral news, Voxtral 3](https://mistral.ai/news/voxtral-3)
  - ZDR is on request, reviewed case by case. It covers stateless endpoints including `/v1/audio/transcriptions`; realtime is not listed. ZDR and training opt-out are separate controls. — [Mistral docs, Zero data retention](https://docs.mistral.ai/admin/monitor-comply/zero-data-retention)
  - Training: "Scale plans are already opted-out by default", while Experiment (free) plan users must opt out manually in the Admin Console (search summary of the official help center). — [Mistral Help Center](https://help.mistral.ai/en/articles/323757-do-you-use-my-user-data-to-train-your-artificial-intelligence-models)
  - The 2602 realtime model is open-weights under Apache 2.0, so on-prem/edge use is possible (search summary of the official docs). — [Mistral docs, audio](https://docs.mistral.ai/studio-api/audio)

### Inferences

- **For a French/EU-oriented product, the cleanest stories are:**
  - Azure: real-time is non-persistent by design, and MAI-T2-Streaming is in Sweden Central, though "global" routing needs checking for EU-only processing.
  - Google: `eu` multi-region, logging off by default.
  - Mistral: EU company, EU-hosted, and the 2602 open weights allow self-hosting.
  - OpenAI EU residency: needs ZDR/MAM approval, and gpt-live-transcribe eligibility must be confirmed.
- **AWS requires an explicit org-level opt-out** to avoid service-improvement use.

### Gaps

- EU Data Boundary status for Azure Speech/MAI was not fetched.
- Google, Microsoft and OpenAI DPA documents were not fetched; they are standard, but check them.
- Mistral default API retention duration is not published on the pages read.
- Whether Mistral realtime audio can be covered by ZDR is not listed.
- The ElevenLabs official training default for non-enterprise STT was not found.

---

## 8. Integration path from Rust: official SDKs, raw WebSocket/gRPC feasibility, native SDK requirements, short-lived tokens for desktop clients

### Takeaway

Only AWS has an official Rust SDK (`aws-sdk-transcribestreaming`). These all expose documented JSON WebSockets usable directly with `tokio-tungstenite`:

- OpenAI
- ElevenLabs
- Mistral
- Azure MAI-T2-Streaming Realtime API

Google Chirp uses gRPC, so Rust needs `tonic` with the googleapis protos.

Classic Azure real-time features (continuous LID, Post-Stream Refinement) are documented only through the native Speech SDKs. Rust then means either a community binding over the C API or an unofficial pure-Rust reimplementation of the undocumented protocol.

Short-lived client credentials exist at every provider:

- OpenAI ephemeral client secrets.
- ElevenLabs single-use tokens (15 min).
- Azure Entra bearer tokens.
- AWS SigV4 presigned URLs.

### Cited Findings

- **AWS:**
  - Official Rust crates `aws-sdk-transcribe` and `aws-sdk-transcribestreaming` are listed among the supported SDKs. The CLI does not support streaming. — [AWS docs, Supported languages (SDK table)](https://docs.aws.amazon.com/transcribe/latest/dg/supported-languages.html); [crates.io aws-sdk-transcribestreaming](https://crates.io/crates/aws-sdk-transcribestreaming)
  - WebSocket access uses a SigV4 presigned URL (`X-Amz-Expires=300` in the example) on `wss://transcribestreaming.{region}.amazonaws.com:8443/stream-transcription-websocket`. HTTP/2 is also available. — [AWS docs, LID with streaming](https://docs.aws.amazon.com/transcribe/latest/dg/lang-id-stream.html)
- **Azure:**
  - Continuous LID is "only supported with Speech SDKs in C#, C++, Java, JavaScript, and Python". — [MS Learn, Language identification](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/language-identification)
  - Phrase list SDK languages: C#, C++, Java, JS, Python, Go, plus CLI. — [MS Learn, Phrase list](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/improve-accuracy-phrase-list)
  - The community crate `cognitive-services-speech-sdk-rs` provides "Rust bindings … thin abstraction around native C API" (historically Linux-only builds). — [crate mirror](https://crates-io.ganymede-static.progval.net/crates/cognitive-services-speech-sdk-rs/0.2.2)
  - Unofficial pure-Rust crates `azure-speech` and questnet's library aim "to avoid the need for the C++ SDK dependency". — [questnet/azure-speech-sdk-rs](https://github.com/questnet/azure-speech-sdk-rs); [azure-speech crate](https://rust-digger.code-maven.com/crates/azure-speech)
  - MAI-T2-Streaming Realtime API is a plain WebSocket. Auth is a "Microsoft Entra (recommended)" bearer token in `Authorization`, or `api-key` as a header or query parameter. The Python sample uses scope `https://cognitiveservices.azure.com/.default`. — [MS Learn, MAI-T2-Streaming Realtime API](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/mai-transcribe-2-streaming-realtime)
- **OpenAI:**
  - WebSocket `wss://api.openai.com/v1/realtime?intent=transcription`; client secrets are referenced for client auth. — [OpenAI docs, Realtime transcription](https://developers.openai.com/api/docs/guides/realtime-transcription)
  - Ephemeral client secrets (`ek_…`) are "short-lived tokens that can be passed to a client app". — [OpenAI API reference, create realtime client secret](https://developers.openai.com/api/docs/api-reference/realtime-sessions/create-realtime-client-secret)
- **ElevenLabs:**
  - Client-side auth uses a single-use token created server-side (`tokens.singleUse.create("realtime_scribe")`) that "automatically expires after 15 minutes". "Never expose your API key to the client." — [ElevenLabs docs, STT streaming cookbook](https://elevenlabs.io/docs/developers/guides/cookbooks/speech-to-text/streaming)
  - It is passed as the `token` query parameter (alternative: `xi-api-key` header). — [ElevenLabs API reference, Realtime STT](https://elevenlabs.io/docs/api-reference/speech-to-text/v-1-speech-to-text-realtime)
- **Mistral:**
  - WebSocket `wss://api.mistral.ai/v1/audio/transcriptions/realtime?model=…`, SDK `pip install mistralai[realtime]` (Python; a TypeScript tab exists). Auth is an API key, and the page links a separate "Client authentication" page for browsers. — [Mistral docs, realtime transcription](https://docs.mistral.ai/studio/audio/speech_to_text/realtime_transcription)
  - Pipecat wraps it through the Mistral SDK's `RealtimeConnection`. — [Pipecat docs, Mistral STT](https://docs.pipecat.ai/api-reference/server/services/stt/mistral.md)
- **Google Gemini Live:** Uses the `BidiGenerateContent (Live) API`. — [Google Cloud docs, Gemini 3.5 Transcribe](https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/gemini/3-5-transcribe)

### Inferences

- **Lowest-friction Rust paths:**
  - OpenAI, ElevenLabs, Mistral and Azure MAI-T2-Streaming via `tokio-tungstenite` plus `serde_json` plus base64. OpenAI and Azure MAI share an almost identical event schema, so one client could target both.
  - AWS via the official crate.
  - Google Chirp via `tonic` with the googleapis `speech/v2` protos (Google StreamingRecognize is gRPC bidi; no official Rust Google Cloud Speech crate was confirmed).
- **Desktop secret handling:** For a distributed desktop app, avoid shipping long-lived keys. Mint per-session tokens from a small backend:
  - OpenAI client secret.
  - ElevenLabs single-use token.
  - Entra token for Azure; Azure Speech's classic STS token would be an alternative but was not verified this session.
  - AWS presigned URL or Cognito credentials.
  - Mistral client auth (details not fetched).

### Gaps

- Mistral's "Client authentication" mechanism (short-lived tokens?) was not fetched.
- No official Rust SDK was confirmed for OpenAI, Google Speech, ElevenLabs or Mistral (community crates likely exist but were not checked).
- The Azure classic Speech WebSocket wire protocol is not officially documented; community crates reverse-engineer it.
- Windows build support for `cognitive-services-speech-sdk-rs` is unknown.

---

## 9. Published French WER or code-switching benchmarks

### Takeaway

No independent French-specific or FR/EN code-switching streaming benchmark was found. All available numbers are vendor or vendor-relayed multilingual averages:

- ElevenLabs puts French in its "≤5% WER" tier.
- Mistral reports FLEURS averages: realtime about 5–9% depending on delay.
- Google reports Gemini 3.5 Transcribe at 4.0% streaming average WER (via Artificial Analysis, secondary).
- Microsoft MAI-T2-Streaming: 2.5% WER (secondary). Post-Stream Refinement: about 10% relative WER reduction, "double-digit" on code-switched speech (vendor).
- OpenAI gpt-live-transcribe: 9.6% vs 11.65% for gpt-realtime-whisper (vendor, language unspecified).

### Cited Findings

- **ElevenLabs:**
  - French is in the "Excellent (≤ 5% WER)" tier; the benchmark is not named. — [ElevenLabs docs, STT overview](https://elevenlabs.io/docs/overview/capabilities/speech-to-text)
  - Vendor claim: "93.5% accuracy across 30 commonly used European and Asian languages" (FLEURS). — [ElevenLabs realtime STT API page](https://elevenlabs.io/realtime-speech-to-text-api)
- **Mistral:**
  - Voxtral 3 FLEURS-45: Small Transcribe 3 about 4.1% WER (forced language) vs Whisper large-v3 about 16%.
  - Realtime FLEURS-35 sits roughly in a 5–9% range across delay settings vs a 4.0% offline baseline. Chart values were hard to map, so treat them as approximate.
  - No French-specific figure.
  - [Mistral news, Voxtral 3](https://mistral.ai/news/voxtral-3)
- **Google:** Gemini 3.5 Transcribe reports 4.0% average WER streaming and 2.6% non-streaming, "as measured by Artificial Analysis" (secondary relay). — [Let's Data Science](https://letsdatascience.com/news/google-releases-gemini-35-transcribe-models-fcddfe2d)
- **Microsoft:**
  - MAI-T2-Streaming: 2.5% WER and 0.13 s latency (secondary; Chinese financial press). — [10jqka, 2026-10-02](https://stock.10jqka.com.cn/20261002/c680431562.shtml)
  - Post-Stream Refinement: "~10% relative reduction in average WER across Tier-1 locales" and "double-digit relative reductions" on "multilingual or code-switched speech" (vendor, internal testing). — [Argon Systems republication](https://argonsys.com/microsoft-cloud/library/for-the-first-time-real-time-transcription-goes-multilingual/)
- **OpenAI:** gpt-live-transcribe at 9.6% error rate vs gpt-realtime-whisper at 11.65% (vendor figures relayed by press). — [Montevideo Portal](https://www.montevideo.com.uy/Ciencia-y-Tecnologia/OpenAI-lanza-nuevos-modelos-de-IA-para-transcribir-voz-y-conversaciones-en-tiempo-real-uc969953)
- **AWS:** No published French WER found.

### Inferences

- **Cross-vendor averages are not comparable:** test sets differ (FLEURS-30/35/45, internal sets, AA's set). FLEURS is read speech, not spontaneous interview speech with English jargon.
- **A small in-house eval set is the only reliable way to rank providers for this use case.** For example, 30–60 min of French tech-interview audio with code-switched questions, scored on WER plus "tech-term recall" plus end-of-speech-to-final latency.

#### Cross-provider fit summary for this use case (inference, based on the cited findings above)

- **Azure MAI-Transcribe-2-Streaming (preview):**
  - Pros: 16 kHz PCM16 native, 10–20 ms chunks, OpenAI-like WebSocket (easy from Rust), multilingual by default including fr/en, client commit for fast finals, Sweden Central region, Azure's "no data at rest" for real-time.
  - Cons: preview with no SLA, streaming keyword biasing undocumented, no language/confidence in SDK output (secondary).
- **Azure classic Speech SDK:**
  - Pros: mature phrase lists (2,000 entries, weight) and diarization.
  - Cons: continuous LID explicitly can't switch within a sentence, and native SDK only, so Rust needs C-API bindings.
  - Use with a fixed fr-FR locale plus phrase list, or the Post-Stream Refinement preview.
- **Google Gemini 3.5 Transcribe Live (GA Oct 2026):**
  - Pros: explicit code-mixing, 16 kHz PCM, `eu` region, 1,000-term biasing, manual activity markers.
  - Cons: 10-minute session cap (needs reconnection logic), no live diarization, pricing unverified.
- **Google Chirp 3:** gRPC, SHORT/SUPERSHORT endpointing, `eu` GA, logging off by default. Streaming "auto"/multi-language is unconfirmed.
- **AWS Transcribe:**
  - Pros: official Rust SDK, multi-language ID including mid-conversation switches, fr-FR custom vocabulary, dual-channel option.
  - Cons: needs ≥2 candidate languages, service-improvement data use unless an org-level opt-out is set, pricing ambiguous.
- **OpenAI gpt-live-transcribe:**
  - Pros: language hints `["fr","en"]`, prompt plus keywords, tunable delay.
  - Cons: 24 kHz input only (resample), no server VAD (client commit), no language/speaker/confidence output, highest price (~$1.02/h/stream), EU residency requires ZDR/MAM and model eligibility is unconfirmed.
- **ElevenLabs Scribe v2 Realtime:**
  - Pros: cheapest list price ($0.39/h), pcm_16000 default, VAD/manual/turn_prediction commits, EU residency endpoint, single-use tokens.
  - Cons: only 50×20-char keyterms (paid add-on), ZRM enterprise-only, no realtime diarization, code-switching behavior undocumented.
- **Mistral Voxtral Realtime 3 (preview, 8 days old):**
  - Pros: EU company and EU-hosted, $0.48/h, 80–2400 ms delay, native end-of-turn, in-stream diarization and biasing (claimed), open-weights predecessor (2602, Apache 2.0) for a local fallback.
  - Cons: model page lacks languages/format/params, biasing "optimized for English", ZDR doesn't list realtime.

### Gaps

- No independent French or FR/EN code-switching streaming WER/latency benchmark (e.g. Artificial Analysis per-language results) was retrieved.
- No AWS French accuracy data found.
- Artificial Analysis pages for these models were not fetched directly.
