# Real-time STT for a French/English interview copilot: independent benchmarks (French, FR-EN code-switching) and EU/GDPR data-protection posture (research as of 2026-10-28)

Scope reminder: a Windows desktop app streams two live audio channels (system loopback = interviewer, mic = candidate) to a cloud STT. Main language French with English technical terms inside the same sentence, plus fully English questions. The user is in France. Every number and policy below carries a date where one was found. "Not found" means the research did not locate it, not that it does not exist.

---

## Q1. What do independent (non-vendor) streaming benchmarks say about accuracy and latency, and do any cover French?

### Takeaway

The two most-cited third-party streaming leaderboards (Artificial Analysis and Coval, snapshot of 16 Oct 2026) put the fastest time-to-final at about 54-140 ms (Soniox, Deepgram, ElevenLabs, AssemblyAI) and the lowest streaming WER at about 2-4% (AssemblyAI, xAI, Microsoft MAI, ElevenLabs, Gemini). Neither says it tests French, and Artificial Analysis's datasets look English-only. No independent French streaming leaderboard was found, so any French or FR-EN ranking has to come from batch benchmarks or your own test.

### Cited Findings

**Artificial Analysis (AA), streaming leaderboard: methodology**

- AA's streaming page compares "AA-WER Streaming Index", time to final transcription, time to first partial, and price per 1,000 minutes. It showed "32 of 47 models" when fetched on 2026-10-28. — [Artificial Analysis streaming](https://artificialanalysis.ai/speech-to-text/streaming)
- Test set: about 8 hours of audio, weighted AA-AgentTalk 50%, VoxPopuli 25%, Earnings22 25%. The page does not say which languages are tested and shows no "last updated" date (only "© 2026"). — [Artificial Analysis streaming](https://artificialanalysis.ai/speech-to-text/streaming)
- How latency is measured: the clock starts at the end of speech detected by SileroVAD. Models with forced endpointing are timed to the next final transcript. Otherwise AA takes the first natural final within 2 s, falling back to partials. Time to first partial is measured to the first transcript-bearing event after speech ends. — [Artificial Analysis streaming](https://artificialanalysis.ai/speech-to-text/streaming)
- Providers in AA's streaming legend include Deepgram, AssemblyAI, Speechmatics, Soniox, OpenAI, ElevenLabs, Mistral, Google, Amazon (via Bedrock) and "Microsoft AI". **Gladia was not in the legend**, and Azure AI Speech is not named. — [Artificial Analysis streaming](https://artificialanalysis.ai/speech-to-text/streaming)

**AA streaming numbers (as transcribed by Micdrop, snapshot 16 Oct 2026; Mistral on 22 Oct 2026). Metric is WER / time to final after end of speech**

- ElevenLabs Scribe v2 Realtime 3.6% / 140 ms. Deepgram Nova-3 6.7% / 60 ms. Soniox stt-rt-v5 4.5% / 54 ms. Google Gemini 3.5 Transcribe Live 4.0% / 400 ms. xAI Grok Voice Transcribe 2.0 2.7% / 490 ms. Mistral Voxtral Realtime (first version) 5.3% / latency not given. Rev AI 10.1%. Soniox's previous version (June 2026) 11.9%. — [Micdrop, 16 Oct 2026 (updated 22 Oct)](https://micdrop.dev/blog/top-speech-to-text-apis-voice-agents)
- Listed outside the ranking: Microsoft MAI-Transcribe-2-Streaming 2.5% / 128 ms (1 Oct 2026) and Meta Muse Voice Transcribe 3.1% / 160 ms. — [Micdrop](https://micdrop.dev/blog/top-speech-to-text-apis-voice-agents)
- Micdrop is an open-source TypeScript library that does no transcription itself. Its maintainer says his own app runs Gladia first with OpenAI as fallback, and no commercial relationship with any vendor is disclosed. — [Micdrop](https://micdrop.dev/blog/top-speech-to-text-apis-voice-agents)

**Coval numbers (as transcribed by Micdrop, 16 Oct 2026). Metric is WER / time to final after end of speech**

- AssemblyAI "Universal-3.6 Pro" 2.1% / 134 ms. Deepgram Nova-3 5.3% / 85 ms. Deepgram Flux 5.8% / 98 ms. ElevenLabs Scribe v2 Realtime 4.3% / 121 ms. Gemini 3.5 Transcribe Live 3.0% / 321 ms. Speechmatics "Linden 1" 3.9% / 275 ms. OpenAI gpt-4o-transcribe 3.7% / 786 ms. Mistral Voxtral Realtime (v1) 5.3% / 418 ms. Gladia Solaria-1 6.1% / 556 ms. Median across all models Coval tracks: 195 ms. Soniox is not listed. — [Micdrop](https://micdrop.dev/blog/top-speech-to-text-apis-voice-agents)
- Micdrop warns the two benchmarks use different audio and timing, so APIs should only be compared within the same benchmark. — [Micdrop](https://micdrop.dev/blog/top-speech-to-text-apis-voice-agents)
- Coval sells evaluation and monitoring tooling. It calls itself "vendor-agnostic", but its own 2026 article reports almost exclusively vendor-reported figures, with inconsistent metrics (WER vs accuracy vs relative reduction, and TTFT vs end-of-turn vs unspecified latency). The article never names French. It asserts, without a source, that streaming WER is "typically 1-3 percentage points worse" than batch. — [Coval blog](https://www.coval.ai/blog/best-speech-to-text-providers-in-2026-independent-benchmarks-and-how-to-choose/)

**AA non-streaming leaderboard (batch, for context)**

- Per a search snapshot of the AA page in Oct 2026: Fun-Realtime-ASR-preview had the lowest AA-WER at 1.7% across 59 models, then MAI-Transcribe-2 2.0%, Voxtral Small Transcribe 3 2.1%, ElevenLabs Scribe v2 2.2% and Voxtral Mini Transcribe 3 2.2%. — [Artificial Analysis non-streaming](https://artificialanalysis.ai/speech-to-text/non-streaming)
- AA posted that Mistral's Voxtral Small Transcribe 3 took #3 on the leaderboard with a 2.1% AA-WER (post about one week before 2026-10-28). — [Artificial Analysis on X](https://x.com/ArtificialAnlys/status/2112587905222709432)

**Hugging Face Open ASR Leaderboard (batch only)**

- The paper (arXiv 2510.06961v1, 8 Oct 2025) uses batch WER and RTFx measured on an NVIDIA A100. **No streaming evaluation.** — [Open ASR Leaderboard paper](https://arxiv.org/html/2510.06961v1)

### Inferences

- AA-AgentTalk, Earnings22 and (very likely) the VoxPopuli subset AA uses are English. AA's streaming WER should therefore be read as an **English** streaming ranking. It says nothing reliable about French or FR-EN accuracy. AA's latency numbers are more transferable, because the endpointing and finalization mechanics do not depend on language (network distance from France to the provider's region still adds to them).
- Within both benchmarks, Deepgram Nova-3 and Soniox are the latency leaders, and ElevenLabs Scribe v2 RT and AssemblyAI combine low latency with low WER. OpenAI gpt-4o-transcribe (786 ms on Coval) and Gladia Solaria-1 (556 ms) are comparatively slow to finalize.
- Model names change fast (AssemblyAI "Universal-3.5 Pro" in the docs vs "Universal-3.6 Pro" in the Coval table, and Speechmatics "Linden 1" vs "Ursa 2" in older material). Pin the model ID when you test.

### Gaps

- No date-stamped raw export of AA's streaming chart was retrievable. The numbers above are Micdrop's transcription, not checked directly against the AA page.
- AA's streaming language mix is not stated on the page (inference above). No AA French or multilingual streaming index was found.
- Not found: any independent streaming latency figure for Azure AI Speech, AWS Transcribe streaming, or Gladia's current model (only Solaria-1 on Coval).
- Coval's own dashboard methodology (dataset, language, region) was not retrieved.

---

## Q2. Which independent benchmarks cover French and French-English code-switching specifically, and what do they show?

### Takeaway

Only batch, mostly synthetic evidence exists. The most relevant study is CoSE-E (arXiv, 28 Sep 2026, EMNLP 2026 workshop). Its FR/EN subset puts ElevenLabs Scribe v2 (3.1% WER), AssemblyAI (3.9%) and Gemini 3 Flash (4.0%) ahead of Deepgram Nova-3 (5.2%), Voxtral Small (6.0%) and Parakeet (7.5%). Whisper-large-v3-turbo collapses (43.4% WER) because it locks onto one language and translates the rest. On monolingual French, the Open ASR Leaderboard (Oct 2025) shows open models at about 4.9-6.6% average WER.

### Cited Findings

**CoSE-E: code-switched enterprise benchmark (independent academic/industry authors)**

- Published as arXiv:2609.35645, submitted 28 Sep 2026 and accepted at the SALMA Workshop (oral) at EMNLP 2026. Authors: Shama Gupta, Hoang H Nguyen, Chelsea Huang, Lindsay Devon Brin, Fanny Riols. — [arXiv abstract](https://arxiv.org/abs/2609.35645)
- The dataset is **synthetic**: 1,212 utterances across 5 language pairs (HR and IT service workflows), including FR/EN (298 utterances) and FR-CA/EN (188). Metrics are WER, semantic WER (SWER) and Answer Error Rate (AER). — [search summary of arXiv 2609.35645](https://arxiv.org/pdf/2609.35645)
- Every model ran with automatic language detection and no language parameter. The paper does not say whether any system ran in streaming mode. — [CoSE-E HTML](https://arxiv.org/html/2609.35645)
- FR/EN results (WER / SWER / AER) — [CoSE-E HTML, Table 2](https://arxiv.org/html/2609.35645):

  | Model                                                         | FR/EN WER | SWER  | AER   | FR-CA/EN WER |
  | ------------------------------------------------------------- | --------- | ----- | ----- | ------------ |
  | ElevenLabs Scribe-v2                                          | 0.031     | 0.006 | 0.051 | 0.041        |
  | Google Gemini-3-Flash                                         | 0.040     | 0.009 | 0.054 | 0.055        |
  | AssemblyAI (text: "Universal-3.5-Pro"; link: universal-3-pro) | 0.039     | 0.009 | 0.062 | 0.052        |
  | Qwen3-Omni-Instruct                                           | 0.061     | 0.010 | 0.055 | 0.071        |
  | Mistral Voxtral-Small-24B                                     | 0.060     | 0.012 | 0.068 | 0.059        |
  | NVIDIA Parakeet-TDT-0.6b-v3                                   | 0.075     | 0.024 | 0.084 | 0.099        |
  | Deepgram Nova-3 Multilingual                                  | 0.052     | 0.019 | 0.065 | 0.072        |
  | OpenAI Whisper-Large-v3-Turbo                                 | 0.434     | 0.061 | 0.111 | 0.535        |

- The paper ranks FR/EN as the **hardest pair** on SWER and AER, with "the most expensive and most consistent code-switching cost". — [CoSE-E HTML](https://arxiv.org/html/2609.35645)
- Whisper failure mode: under automatic language ID it "detects a single language and translates the rest of the speech into English". — [CoSE-E HTML](https://arxiv.org/html/2609.35645)
- About 57% of errors that reach AER involve a corrupted critical entity (ID, hostname, URL, email). When code-switching introduces an entity error, AER worsens about 40% of the time vs a base rate of about 5%, roughly 8x the task-failure risk. Overall the paper finds the code-switching cost "real but largely surface-level": about 80% of utterances show no change in the downstream answer. — [CoSE-E HTML](https://arxiv.org/html/2609.35645)

**Other code-switching studies (not French, but indicative)**

- "Benchmarking Commercial ASR Systems on Code-Switching Speech: Arabic, Persian, and German" (arXiv 2605.19069, May 2026) benchmarks 5 commercial providers. ElevenLabs Scribe v2 is best at 13.2% overall WER and 0.936 BERTScore. The authors note that WER penalizes semantically correct transliteration choices. — [arXiv 2605.19069](https://arxiv.org/abs/2605.19069)
- A zero-resource code-switched speech benchmark (arXiv 2310.03018) has a French-English track of 4,020 human-validated samples synthesized with Amazon Polly voices. It targets self-supervised speech encoders, not end-to-end ASR. — [search summary / SigPort PDF](https://sigport.org/sites/default/files/docs/2310.03018_0.pdf)
- A systematic literature review of end-to-end code-switching ASR counts only 3 papers on French-English. — [arXiv 2507.07741](https://arxiv.org/html/2507.07741v1)

**Monolingual French (batch)**

- Open ASR Leaderboard paper (8 Oct 2025), French average WER over its French multilingual sets (CoVoST-2 and FLEURS per search summary; per-dataset values not broken out): NVIDIA Canary 1B v2 4.86%, Microsoft Phi-4-Multimodal 5.13%, NVIDIA Parakeet TDT 0.6B v3 5.38%, Mistral Voxtral Mini 3B 5.96%, OpenAI Whisper Large v3 6.59%, ElevenLabs Scribe v1 15.1% (the only proprietary system in that table). — [Open ASR Leaderboard paper](https://arxiv.org/html/2510.06961v1)
- The same paper says Whisper-derived fine-tunes often trade multilingual coverage for English accuracy, and that broader language coverage in NVIDIA's models "comes at the cost of English transcription accuracy". — [Open ASR Leaderboard paper](https://arxiv.org/html/2510.06961v1)
- "A Benchmark of French ASR Systems Based on Error Severity" (Tholly, Wottawa, Rouvier, Dufour; COLING 2025; arXiv 18 Jan 2025) evaluates 10 French ASR systems using a four-level error-severity taxonomy. Per-system results were not retrieved. — [arXiv 2501.10879](https://arxiv.org/abs/2501.10879)

**Vendor-published or low-independence French/multilingual claims (VENDOR CLAIMS, not independent)**

- Speechmatics claims Ursa 2 is "35% better" on code-switching (its own benchmark) and an 18% relative WER reduction over Ursa 1 across 55 languages. — [as reported by Coval](https://www.coval.ai/blog/best-speech-to-text-providers-in-2026-independent-benchmarks-and-how-to-choose/)
- ElevenLabs claims 93.5% accuracy for Scribe v2 Realtime across 30 European and Asian languages (11 Nov 2025). Microsoft claims 3.8% average FLEURS WER over 25 languages for MAI-Transcribe-1 (2 Apr 2026). — [as reported by Coval](https://www.coval.ai/blog/best-speech-to-text-providers-in-2026-independent-benchmarks-and-how-to-choose/)
- Gladia's own blog claims about 103 ms partial and about 270 ms final latency. — [search summary of Gladia blog](https://www.gladia.io/blog/best-speech-to-text-apis)
- VexaScribe, a transcription vendor, puts Whisper Large-v3 at about 5-7% WER on clean French audio, Deepgram Nova-3 and AssemblyAI Universal-2 at 6-9%, and all of them at 12-25% on noisy audio (July 2026). This source has a conflict of interest and its numbers should not be relied on. — [VexaScribe (FR)](https://vexascribe.com/fr/blog/comparaison-precision-transcription)

### Inferences

- For "Comment tu gères le state management dans Vue ?"-style speech, CoSE-E is the closest public evidence. It suggests Scribe v2, AssemblyAI's Universal-3.x Pro and Gemini handle FR/EN mixing best in batch, with Deepgram Nova-3 Multilingual somewhat behind. Vanilla Whisper with auto language ID is unsuitable for code-switched French unless the language is forced (forcing brings its own risk on English terms).
- CoSE-E's entity finding matters for an interview copilot: hostnames, library names and acronyms are the tokens most likely to break and most damaging to the downstream LLM answer. Keyterm or context biasing (Q3) targets exactly this.
- CoSE-E is synthetic (TTS-style), batch, and auto-LID only. Real spontaneous interview speech with loopback compression will likely score worse, and streaming typically adds WER (Coval's unsourced 1-3 pp claim).

### Gaps

- No independent **streaming** benchmark of French or FR-EN code-switching was found.
- No naturally recorded (non-synthetic) FR-EN code-switching ASR benchmark with commercial systems was found.
- CoSE-E does not cover Soniox, Speechmatics, Gladia, Azure, AWS, OpenAI gpt-4o-transcribe or gpt-transcribe. Their FR/EN performance is not independently measured.
- Per-dataset French figures (FLEURS-fr, MLS-fr, Common Voice-fr) for current commercial models were not found. The live HF leaderboard space could not be read as text.

---

## Q3. What are the known code-switching failure modes, and what mitigations does each provider document?

### Takeaway

The main failure modes are language-ID locking (the model picks one language for the whole utterance or session, then mis-hears or translates the other), near-homophone substitution across languages, and corruption of technical entities. Most providers now offer a "multi" or code-switching mode plus a hint list of expected languages and a keyterm or context list. The limits differ a lot (ElevenLabs realtime: 50 keyterms; AssemblyAI: 100; Voxtral: 100 but biasing optimized for English; Google: 1,000 phrases), and Speechmatics has no documented FR-EN bilingual pack.

### Cited Findings

**Failure modes (general)**

- Whisper with auto LID "detects a single language and translates the rest of the speech into English" (CoSE-E, Sep 2026). — [CoSE-E](https://arxiv.org/html/2609.35645)
- Bilingual or language-locked models tend to stay in the language the utterance started in. Near-homophones across languages get picked with high confidence when the language prior is wrong (vendor wiki example: "see" vs "sí"). Existing LID models often fail on English spoken with the matrix language's accent (EACL 2026 Findings). — [Soniox wiki (vendor)](https://soniox.com/wiki/code-switching-speech-recognition); [EACL 2026 Findings](https://preview.aclanthology.org/ingest-eacl/2026.findings-eacl.242.pdf)
- Gladia warns that if the start of the audio contains silence, music or a different language than the main content, "this can lead to incorrect detection for the whole transcription". — [Gladia code-switching docs](https://docs.gladia.io/chapters/language/code-switching)
- Google Chirp 3's `language_codes=["auto"]` detects the **dominant** language. The documented example uses `Recognize`; auto-detection in streaming is not shown. — [Chirp 3 docs, updated 2026-10-26](https://docs.cloud.google.com/speech-to-text/docs/models/chirp-3)

**Provider mitigations (documentation, fetched Oct 2026)**

- **Deepgram:** add `language=multi` to the streaming WebSocket URL (Nova-2, Nova-3, Nova-3 Medical), or use `model=flux-general-multi` on `/v2/listen` for Flux Multilingual. `language_hint` biases detection without restricting it (Nova-3/Flux only; an unsupported code returns HTTP 400). `endpointing=100` is recommended for code-switching. Each word carries a `language` field. — [Deepgram multilingual code-switching docs](https://developers.deepgram.com/docs/multilingual-code-switching)
- Nova-3 Multilingual went GA on 31 Mar 2025 for real-time code-switching across 10 languages including **French and English**. Multilingual keyterm prompting was added in Dec 2025. An updated multilingual model shipped in Feb 2026 with "largest gains in code-switching scenarios". — [Deepgram changelog / search summary](https://deepgram.com/changelog/nova-3-multilingual-ga); [Deepgram 5 Feb 2026 changelog](https://developers.deepgram.com/changelog/2026/2/5); [Deepgram blog, Dec 2025](https://deepgram.com/learn/deepgram-expands-nova-3-with-10-new-languages-and-multilingual-keyterm-prompting.md)
- **AssemblyAI:** Universal-3.5 Pro Streaming supports 18 languages with automatic code-switching, including French, "without any configuration changes". `prompt` and `keyterms_prompt` can be combined, up to 100 keyterms of at most 50 characters each. A cheaper Universal-Streaming Multilingual covers 6 languages including French. One AssemblyAI page says prompt and keyterms are mutually exclusive on Universal-3 Pro, which conflicts with the 3.5 Pro migration guide. AssemblyAI advises starting without keyterms, because large lists of common terms cause over-correction. — [AssemblyAI 3.5 Pro streaming migration guide](https://assemblyai.com/docs/streaming/migration-guides/universal-to-universal-3-5-pro-streaming); [AssemblyAI voice agent best practices](https://assemblyai.com/docs/voice-agents/best-practices); [AssemblyAI pricing (md)](https://www.assemblyai.com/md/pricing)
- **Gladia:** set `language_config` with an explicit language list (keep it short, never empty) and `code_switching: true`. Per-utterance language codes are returned. Custom vocabulary is a **post-transcription replacement based on phoneme similarity**, with `pronunciations` and `intensity`. — [Gladia code-switching docs](https://docs.gladia.io/chapters/language/code-switching); [Gladia live recommended parameters](https://docs.gladia.io/chapters/live-stt/recommended-parameters)
- **Speechmatics:** no French-English bilingual pack was found. Documented bilingual models include Spanish-English (real-time and batch) and Arabic-English (2026). `additional_vocab` (with `sounds_like`) raises the weight of listed words. Recall.ai's integration docs say Speechmatics real-time through Recall.ai does not support code-switching (third-party wrapper). — [Speechmatics ES-EN article, 29 Feb 2024](https://www.speechmatics.com/company/articles-and-news/mastering-bilingual-communication-transcribe-english-and-spanish); [BusinessWire, Mar 2026](https://www.businesswire.com/news/home/20260310752540/en); [Recall.ai docs](https://docs.recall.ai/docs/speechmatics); [LiveKit Speechmatics plugin](https://docs.livekit.io/reference/python/v1/livekit/plugins/speechmatics/index.html)
- **Soniox:** per-token language identification within a sentence. `language_hints` biases toward the expected languages without excluding others, and `language_hints_strict` is best-effort. In real-time, a token's language label can be revised as more audio arrives. — [Soniox language detection (vendor)](https://soniox.com/speech-to-text/features/language-detection)
- **Azure AI Speech:** language identification takes up to 4 candidates at start or up to 10 in continuous mode. Microsoft's own example is the fr-FR / en-US pair. Only one locale per base language is allowed (not en-US plus en-GB). A Sept 2025 Microsoft Tech Community post says Speech containers do not support dynamic multi-language switching in one session. — [Azure language identification docs](https://learn.microsoft.com/en-my/Azure/ai-services/speech-service/language-identification); [Tech Community](https://techcommunity.microsoft.com/blog/azure-ai-foundry-blog/-/4448772)
- **Google Chirp 3:** `StreamingRecognize` is supported. fr-FR and fr-CA are GA. Speech adaptation (phrase boost) is GA with up to 1,000 phrases ("use as few as possible"). Diarization and word timestamps are not available in streaming. — [Chirp 3 docs, updated 2026-10-26](https://docs.cloud.google.com/speech-to-text/docs/models/chirp-3)
- **AWS Transcribe streaming:** `IdentifyMultipleLanguages` with `LanguageOptions` (2 or more codes, at most 5 recommended). Custom vocabularies apply only when their language matches the detected language. Not combinable with custom language models or redaction. Language ID is "not supported in all regions". — [AWS StartStreamTranscription API](https://docs.amazonaws.cn/en_us/transcribe/latest/APIReference/API_streaming_StartStreamTranscription.html)
- **OpenAI:** the newer `gpt-transcribe` accepts a `languages` list when audio may contain more than one language, and in Realtime sessions it automatically uses earlier transcribed turns as context. `gpt-4o-transcribe` takes a free-text `prompt` (e.g. "expect words related to technology") and a single ISO-639-1 `language`. — [OpenAI transcription guide](https://developers.openai.com/api/docs/guides/transcription); [OpenAI Realtime API reference](https://developers.openai.com/api/docs/api-reference/realtime)
- **ElevenLabs Scribe v2 Realtime:** keyterm prompting works in realtime, limited to 50 keyterms of at most 20 characters (batch allows 1,000 of 50 characters). It is context-aware and billed as an extra-cost add-on. — [ElevenLabs keyterm prompting](https://elevenlabs.io/docs/eleven-api/guides/how-to/speech-to-text/batch/keyterm-prompting)
- **Mistral Voxtral Realtime:** context biasing with up to 100 words or phrases, but "optimized for English", with other languages "experimental". Open-weight (Apache 2.0), delay configurable below 200 ms, 13 languages. — [Mistral, Voxtral Transcribe 2](https://mistral.ai/news/voxtral-transcribe-2)

### Inferences

- Shared configuration pattern: multi/code-switching mode on, language hints = [fr, en], a short keyterm list of the English tech vocabulary (e.g. "state management", "Vue", "Kubernetes", "useEffect"), and short endpointing. AssemblyAI warns that big keyterm lists over-correct, so keep the list scoped to the job description or stack.
- Mistral's biasing is English-optimized. For French sentences containing English terms, the English terms are what get biased, which may suit this use case, but this is untested.
- Gladia's vocabulary is a post-hoc phonetic replacement, not decoder biasing, so it can fix spellings but not recover words the model never emitted.
- Because the app has two separate channels, a per-channel language prior could help (e.g. interviewer sometimes fully English). Session-level LID locking on one channel would not then contaminate the other.

### Gaps

- Not found: provider documentation of the "English term translated into French" error (e.g. "gestion d'état" emitted for "state management"). Only Whisper's translate-to-English behavior is documented (CoSE-E).
- Not confirmed: whether Deepgram keyterm prompting works with `language=multi` in **streaming** specifically (the Dec 2025 blog title says multilingual keyterm prompting; the code-switching page does not mention it).
- Not confirmed: whether Google Chirp 3 `auto` and multiple `language_codes` work in `StreamingRecognize`, or whether AWS multi-language streaming ID is available in eu-west-3 (Paris) / eu-central-1.
- ElevenLabs and Voxtral code-switching behavior is not documented beyond the language lists.

---

## Q4. Which providers offer EU data residency / an EU endpoint for REAL-TIME streaming specifically?

### Takeaway

Real-time EU endpoints are documented for Deepgram, AssemblyAI, Speechmatics, Soniox, Gladia, OpenAI (Realtime, approval-gated), Google Chirp 3 (`eu` multi-region) and ElevenLabs (Enterprise only). Almost all of them keep account, usage and billing metadata, or support access, outside the EU. "EU residency" usually covers audio and transcripts only.

### Cited Findings

- **Deepgram:** `wss://api.eu.deepgram.com/v1/listen` and `/v2/listen` (Flux). Hosted in the EU, but "the specific country may change over time". Requests are never routed outside the EU, and fail rather than fall back if the region is down. Whisper models are not available in the EU. Pin a specific country via Deepgram Dedicated. — [Deepgram regional endpoints](https://developers.deepgram.com/reference/regional-endpoints). GA was announced on Deepgram's blog, listed as Jan 2026. — [Deepgram blog](https://deepgram.com/learn/deepgram-eu-endpoint-now-generally-available). An older custom-endpoints page still calls the EU endpoint "Early Access" and STT-only (outdated). — [Deepgram custom endpoints](https://developers.deepgram.com/reference/custom-endpoints)
- **AssemblyAI:** `wss://streaming.eu.assemblyai.com/v3/ws` (EU Data Zone). Possible EU processing locations: Frankfurt, Stockholm, Milan, Spain, Ireland, **Paris (eu-west-3)**. Audio, transcripts and prompts are stored in the chosen region. Account and usage data are stored in the US. Team members outside the region may access data for debugging, and a US subprocessor may be used for support. File names may go to US subprocessors CrowdStrike and Datadog. — [AssemblyAI streaming endpoints and data zones](https://assemblyai.com/docs/streaming/endpoints-and-data-zones)
- **Speechmatics:** real-time is available in EU1 and US1 (AU1 is batch only). Enterprise EU1 is `neu.rt.speechmatics.com`; on-demand EU2 is `eu2.rt.speechmatics.com`. A 20 Jul 2026 changelog added `global.rt.speechmatics.com`, which routes to the nearest region; regional endpoints remain "for customers with specific data residency requirements". Calling a region you are not contracted for fails. — [Speechmatics regions](https://docs.speechmatics.com/administration/regions); [Speechmatics authentication](https://docs.speechmatics.com/introduction/authentication); [Speechmatics changelog 20 Jul 2026](https://speechmatics.featurebase.app/changelog/20260720-rt-saas)
- **Soniox:** EU region with full API support: `api.eu.soniox.com` and real-time `stt-rt.eu.soniox.com`, set per project with region-specific API keys. Residency excludes system data (account and project metadata, usage, billing), which "may be processed outside the selected region". — [Soniox data residency](https://soniox.com/docs/stt/data-residency)
- **Gladia:** the live API takes `region=eu-west` or `region=us-west`, and the support article says EU is the default. Gladia's terms say "by default, we use a European provider based in France". — [Gladia support](https://gladia.support.site/article/which-server-for-live-transcription-eu-or-us); [Gladia terms (last updated 09/02/2024)](https://gladia.io/security). Its compliance hub says EU and US workloads run on separate region-specific infrastructure, and HDS hosting is available for enterprise. — [Gladia compliance hub](https://gladia.io/compliance-hub)
- **OpenAI:** `eu.api.openai.com` gives storage **and** processing in Europe/EEA+Switzerland for `/v1/realtime` and `/v1/audio/transcriptions`. Tracing is "not currently EU data residency compliant" for Realtime. Residency is set per project (also selectable per request from a Global project) and requires ZDR, Modified Abuse Monitoring or a similar control with prior OpenAI approval via sales. There is a 10% price uplift for residency-eligible models released on or after 5 Mar 2026. — [OpenAI "Your data" guide](https://developers.openai.com/api/docs/guides/your-data). Per OpenAI's announcement, existing projects cannot be switched to EU residency after creation. — [OpenAI EU data residency announcement](https://openai.com/index/introducing-data-residency-in-europe/)
- **Google Cloud STT (Chirp 3):** GA regions are the `us` and `eu` multi-regions (`eu-speech.googleapis.com`), with more planned. Streaming is supported. — [Chirp 3 docs, updated 2026-10-26](https://docs.cloud.google.com/speech-to-text/docs/models/chirp-3)
- **Azure AI Speech:** real-time audio is "processed only on the Azure's server memory, and no data is stored at rest". — [Microsoft Learn, Speech to text data/privacy (updated 2026-08-26)](https://learn.microsoft.com/en-us/azure/ai-foundry/responsible-ai/speech-service/speech-to-text/data-privacy-security)
- **ElevenLabs:** EU isolated environment at `api.eu.residency.elevenlabs.io` and `wss://api.eu.residency.elevenlabs.io`, "an exclusive feature available to ElevenLabs' Enterprise customers". Storage is in-region, but "processing may nevertheless occur outside of the selected location" (affiliates, subprocessors, support, moderation). The page does not mention Scribe or STT. — [ElevenLabs data residency](https://elevenlabs.io/docs/overview/administration/data-residency.md)
- **Mistral:** a third-party gateway (Opper) lists Mistral's EU route as France-based, with a DPA and "data stays in EU". — [Opper listing for Voxtral Mini Transcribe Realtime](https://opper.ai/mistral/voxtral-mini-transcribe-realtime-2602). Voxtral Realtime is open-weight (Apache 2.0), so it can be self-hosted. — [Mistral](https://mistral.ai/news/voxtral-transcribe-2)
- **AWS Transcribe:** by default, content kept for service improvement "might" be stored in a region outside the one where the service is used (see Q5). — [Amazon Transcribe opt-out docs](https://docs.amazonaws.cn/transcribe/latest/dg/opt-out.html)

### Inferences

- For a France-based user wanting EU-only processing of live audio on standard self-serve plans, the clearest documented options are Deepgram EU, AssemblyAI EU, Soniox EU, Gladia (EU default), Speechmatics EU2, Google Chirp 3 `eu`, and Azure in an EU region. OpenAI's EU Realtime and ElevenLabs' EU environment are gated behind approval or Enterprise.
- A fully local model (Whisper family, Parakeet/Canary, Voxtral Realtime open weights) removes the processor and transfer question entirely, at an accuracy and latency cost on a desktop GPU or CPU (see Q2 on Whisper code-switching).

### Gaps

- Azure: the specific EU regions offering real-time STT (e.g. France Central, West Europe) and the EU Data Boundary scope for Speech were not verified in this research.
- AWS: availability of Transcribe **streaming** in eu-west-3 (Paris) and of multi-language streaming ID in EU regions was not verified.
- AssemblyAI: which streaming models (Universal-3.5 Pro, multilingual) are available on the EU endpoint is not stated.
- Mistral: Mistral's own documentation of API hosting location for Voxtral realtime was not retrieved. Only a third-party gateway listing was found.
- ElevenLabs: whether Scribe v2 Realtime is available in the EU isolated environment is not documented.

---

## Q5. Default retention, training use and opt-out, DPA, certifications and sub-processors per provider

### Takeaway

Providers split into three groups:

- **No training and no storage of real-time data by default:** Soniox, Speechmatics (training only if enabled), Azure (real-time not stored), Gladia (paid plans), OpenAI API (no training since Mar 2023, but 30-day abuse-monitoring logs unless ZDR is approved).
- **Opt-out required, sometimes at a price:** Deepgram (per-request `mip_opt_out`, reportedly higher rates), AssemblyAI (paid plans only; free users cannot opt out), AWS Transcribe (opt-in by default; opt out via AWS Organizations policy), Mistral free tier, ElevenLabs (account setting).
- **Opt-in only:** Google (data logging off by default).

### Cited Findings

**Deepgram**

- Model Improvement Partnership Program: "Participation in this program is voluntary". Opt out per request with `mip_opt_out=true` (or `"mip_opt_out": true` in Voice Agent Settings). "Data from opted-out requests is retained only for the duration necessary to process the request." — [Deepgram MIP docs](https://developers.deepgram.com/docs/the-deepgram-model-improvement-partnership-program)
- Community answers say opting out "will result in an increase in service rates". — [Deepgram community](https://community.deepgram.com/answers/en/opt-out-model-improvement-dotnet-sdk-1308609946127962120)
- One integrator (TechSmith Audiate) states its users are opted out by default. That is the integrator's choice, not Deepgram's default. — [TechSmith support](https://support.techsmith.com/hc/en-us/articles/38994981837325-Opting-In-To-Improve-Speech-to-Text-Results)
- Deepgram's Australia endpoint announcement says operational metadata and billing are processed in the US. No equivalent statement for the EU endpoint was found. — [search summary of Deepgram AU announcement](https://deepgram.com/learn/deepgram-australia-endpoint-now-generally-available)

**AssemblyAI**

- A "Model Improvement Program" toggle controls training, benchmarking and training on de-identified data. **Paid plans only: free users cannot opt out**, set a TTL, or sign a BAA. You can also opt out by emailing data-opt-out@assemblyai.com. Opting out forgoes pricing-page discounts and is not retroactive. ToS section references are inconsistent across pages (4.3(b)/4.6 vs 4.4). — [AssemblyAI data controls](https://assemblyai.com/docs/data-controls); [AssemblyAI opt-out FAQ](https://assemblyai.com/docs/faq/how-to-opt-out-of-data-sharing-for-our-model-improvement-program)
- SOC 2, ISO 27001 and HIPAA BAA are claimed by a third-party comparison and were not verified on AssemblyAI's trust center. — [Sonix article](https://sonix.ai/resources/zh/soc2-compliant-transcription-software-technology/amp/)

**Speechmatics**

- "Realtime SaaS does not store audio files, transcripts, or configuration data." — [Speechmatics security and compliance docs](https://docs.speechmatics.com/administration/security-and-compliance)
- Training uses data in anonymized form only "if directly Enabled by You" (portal toggle), under an addendum that prevails over the main terms. — [Speechmatics data retention terms](https://www.speechmatics.com/legal/data-retention-terms)
- Certifications: ISO/IEC 27001:2022, SOC 2 Type II, GDPR, HIPAA. The Trust Center requires a request and NDA. GDPR Art. 27 EU representative: The DPO Centre Europe Ltd (Dublin). — [Speechmatics security and compliance](https://docs.speechmatics.com/administration/security-and-compliance); [Speechmatics privacy policy](https://speechmatics.com/legal/privacy-policy)

**Gladia**

- Compliance hub (© 2026): data is "never used for model training unless explicitly agreed". Zero Data Retention for paid plans. DPAs available for EU and US customers. Sub-processor list (purposes and locations) via the Trust Center. Badges for SOC 2 Type 2, ISO 27001, HIPAA and GDPR. — [Gladia compliance hub](https://gladia.io/compliance-hub)
- **Conflict:** the older General Terms (last updated 09/02/2024) say user audio "may be used for the training of the models", while Pro and Enterprise users' data will not be. Free-tier audio may therefore be used for training under those terms. — [Gladia terms](https://gladia.io/security)

**Soniox**

- Content sent to the API (audio, transcripts, metadata) "is never used to train or improve our models". — [Soniox data residency docs](https://soniox.com/docs/stt/data-residency)
- Real-time audio and transcripts are not stored, and are "kept in memory only" (vendor marketing pages). — [Soniox vs Whisper page](https://soniox.com/alternative/stt/openai-whisper); [Soniox Danish voice-agent page](https://soniox.com/speech-to-text/use-cases/voice-agents/danish)
- Claims SOC 2 Type 2, ISO/IEC 27001:2022, HIPAA and GDPR (vendor claim, certificates not inspected). — [Soniox site](https://soniox.com/speech-to-text)

**Azure AI Speech**

- For real-time STT, fast transcription, pronunciation assessment and speech translation, "Microsoft does not retain or store the data provided by customers". Batch output can sit in Microsoft storage with `timeToLive`. Microsoft states the customer "is required to obtain all necessary permissions" and notes that audio may be regulated under communications laws (page ms.date 2026-03-31, updated 2026-08-26). — [Microsoft Learn](https://learn.microsoft.com/en-us/azure/ai-foundry/responsible-ai/speech-service/speech-to-text/data-privacy-security)

**Google Cloud Speech-to-Text**

- "By default, Speech-to-Text does not log customer audio data or transcripts." Opt-in data logging lets Google use the data to improve the service in exchange for discounted pricing, set per project. Before enabling, the customer must certify "proper permissions from the data originator(s)". — [Google STT data logging](https://cloud.google.com/speech-to-text/docs/data-logging?hl=zh-tw); [Enable data logging](https://cloud.google.com/speech-to-text/docs/enable-data-logging?hl=it)

**AWS Transcribe**

- "By default, Amazon Transcribe stores and uses voice inputs that it has processed to develop the service". Opt out via an AWS Organizations AI services opt-out policy; opting out deletes stored improvement copies. Improvement content may be stored "in an Amazon Web Services Region outside of the ... Region where you are using the service". — [Amazon Transcribe opt-out](https://docs.amazonaws.cn/transcribe/latest/dg/opt-out.html); [AI services opt-out policies](https://docs.amazonaws.cn/en_us/organizations/latest/userguide/orgs_manage_policies_ai-opt-out.html)

**OpenAI (API)**

- API data is not used for training by default (since 1 Mar 2023). Abuse-monitoring logs are kept up to 30 days. `/v1/realtime` and `/v1/audio/transcriptions` are ZDR-eligible, but ZDR and Modified Abuse Monitoring need "prior approval by OpenAI". — [OpenAI "Your data" guide](https://developers.openai.com/api/docs/guides/your-data)
- A third-party guide says EU projects require approval for modified abuse monitoring or ZDR, which conflicts with the 2025 announcement wording that EU projects are handled "with zero data retention". The current docs side with approval-gating. — [wavect.io guide](https://wavect.io/blog/eu-data-residency-ai-apps-2026.md); [OpenAI announcement](https://openai.com/index/introducing-data-residency-in-europe/)

**ElevenLabs**

- Zero Retention Mode is optional and must be enabled; for EU it has "stricter requirements". Details are in the DPA. — [ElevenLabs data residency](https://elevenlabs.io/docs/overview/administration/data-residency.md)
- Training on personal data can be turned off in account settings under "Data use" ("Terms and Privacy"). The opt-out is reportedly prospective only. These come from third-party clause trackers, not verified against ElevenLabs' own text. — [ConductAtlas](https://conductatlas.com/platform/elevenlabs/elevenlabs-usage-policy/user-may-opt-out-of-content-used-for-training/); [ConductAtlas (prospective)](https://conductatlas.com/platform/elevenlabs/elevenlabs-privacy-policy/training-opt-out-prospective-only/)

**Mistral**

- Mistral help center (Mistral Studio): in Free mode "we may use your data (input and output) to train our artificial intelligence models", with withdrawal possible at any time. Pay-as-you-go customers "have the right to opt out at any time". — [Mistral help: do you use my data](https://help.mistral.ai/en/articles/323757-do-you-use-my-user-data-to-train-your-artificial-intelligence-models)
- An earlier help article (returned 404 on 2026-10-28, quoted via search snippet) said "Scale plans are opted-out by default" and Experiment plans opt out via an Admin Console toggle. — [Mistral help (404 at fetch time)](https://help.mistral.ai/en/articles/347614-can-i-opt-out-of-my-input-or-output-data-being-used-for-training)
- Sub-processor list on the Trust Center, with email notifications. — [Mistral sub-processors](https://help.mistral.ai/en/articles/455208-who-are-your-subprocessors-and-how-can-i-stay-updated-on-changes)
- 30-day abuse-monitoring retention, and ZDR only by contract, per a third-party gateway (unverified against Mistral). — [Opper](https://opper.ai/mistral/voxtral-mini-transcribe-realtime-2602)

### Inferences

- For a hobby or self-serve account, the safest defaults (no training without action) are Soniox, Speechmatics, Azure, Google (logging off), OpenAI (no training, but 30-day logs) and Gladia paid.
- Deepgram requires `mip_opt_out=true` on every request (and apparently costs more). AssemblyAI requires a paid plan plus a toggle or email. AWS requires an Organizations policy. Mistral and Gladia free tiers may train on audio.
- Real-time "no storage" claims (Azure, Speechmatics, Soniox) are the most relevant posture for live interview audio. OpenAI's 30-day abuse log on `/v1/realtime` is a notable exception unless ZDR is approved.

### Gaps

- Not verified directly: SOC 2 / ISO 27001 status of Deepgram, AssemblyAI, ElevenLabs, Mistral and OpenAI (trust centers not retrieved); certifications of AWS, Google and Azure (assumed standard but not checked here).
- Not found: Deepgram's "Your Data at Deepgram" page (default retention for non-opted-out requests); Mistral's primary API retention period; ElevenLabs' own STT-specific training and retention text; Azure's statement on training use of Speech data (not on the fetched page).
- Sub-processor lists exist for Gladia and Mistral (via their Trust Centers) and in part for AssemblyAI (CrowdStrike, Datadog). Full lists were not retrieved for any provider.

---

## Q6. Any CNIL / EU regulator actions involving these providers, and the EU-US transfer framework status?

### Takeaway

No CNIL action specific to speech recognition or voice data by any candidate STT provider was found. The notable EU regulator action is the Italian Garante's EUR 15M fine on OpenAI (Dec 2024, ChatGPT, not the transcription API), whose appeal status is contested. The EU-US Data Privacy Framework survived the General Court (3 Sep 2025), but an appeal (C-703/25 P) is pending at the CJEU.

### Cited Findings

- Italy's Garante fined OpenAI EUR 15 million in Dec 2024. The grounds were failing to notify the March 2023 breach and processing personal data to train ChatGPT without an appropriate legal basis; the fine came with a 6-month awareness campaign, and OpenAI announced an appeal. — [Lewis Silkin, 14 Jan 2025](https://lewissilkin.com/insights/2025/01/14/openai-faces-15-million-fine-as-the-italian-garante-strikes-again-102jtqc)
- Appeal status conflicts: an Italian report says the measure was suspended conditional on a bond, pending a ruling on the merits; another (undated) report says a Rome court overturned the fine. — [ItaliaOggi](https://www.italiaoggi.it/diritto-e-fisco/privacy/chatgpt-sospesa-la-sanzione-del-garante-privacy-da-15-milioni-di-euro-jojzy80l); [iTiger](https://www-web.itiger.com/news/1102273421)
- DPF: the General Court dismissed Latombe's challenge on 3 Sep 2025, and the appeal C-703/25 P was filed on 31 Oct 2025. A June 2026 report notes Microsoft intervening in support of the Commission, so the case was still pending then. — [WilmerHale, 1 Dec 2025](https://www.wilmerhale.com/en/insights/blogs/wilmerhale-privacy-and-cybersecurity-law/20251201-european-court-of-justice-to-review-challenge-to-eu-us-data-privacy-framework); [IEU Monitoring, 28 Jun 2026](https://ieu-monitoring.com/editorial/microsoft-backs-eu-commission-in-eu-court-case-on-transatlantic-data-flows-and-privacy/1244467/)

### Inferences

- Most candidates are US companies (or route metadata or support to the US). Where data leaves the EU, transfers rely on the DPF and/or SCCs, and a CJEU ruling against the DPF would bring back the post-Schrems II SCC and TIA burden. Choosing EU-resident endpoints, or an EU-headquartered provider, reduces this exposure but does not eliminate it, since metadata, support access and US parent-company access remain.

### Gaps

- No CJEU judgment, hearing date or Advocate General opinion in C-703/25 P was found as of 2026-10-28.
- No CNIL sanctions against Deepgram, AssemblyAI, Speechmatics, Gladia, Soniox, ElevenLabs or Mistral were found. CNIL actions against Google, Amazon or Microsoft in non-STT contexts were not researched.
- Final status of the OpenAI–Garante appeal is unconfirmed.

---

## Q7. Legal considerations in France for capturing and transmitting the interviewer's voice to a third-party STT (factual, not legal advice)

### Takeaway

Two regimes apply. The **French Penal Code** makes it an offence to "capture, record **or transmit**" words spoken in a private or confidential capacity without the speaker's consent (1 year / EUR 45,000). Consent is presumed only if this is done openly and the person could have objected. **GDPR** applies unless the candidate's use counts as "purely personal or household activity", an exemption construed narrowly (CJEU Ryneš); the STT provider is never covered by it. Practical reading: tell the interviewer and obtain agreement before streaming their voice.

### Cited Findings

**French criminal law**

- Art. 226-1 Code pénal: "Est puni d'un an d'emprisonnement et de 45 000 euros d'amende le fait, au moyen d'un procédé quelconque, volontairement de porter atteinte à l'intimité de la vie privée d'autrui en captant, enregistrant ou transmettant, sans le consentement de leur auteur, des paroles prononcées à titre privé ou confidentiel…" — [dossierjuridique.fr (26 Apr 2026, verified 2026-10-15)](https://dossierjuridique.fr/blog/enregistrement-clandestin-penal)
- Art. 226-2 punishes keeping, disclosing to the public or a third party, or using any recording obtained this way, and allows confiscation of equipment. — [dossierjuridique.fr](https://dossierjuridique.fr/blog/enregistrement-clandestin-penal)
- Presumed consent: a Ministry of Justice answer to a parliamentary question (13th legislature) states consent is presumed when the act is done "au vu et au su" of the person who could have objected but did not. — [Assemblée nationale QE 17430](https://www.assemblee-nationale.fr/dyn/13/questions/QANR5L13QE17430.pdf)
- Covert recordings may still be admitted as evidence in some proceedings, but the person who recorded can still be prosecuted under 226-1. — [MACSF](https://www.macsf.fr/responsabilite-professionnelle/cadre-juridique/un-enregistrement-clandestin-peut-il-etre-une-preuve-lors-d-un-proces)

**CNIL guidance on call recording (workplace)**

- The CNIL page on listening to and recording calls at work (published 5 May 2009, marked "obsolète et en cours de mise à jour") says:
  - external interlocutors must be informed of the system, its purposes, recipients and retention, and of their right to object "avant la fin de la conversation téléphonique";
  - the legal basis is a legal obligation or the employer's legitimate interest;
  - recordings may be kept up to 6 months and analysis documents up to 1 year;
  - permanent or systematic recording is prohibited unless a legal text provides for it.
    — [CNIL](https://www.cnil.fr/fr/lecoute-et-lenregistrement-des-appels-sur-le-lieu-de-travail)
- Legisocial reports a CNIL référentiel of April 2026 on call recording (12-month, or 6-month for evaluations, retention). Not verified on cnil.fr. — [Legisocial](https://www.legisocial.fr/actualites-sociales/7880-ecoute-enregistrements-telephoniques-duree-conservation-donnees-personnelles.html)

**GDPR scope and roles**

- The household exemption (Art. 2(2)(c), Recital 18) is "to be construed narrowly" (CJEU C-212/13 Ryneš, 11 Dec 2014). Recital 18 expressly states that controllers or processors providing the means for personal processing are **not** exempt. — [Thomas Helbing on Ryneš](https://www.thomashelbing.com/en/wissen/dsgvo-hub/rechtsprechung/1.4.20-eugh-rynes); [Hogan Lovells](https://www.hoganlovells.com/en/publications/cctv-cjeu-narrows-the-scope-of-the-household-exemption)
- EDPB Guidelines 02/2021 on virtual voice assistants:
  - voice used for authentication or identification is biometric data (special category, explicit consent);
  - information should be given "at the time of the processing at the latest", and the guidelines point to call-centre "this call is recorded" notices as a model;
  - "accidental users" whose voices are captured need consideration.
    — [EDPB Guidelines 02/2021 (consultation version)](https://www.edpb.europa.eu/system/files/2021-03/edpb_guidelines_022021_virtual_voice_assistants_adopted-public-consultation_en.pdf); [CMS summary](https://cms.law/en/bel/legal-updates/EDPB-Guidelines-on-Virtual-Voice-Assistants)
- Providers place responsibility for permissions on the customer:
  - Microsoft: the customer is "required to obtain all necessary permissions for processing of the data", and audio may also be regulated under communications laws. — [Microsoft Learn](https://learn.microsoft.com/en-us/azure/ai-foundry/responsible-ai/speech-service/speech-to-text/data-privacy-security)
  - Google: before enabling data logging the customer must certify "proper permissions from the data originator(s)". — [Google](https://cloud.google.com/speech-to-text/docs/enable-data-logging?hl=it)

### Inferences

- Streaming the interviewer's voice to a cloud STT is plausibly "transmitting" under Art. 226-1 even if nothing is stored. Whether a job interview counts as words spoken "à titre privé ou confidentiel" is a fact-specific question for a lawyer; recruitment interviews are often treated as confidential. Explicit notice and agreement at the start of the call (the "au vu et au su" route, or express consent) appears to be the defensible path.
- If the household exemption does not apply (the narrow reading makes this likely when a third party's voice is sent to a commercial service), the candidate would be a **controller**. They would need:
  - a legal basis: consent or legitimate interest;
  - Art. 13/14 information to the interviewer;
  - an Art. 28 DPA with the STT provider (available from Gladia, Mistral, ElevenLabs and others per Q5);
  - Chapter V transfer safeguards for any non-EU processing.
    The STT provider would be a **processor**, and would be a controller for any training use. Training use of a non-consenting interviewer's voice is a strong reason to disable training (Q5).
- Design-level mitigations consistent with the CNIL and EDPB guidance: an EU endpoint; a provider with no real-time storage and no training; no audio persistence on the client; short or no transcript retention; a visible "transcription active" notice; and optionally an option to transcribe only the candidate's microphone channel.

### Gaps

- No CNIL publication was found that specifically addresses an individual (non-employer) recording or transcribing a job interview, or AI "meeting copilots". The CNIL call-recording page is from 2009 and flagged obsolete. The April 2026 référentiel was not verified.
- Not found: case law on whether a recruitment interview is "à titre privé ou confidentiel" under 226-1, or on live transcription without storage.
- The final adopted version of EDPB Guidelines 02/2021 (vs the consultation version cited) was not retrieved.
- AI Act considerations (recruitment AI as high-risk applies to employers and deployers, not candidates) were out of scope and not researched.
