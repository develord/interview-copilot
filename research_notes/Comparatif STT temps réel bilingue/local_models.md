# Local / on-device streaming STT for a Rust Windows "interview copilot" (French main + English code-switching), state as of 2026-10-28

Scope: whisper.cpp / whisper-rs, faster-whisper streaming wrappers (whisper_streaming, SimulStreaming, WhisperLiveKit), Whisper large-v3 / large-v3-turbo, NVIDIA Parakeet TDT 0.6B v3 and Canary-1B-v2, Kyutai STT (DSM), Mistral Voxtral Realtime, Vosk, sherpa-onnx streaming models.
Use-case recap: 16 kHz mono PCM16, 20 ms frames, 2 sources (loopback + mic), final transcript of a finished question within ~0.5–1 s, consumer x64 laptop that may have no NVIDIA GPU.

Research note: about 25 tool calls. Several numbers come from third-party pages. Each one is flagged where it appears. Where nothing was found, the note says "not found".

---

## 1. True streaming vs chunked re-decoding: which architecture each option uses, and what that means for latency and partial stability

### Takeaway

Three candidates are natively streaming and have a fixed, small delay: Kyutai STT 1B en_fr (0.5 s), Voxtral Mini 4B Realtime (80 ms–2.4 s, 480 ms recommended) and Vosk / sherpa-onnx streaming zipformers. Whisper, Parakeet v3 and Canary v2 are offline encoders. They need either a sliding window that is re-decoded (Whisper streaming wrappers, whisper.cpp `stream`) or VAD segmentation followed by one decode per utterance. For this use case, the final transcript only matters when the question ends. "VAD endpoint, then one offline decode" is therefore a valid pattern, and its latency is endpoint silence plus decode time.

### Cited Findings

- **Kyutai STT 1B en_fr**: a decoder-only Transformer over Mimi audio tokens that uses Moshi's multistream design. The text stream is shifted relative to the audio stream. Frame rate is 12.5 Hz. Delay is **0.5 s** for `stt-1b-en_fr` and **2.5 s** for the English-only `stt-2.6b-en`. — [HF model card kyutai/stt-1b-en_fr](https://huggingface.co/kyutai/stt-1b-en_fr); [kyutai-labs/delayed-streams-modeling](https://github.com/kyutai-labs/delayed-streams-modeling)
- The DSM paper reports a delay-conditioned variant trained on delays of 0.25–4 s, accurate to roughly 300 ms around the target delay. It contrasts this with "streaming Whisper", whose delay has "a large variance". — [arXiv 2509.08753 (DSM)](https://arxiv.org/html/2509.08753)
- **Voxtral Mini 4B Realtime (2602)**: the card calls it a "natively streaming architecture". It has a causal audio encoder trained from scratch, and both the encoder and the LLM use sliding-window attention ("infinite" streaming). Delay is configurable from **80 ms to 2.4 s** in 80 ms steps, with **480 ms** recommended as the "sweet spot". Each text token covers 80 ms of audio. — [HF card mistralai/Voxtral-Mini-4B-Realtime-2602](https://huggingface.co/mistralai/Voxtral-Mini-4B-Realtime-2602)
- Paper and blog: Voxtral Realtime is built on the Delayed Streams Modeling framework (the same family as Kyutai's), with a causal encoder, a temporal adapter and an autoregressive decoder. At 480 ms delay it is "on par with Whisper". At about 2.4 s delay it matches the batch model Voxtral Mini Transcribe V2. Released February 2026. — [arXiv 2602.11298](https://arxiv.org/abs/2602.11298); [Mistral "Voxtral transcribe 2" announcement](https://mistral.ai/news/voxtral-transcribe-2)
- **Parakeet TDT 0.6B v3** is an offline model. "Streaming" here means a NeMo chunked-inference script with configurable chunk and context windows. Full attention handles about 24 minutes of audio on an A100 80GB, and local attention about 3 hours. — [HF card nvidia/parakeet-tdt-0.6b-v3](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3)
- **Canary-1B-v2**: the model card does not mention streaming. — [HF card nvidia/canary-1b-v2](https://huggingface.co/nvidia/canary-1b-v2)
- **whisper.cpp `whisper-stream`** samples the mic about every 0.5 s and transcribes continuously. The example uses `--step 500 --length 5000`, which re-decodes a sliding window. It requires SDL2. The README does not document how partials stabilise. — [ggml-org/whisper.cpp README](https://github.com/ggml-org/whisper.cpp)
- **whisper_streaming** (UFAL, 2023) uses the LocalAgreement policy and reports **3.3 s latency** on long-form unsegmented speech. **SimulStreaming** is its successor (AlignAtt policy) and claims to be "~5 times faster" than whisper_streaming. It targets 1–2 GPUs (Whisper large-v3 plus an optional EuroLLM 9B). — [ufal/whisper_streaming](https://github.com/ufal/whisper_streaming); [ufal/SimulStreaming](https://github.com/ufal/SimulStreaming)
- **WhisperLiveKit** wraps both policies (`simulstreaming`, the default, and `localagreement`). Its backend auto-selects MLX on macOS, then faster-whisper, then vanilla Whisper. — [WhisperLiveKit README (HF Space mirror)](https://huggingface.co/spaces/langtech-innovation/WhisperLiveKit/blob/2ec2266929f4b4315b2128545b2cfeb4aaed1b5c/README.md)
- One aggregator claims SimulStreaming reaches "200–800 ms latency on first word instead of 2–5 s". It cites no benchmark, so treat it as unverified. — [x-cmd WhisperLiveKit page](https://www.x-cmd.com/install/whisperlivekit)
- **sherpa-onnx** lists streaming model families (Zipformer transducer, Paraformer, CTC) and non-streaming ones (Whisper, NeMo Parakeet TDT, Moonshine, SenseVoice, Dolphin, and others). Its Rust examples include `streaming_zipformer_microphone.rs` and several `*_simulate_streaming_microphone.rs` files, which pair VAD with an offline model. — [k2-fsa/sherpa-onnx README](https://github.com/k2-fsa/sherpa-onnx); [docs.rs sherpa-onnx](https://docs.rs/sherpa-onnx)

### Inferences

- A copilot does not need stable word-by-word partials to produce an answer. It needs a correct **final** transcript soon after the question ends. Two pipelines fit:
  - (a) A natively streaming model (Kyutai 1B, Voxtral Realtime). The final text arrives about one model delay (0.5 s / 0.48 s) after the last word, plus the endpoint decision.
  - (b) VAD or endpointing followed by one offline decode (Parakeet v3, Whisper turbo). Final latency is the trailing-silence threshold (typically 300–600 ms) plus decode time for the whole utterance.
- Sliding-window Whisper streaming (whisper_streaming / LocalAgreement) is the weakest fit. The only measured figure is ~3.3 s, and it re-decodes the same audio many times, which doubles in cost with two audio sources.
- Partials can still come from a cheap streaming model, with a better offline model run at the endpoint. This is a common two-pass pattern; no source here benchmarks it for French.

### Gaps

- No independent benchmark of first-token or final-token latency on a consumer x64 CPU was found for any model.
- WhisperLive (Collabora): its README was not checked in this pass, so no figures.
- No measured latency figure for SimulStreaming from its maintainers was retrieved; only the "~5× faster" claim.

---

## 2. French quality and code-switching (FR sentences containing EN technical terms, plus English-only questions)

### Takeaway

On French benchmarks, the best open models in range are Parakeet v3 (FLEURS-fr 5.15%), the Voxtral Realtime family (FLEURS-fr 6.42% at 480 ms delay; offline Voxtral Mini Transcribe 2.0 at 4.32%) and Whisper large-v3-turbo (~6.6–6.8% FLEURS-fr, third-party figures). Kyutai 1B en_fr has **no published French WER**. Vosk French is much worse (11.6–14.7%). No source found documents how any of these models handles French/English code-switching. This is the biggest unknown and must be tested on the real target sentences.

### Cited Findings

- **Parakeet TDT 0.6B v3**: 25 European languages including French. French WER (greedy, no punctuation or capitalisation): **FLEURS 5.15%, MLS 4.97%, CoVoST 6.05%**. No Common Voice French WER is reported. Automatic language detection needs no prompting. Code-switching is not mentioned. Released 2025-08-14. — [HF card nvidia/parakeet-tdt-0.6b-v3](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3)
- **Canary-1B-v2**: 25 languages including French, ASR plus speech translation (to and from English). The card gives only aggregate WERs (FLEURS-25 8.40%, CoVoST-13 8.85%, MLS-6 7.27%); no French-specific number was found. The examples always pass `source_lang`. The hallucination metric is 134.7 characters per minute on MUSAN noise. Code-switching is not mentioned. Released 2025-08-14. — [HF card nvidia/canary-1b-v2](https://huggingface.co/nvidia/canary-1b-v2)
- **Voxtral Mini 4B Realtime**: 13 languages including French. French FLEURS WER by delay: **160 ms 9.75%, 240 ms 8.00%, 480 ms 6.42%, 960 ms 5.68%, 2400 ms 5.23%**. The offline Voxtral Mini Transcribe 2.0 reaches 4.32%. Code-switching is not mentioned. — [HF card Voxtral-Mini-4B-Realtime-2602](https://huggingface.co/mistralai/Voxtral-Mini-4B-Realtime-2602)
- **Kyutai stt-1b-en_fr**: English plus French, but the card has no WER table. Kyutai's announcement says they "only benchmarked the 2.6B model" (English-only). — [HF card](https://huggingface.co/kyutai/stt-1b-en_fr); [Kyutai blog, 2025-06-19](https://kyutai.org/2025/06/19/stt-open-source.html)
- The DSM paper has no French results. Its benchmark model, DSM-ASR 2.6B, is "trained for English only", although the pretraining audio was English and French. English short-form average WER is **6.4%**, against streaming Whisper large-v3 at 9.4% and streaming Parakeet-TDT-v2 at 7.0%. — [arXiv 2509.08753](https://arxiv.org/html/2509.08753)
- **Whisper large-v3-turbo (ONNX via sherpa-onnx, CPU)**: FLEURS-fr WER **6.84% INT8 / 6.57% FP16**, and FLEURS-en 5.32% / 5.27%. A language hint was supplied and temperature fallback was disabled. This is a third-party conversion. — [soniqo/Whisper-Large-v3-Turbo-ONNX](https://huggingface.co/soniqo/Whisper-Large-v3-Turbo-ONNX)
- A secondary French page puts Whisper large-v3 at roughly 5–7% WER on French FLEURS and turbo at about 7.83% average on the Open ASR Leaderboard (English). Not a primary source. — [VexaScribe (FR)](https://vexascribe.com/fr/transcription-whisper)
- **Vosk French**:
  - `vosk-model-fr-0.22` (1.4 GB): **14.72% CV, 11.64% MLS**, 13.10% mTEDx, 21.61% podcast, 13.22% VoxPopuli.
  - `vosk-model-small-fr-0.22` (41 MB): 23.95% CV, 19.30% mTEDx.
  - LINTO `vosk-model-fr-0.6-linto-2.2.0`: 16.19% CV.
  - English `vosk-model-en-us-0.22` (1.8 GB): 5.69% LibriSpeech test-clean.
    — [Vosk models page](https://alphacephei.com/vosk/models)
- **sherpa-onnx French streaming zipformer** (`sherpa-onnx-streaming-zipformer-fr-2023-04-14`): a 2023 model. A user reports "a lot of mistakes" in its transcriptions and says hotwords did not fix them; the issue asks for a better French streaming alternative and got no answer in the results seen. — [sherpa-onnx issue #3144](https://github.com/k2-fsa/sherpa-onnx/issues/3144)
- **Code-switching, general**:
  - A public issue about a voice-dictation feature (engine not named) describes a single-language decoder on French speech with English tech terms. English words are "replaced by a French homophone" or "silently dropped". — [claudeissues.com #83881](https://claudeissues.com/issue/83881-voice-dictation-single-language-decoder-cannot-handle-code-switching-english-tec)
  - A vendor blog (low reliability) says Whisper's accuracy degrades at language boundaries and that it is more prone to hallucination there. — [convertaudiototext.com](https://convertaudiototext.com/fr/blog/fix-multilingual-code-switching)

### Inferences

- **Whisper**: the decoder is conditioned on a single language token per 30 s window (general Whisper design, not re-verified in this pass). Forcing `fr` should keep most anglicisms common in French tech speech ("state management", "Vue", "hooks") as loanwords. Rarer English phrases risk being Frenchified, misspelled or dropped. Auto-detect on short or mixed chunks can flip to English or translate. Forcing `fr` for the interviewer and running auto-detect for English-only questions are conflicting requirements, so this needs a real test set.
- **Parakeet v3** has no language token and a single shared multilingual tokenizer (the card says no prompting is needed). That may handle intra-sentence switching more gracefully than Canary, which takes `source_lang`, or Whisper. This is unverified and must be tested.
- **Kyutai 1B en_fr** was trained jointly on English and French. That makes it architecturally attractive for FR/EN mixing, but without a published French WER its quality is unknown.
- **Vosk** large models have a static vocabulary, so English tech terms in a French model are likely out-of-vocabulary. It is unsuitable as the primary engine.
- Practical recommendation for the report: build a 50–100 sentence French/English interview test set (e.g. "Comment tu gères le state management dans Vue ?"). Measure WER and an "English term recall" metric across Parakeet v3, Whisper turbo (forced `fr` vs auto), Voxtral Realtime and Kyutai 1B.

### Gaps

- No French WER for Kyutai stt-1b-en_fr was found anywhere (card, GitHub, paper, blog).
- No French-specific WER for Canary-1B-v2 on its card.
- No published code-switching (FR/EN intra-sentence) evaluation for any of these models.
- No primary-source (OpenAI) French FLEURS WER for large-v3-turbo was retrieved in this pass.
- The Open ASR Leaderboard multilingual tab was not fetched.

---

## 3. Model sizes, RAM/VRAM, real-time factor on CPU and GPU, quantization

### Takeaway

On a consumer x64 laptop without an NVIDIA GPU, the realistic CPU options are Parakeet v3 int8 (0.6B) and Whisper turbo quantised (809M). The only CPU speed figure found for Whisper turbo is RTF ~0.19–0.24 on an Apple M5 Pro: an 8 s question would take ~1.5–2 s to decode, which misses the 0.5–1 s target. No consumer-CPU figure was found for Parakeet v3, Kyutai or Voxtral Realtime. Voxtral Realtime (4B) and Kyutai (1B, CUDA/Metal-oriented) effectively assume a GPU or Apple silicon.

### Cited Findings

- **whisper.cpp memory table** (disk / RAM):

  | Model  | Disk    | RAM     |
  | ------ | ------- | ------- |
  | tiny   | 75 MiB  | ~273 MB |
  | base   | 142 MiB | ~388 MB |
  | small  | 466 MiB | ~852 MB |
  | medium | 1.5 GiB | ~2.1 GB |
  | large  | 2.9 GiB | ~3.9 GB |

  There is no row for large-v3-turbo. The README covers integer quantisation (e.g. q5_0). — [whisper.cpp README](https://github.com/ggml-org/whisper.cpp)

- whisper.cpp backends: x86 AVX, CUDA, **Vulkan** (cross-vendor), OpenVINO (Intel CPU and GPU), ROCm/HIP, a Ryzen AI NPU via VitisAI (Windows 11), plus OpenBLAS and others. **DirectML is not mentioned.** Windows builds with MSVC and MinGW. — [whisper.cpp README](https://github.com/ggml-org/whisper.cpp)
- whisper.cpp version: Arch Linux packages `whisper-cpp-vulkan` **1.9.1** (build date 2026-06-19). The GitHub release list was not readable in this pass. — [Arch package](https://archlinux.org/packages/extra/x86_64/whisper-cpp-vulkan/)
- **large-v3-turbo GGUF sizes** (community repo): Q3_K 368 MB, Q4_K 474 MB, Q5_K 574 MB. — via search summary of a HF repo, [Orbination-Whisper-AI](https://huggingface.co/antoniosmich/Orbination-Whisper-AI) (third-party, not opened directly)
- **large-v3-turbo ONNX INT8 via sherpa-onnx, CPU ("M5 Pro")**: a second number per language that is presumably RTF but is unlabelled on the page. **FR 0.188 INT8 / 0.215 FP16**, EN 0.239 INT8 / 0.189 FP16. INT8 files: encoder 643 MB, decoder 344 MB. — [soniqo/Whisper-Large-v3-Turbo-ONNX](https://huggingface.co/soniqo/Whisper-Large-v3-Turbo-ONNX)
- **Full large-v3 on an M1 CPU** (OpenASR repackage, not whisper.cpp): RTF 1.17 fp16, 0.65 q8_0, 0.61 q4_k on an 11 s clip. fp16 is slower than real time. — [OpenASR/whisper-large-v3](https://huggingface.co/OpenASR/whisper-large-v3)
- whisper.cpp community benchmark on a 2010 i5-460M covers all quantisation types. q4_0 was the fastest on legacy hardware; turbo rows were not visible in the excerpt. — [whisper.cpp discussion #3752](https://github.com/ggml-org/whisper.cpp/discussions/3752)
- **Parakeet v3**: 0.6B params. Open ASR Leaderboard **RTFx 3332.74**, a GPU batch figure, not CPU. — [HF card](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3)
- **Parakeet v3 int8 ONNX for sherpa-onnx**: separate `encoder.int8.onnx`, `decoder.int8.onnx`, `joiner.int8.onnx` and `tokens.txt`, CC-BY-4.0. — [CoderViking/parakeet-tdt-0.6b-v3-onnx](https://huggingface.co/CoderViking/parakeet-tdt-0.6b-v3-onnx)
- A user reported an ONNX Runtime opset validation error when loading the v3 int8 files in sherpa-onnx; no resolution was shown. — [NVIDIA forum](https://forums.developer.nvidia.com/t/sherpa-onnx-error/353231)
- **Canary-1B-v2**: ~978M params, Open ASR **RTFx 749** (GPU). — [HF card](https://huggingface.co/nvidia/canary-1b-v2)
- **Kyutai stt-1b-en_fr**: ~1.0B params, BF16 safetensors. The card gives no inference hardware requirements. — [HF card](https://huggingface.co/kyutai/stt-1b-en_fr)
- Kyutai throughput: an H100 handles 400 real-time streams. The Rust server on an L40S serves 64 connections at 3× real time. The README mentions only CUDA builds (`--features cuda`) and MLX for Apple; no CPU or Windows guidance. — [delayed-streams-modeling](https://github.com/kyutai-labs/delayed-streams-modeling)
- DSM-ASR 2.6B on an H100: RTF 6.9 at batch size 1 (i.e. 6.9× faster than real time). — [arXiv 2509.08753](https://arxiv.org/html/2509.08753)
- **Voxtral Mini 4B Realtime**: ~4B params (≈3.4B LM + ≈970M encoder), BF16. vLLM needs "a single GPU with 16GB or more". The card says it is "optimized for on-device deployment" but gives no CPU figures. Open ASR **RTFx 93.32**; throughput above 12.5 tokens/s is needed for real time. — [HF card](https://huggingface.co/mistralai/Voxtral-Mini-4B-Realtime-2602)
- **voxtral.c** (antirez, pure C):
  - A secondary blog claims 2.5× faster than real time on Apple Silicon, and another write-up reports ~2.9 s encoder time for 60 s of audio on Apple Silicon. Both are unverified.
  - A note warns that on CPU-only setups a short encoder interval (`-I`) can make it slower, and that long sessions need mmap and a rolling KV cache.
  - The non-Apple path is a BLAS fallback.
    — [tilnote.io](https://tilnote.io/en/pages/698afc1a3803f8e6dddaedb6); [notes.billmill.org](https://notes.billmill.org/AI/speech-to-text/voxtral.c.html)
- **Vosk**: French big model 1.4 GB, small 41 MB; the LINTO French model is listed at 0.4× RT. Vosk is Kaldi-based and CPU-friendly. — [Vosk models](https://alphacephei.com/vosk/models)
- **SimulStreaming** is optimised for 1–2 GPUs (large-v3 1.5B + EuroLLM 9B). — [ufal/SimulStreaming](https://github.com/ufal/SimulStreaming)

### Inferences

- **Latency budget on CPU**: with VAD-then-decode and a typical 5–10 s question, decode time scales with the RTF.
  - At RTF 0.2 (Whisper turbo on M5 Pro, probably faster than a mid-range x64 laptop CPU), decode is 1–2 s. That misses 0.5–1 s, and the second audio source doubles the load.
  - Parakeet TDT 0.6B has a much lighter decoder, so CPU decoding is plausibly several times faster. Without a measured number, benchmark it on target hardware.
- **Memory**:
  - Whisper turbo Q5 needs ~0.6 GB on disk and roughly 1–1.5 GB of RAM in use (extrapolated, not measured).
  - Parakeet v3 int8 is roughly ~0.7 GB (rough estimate from fp32 0.6B / 4; not measured).
  - Voxtral 4B BF16 is ~8 GB of weights (4B × 2 bytes). Only 4-bit community ports bring it into laptop RAM, and CPU real-time is not demonstrated.
  - Kyutai 1B BF16 is ~2 GB of weights.
- **With an NVIDIA GPU (CUDA)**: all options become feasible.
  - Whisper turbo decode drops well under 0.5 s.
  - Kyutai 1B (moshi-server or `stt-rs`, CUDA) gives native 0.5 s streaming.
  - Voxtral Realtime needs ≥16 GB VRAM via vLLM, which is beyond most laptops. Community C or Rust ports may need less (unverified).
- **With non-NVIDIA GPUs on Windows**, whisper.cpp's Vulkan backend is the most mature route found. ONNX Runtime has a DirectML execution provider, but the sherpa-onnx Rust crate page does not expose a DirectML flag, so Parakeet-on-DirectML through sherpa-onnx is unconfirmed.

### Gaps

- No measured CPU RTF on a consumer x64 laptop (e.g. Intel Core Ultra or Ryzen 7) for Parakeet v3 int8, Whisper turbo q5/q8, Kyutai 1B or Voxtral Realtime. This is the single most important missing data point; recommend benchmarking.
- No RAM or VRAM figure for large-v3-turbo in whisper.cpp's table.
- No verified CPU-only real-time result for voxtral.c or the Rust Voxtral port.
- Kyutai on CPU (candle CPU backend) is undocumented.

---

## 4. Rust integration maturity, Windows build friction, licensing

### Takeaway

The two most mature Rust paths are **sherpa-onnx** (crate 2.0.1, 2026-10-27; static linking with prebuilt Windows x64 libraries downloaded automatically; Apache-2.0) and **whisper-rs** (0.16.0, 2026-03-12; repo moved to Codeberg; Unlicense; wraps MIT whisper.cpp). Kyutai ships native Rust (candle, moshi-server, `stt-rs`), but the documented builds use CUDA or Metal. Licences are commercial-friendly overall. Parakeet, Canary and Kyutai weights are **CC-BY-4.0** (attribution required). Voxtral Realtime is **Apache 2.0**. Watch for Vosk's French pguyot model (**CC-BY-NC-SA, non-commercial**) and the LINTO French model (**AGPL**).

### Cited Findings

- **sherpa-onnx Rust crate**:
  - Version 2.0.1, dated 2026-10-27, Apache-2.0; repository k2-fsa/sherpa-onnx, owner csukuangfj (the sherpa-onnx maintainer).
  - **Links statically by default.** If `SHERPA_ONNX_LIB_DIR` is unset, it downloads a prebuilt archive from GitHub releases. Prebuilt archives cover **Windows x64 and arm64**, Linux and macOS.
  - The `shared` feature copies DLLs next to binaries on Windows. No CUDA or DirectML feature flag is documented on the page.
  - APIs: `OnlineRecognizer` / `OnlineStream` (streaming) and `OfflineRecognizer` (offline).
  - Examples: `streaming_zipformer_microphone.rs`, `nemo_parakeet.rs`, `whisper.rs`, `sense_voice.rs`, `silero_vad_remove_silence.rs`, `ten_vad_remove_silence.rs`, `keyword_spotter.rs`, online and offline punctuation.
    — [docs.rs sherpa-onnx](https://docs.rs/sherpa-onnx)
- sherpa-onnx lists 12 language APIs including Rust and a C API, supports Windows x64, and is Apache-2.0. NeMo Canary was **not** listed in the README excerpt retrieved. — [k2-fsa/sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx)
- **whisper-rs**: newest 0.16.0 (2026-03-12); previous releases 0.15.1 (2025-09-10) and 0.14.4 (2025-07-30). Repository is now **codeberg.org/tazz4843/whisper-rs**, licence **Unlicense**, ~1.8 M downloads. — [crates.io API whisper-rs](https://crates.io/api/v1/crates/whisper-rs)
- whisper.cpp is **MIT** and lists whisper-rs as its Rust binding. Windows builds with MSVC and MinGW. — [whisper.cpp README](https://github.com/ggml-org/whisper.cpp)
- **Kyutai** code and weights:
  - Python code is MIT and the Rust backend Apache-2.0. `moshi-server` installs with `cargo install --features cuda moshi-server`, and the standalone `stt-rs` runs with `cargo run --features cuda`. MLX covers Apple; the 1B model was tested on an iPhone 16 Pro via moshi-swift.
  - STT weights are **CC-BY 4.0**.
  - Windows is not mentioned.
    — [delayed-streams-modeling](https://github.com/kyutai-labs/delayed-streams-modeling); [HF card](https://huggingface.co/kyutai/stt-1b-en_fr)
- Kyutai STT is also in Hugging Face Transformers (`kyutai/stt-1b-en_fr-trfs`, transformers ≥ 4.53). — [HF card](https://huggingface.co/kyutai/stt-1b-en_fr); [Transformers docs](https://huggingface.co/docs/transformers/v5.0.0rc0/en/model_doc/kyutai_speech_to_text)
- **Parakeet v3 and Canary-1B-v2**: CC-BY-4.0, "ready for commercial/non-commercial use". — [Parakeet card](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3); [Canary card](https://huggingface.co/nvidia/canary-1b-v2)
- **Voxtral Mini 4B Realtime**: Apache 2.0.
  - Runtimes: vLLM (recommended, `/v1/realtime`), Transformers ≥ 5.2.0, and ExecuTorch (marked untested).
  - Community ports: voxtral.c (pure C), mlx-audio, voxmlx, and a **Rust** port.
  - llama.cpp is not supported yet.
    — [HF card](https://huggingface.co/mistralai/Voxtral-Mini-4B-Realtime-2602)
- **Vosk** licences: most models Apache 2.0. Exceptions: `vosk-model-small-fr-pguyot-0.3` is **CC-BY-NC-SA 4.0** (non-commercial) and `vosk-model-fr-0.6-linto-2.2.0` is **AGPL**. — [Vosk models](https://alphacephei.com/vosk/models)
- Known friction: loading Parakeet v3 int8 in sherpa-onnx failed with an ONNX Runtime opset validation error for one user. — [NVIDIA forum](https://forums.developer.nvidia.com/t/sherpa-onnx-error/353231)

### Inferences

- **Lowest Windows friction**: `sherpa-onnx` crate (static, prebuilt). One dependency gives Parakeet v3, Whisper, Silero VAD and streaming zipformers, without building C++ or ONNX Runtime by hand.
- **whisper-rs** needs a C/C++ toolchain (CMake + MSVC) to build whisper.cpp, plus extra flags for CUDA or Vulkan. That is moderate friction but well trodden.
- **Kyutai on Windows** without CUDA is the riskiest path. It is undocumented; candle has a CPU backend, but performance and testing are unknown.
- **Voxtral Realtime from Rust**: the community port's maturity is unknown. Running vLLM locally on Windows is impractical for a desktop app.
- The `ort` crate (ONNX Runtime for Rust) is an alternative for running Parakeet or Canary ONNX directly with DirectML. Its version and maturity were not checked in this pass.
- CC-BY-4.0 means attribution must appear in the app (e.g. an About or licences screen).

### Gaps

- `ort` crate version, DirectML or CUDA EP maturity on Windows: not checked.
- Maturity and URL of the Voxtral Rust port: not found (only "Rust" on the HF card).
- Whether the sherpa-onnx Rust crate supports a GPU provider (CUDA or DirectML) on Windows: not documented on docs.rs.
- Silero VAD model licence: not verified in this pass (believed MIT; unconfirmed).

---

## 5. Prompting, hotwords and context biasing for technical vocabulary

### Takeaway

Biasing support is weak or experimental across local models. Kyutai has experimental text or audio prompting that is "very sensitive to the prompt". Whisper has an initial-prompt mechanism (not re-verified in the sources fetched here). sherpa-onnx hotwords exist for transducers but reportedly did not fix the French zipformer. Vosk small models allow runtime vocabulary changes, while the big models are static. No biasing is documented for Parakeet v3, Canary v2 or Voxtral Realtime. Plan on LLM post-correction of tech terms instead.

### Cited Findings

- **Kyutai**: supports text, audio or text-audio prompts (the example changes a name's spelling). The README calls this experimental and "very sensitive to the prompt". — [delayed-streams-modeling](https://github.com/kyutai-labs/delayed-streams-modeling)
- **sherpa-onnx**: the README excerpt did not document hotwords. A downstream app says it boosts script words as hotwords with modified beam search (secondhand). One user said hotwords did not fix the errors of the French streaming zipformer. — [sherpa-onnx README](https://github.com/k2-fsa/sherpa-onnx); [issue #3144](https://github.com/k2-fsa/sherpa-onnx/issues/3144)
- **sherpa-onnx Rust** includes a `keyword_spotter.rs` example (keyword spotting, not ASR biasing). — [docs.rs sherpa-onnx](https://docs.rs/sherpa-onnx)
- **Vosk**: most small models allow dynamic vocabulary reconfiguration; big models are static, except "lgraph" variants such as `vosk-model-en-us-0.22-lgraph`. — [Vosk models](https://alphacephei.com/vosk/models)
- **whisper.cpp**: the README does not mention initial-prompt support. Not found in the fetched text. — [whisper.cpp README](https://github.com/ggml-org/whisper.cpp)
- **Voxtral Realtime**: context biasing is not mentioned on the HF card. — [HF card](https://huggingface.co/mistralai/Voxtral-Mini-4B-Realtime-2602)
- **Parakeet v3 and Canary v2**: no biasing or prompting is mentioned on the cards. — [Parakeet](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3); [Canary](https://huggingface.co/nvidia/canary-1b-v2)

### Inferences

- **Whisper's `initial_prompt`** (`FullParams::set_initial_prompt` in whisper-rs, `--prompt` in whisper.cpp) is widely used to prime vocabulary such as "Vue, React, state management, Kubernetes". This is from general knowledge and was not verified in this pass. It also nudges Whisper to keep English terms in French output, which is useful for code-switching, but it can increase hallucination on silence.
- For Parakeet (TDT), NeMo has GPU-oriented word-boosting and context-biasing tooling. Nothing indicates it is exposed in sherpa-onnx for Parakeet. Unverified.
- **The robust approach for a copilot**: pass the raw transcript to the answering LLM with a glossary (job description, stack keywords) and let it repair misrecognised terms. This adds no extra STT latency.

### Gaps

- whisper.cpp / whisper-rs prompt API was not confirmed from a fetched source in this pass.
- sherpa-onnx hotwords documentation (which model types, which decoding method) was not fetched.
- No documented biasing for Voxtral Realtime or Parakeet v3 in local runtimes.

---

## 6. Built-in VAD and endpointing

### Takeaway

VAD is well covered locally:

- **Silero VAD** (and **TEN-VAD**) are available in the sherpa-onnx Rust crate.
- **whisper.cpp** has integrated Silero VAD (`--vad`, Silero v6.2.0, with tunable thresholds and durations).
- **sherpa-onnx streaming recognizers** have `enable_endpoint` rules.
- **Kyutai 1B** has a built-in **semantic VAD** that predicts end of turn, which is ideal for the "question finished" trigger.
- **Voxtral Realtime** documents no VAD.

### Cited Findings

- **whisper.cpp VAD**: Silero VAD via `--vad` with a separate model (download script for `silero-v6.2.0`). Tunables: threshold, minimum speech and silence durations, maximum speech duration, speech padding, sample overlap. — [whisper.cpp README](https://github.com/ggml-org/whisper.cpp)
- **sherpa-onnx Rust**: `silero_vad_remove_silence.rs` and `ten_vad_remove_silence.rs` examples. The streaming zipformer example sets `enable_endpoint = true`. No dedicated endpoint example exists. — [docs.rs sherpa-onnx](https://docs.rs/sherpa-onnx)
- sherpa-onnx explicitly supports Silero VAD; TEN-VAD was not in the README excerpt but appears in the Rust crate examples. — [sherpa-onnx README](https://github.com/k2-fsa/sherpa-onnx); [docs.rs](https://docs.rs/sherpa-onnx)
- **Kyutai stt-1b-en_fr** includes a semantic VAD (1B only). `stt-rs --vad` prints the VAD output, which the README highlights for voice agents. Word-level timestamps are available. — [delayed-streams-modeling](https://github.com/kyutai-labs/delayed-streams-modeling); [HF card](https://huggingface.co/kyutai/stt-1b-en_fr)
- **Voxtral Realtime**: no VAD mentioned. In Mistral's API, realtime mode is not compatible with diarisation. — [HF card](https://huggingface.co/mistralai/Voxtral-Mini-4B-Realtime-2602); [Mistral docs audio overview](https://docs.mistral.ai/studio/audio/overview)

### Inferences

- **For the copilot**, run Silero or TEN VAD on each source (mic and loopback separately, which also gives free speaker attribution: "me" vs "interviewer"). End a question after ~400–600 ms of trailing silence, then decode the segment with the offline model. A semantic end-of-turn model (Kyutai's, or a small LLM check) can avoid cutting mid-question at a hesitation pause.
- **20 ms frames**: Silero VAD typically works on 32 ms windows (512 samples at 16 kHz), so some re-buffering is needed. Not verified in this pass.

### Gaps

- Exact sherpa-onnx endpoint rule parameters (rule1/2/3 trailing-silence defaults) were not fetched.
- No quantitative evaluation of Kyutai's semantic VAD (precision and latency) was found.

---

## 7. Privacy vs accuracy/latency, hybrid feasibility, and overall verdict for this use case

### Takeaway

A fully local pipeline can realistically meet the 0.5–1 s target **if the laptop has a decent GPU or NPU path**, or **on CPU only with Parakeet v3 int8 plus VAD**, subject to benchmarking. Whisper turbo on CPU likely misses the target (RTF ~0.2 means 1–2 s for a typical question). On the found numbers, the best local French accuracy is Parakeet v3 (FLEURS-fr 5.15%). The best true-streaming open option with published French numbers is Voxtral Realtime (6.42% at 480 ms), but its 4B size makes CPU-only laptops a poor fit. Kyutai 1B en_fr is the most architecturally aligned option (EN+FR, 0.5 s delay, semantic VAD, native Rust), but its French WER is unpublished and CPU/Windows support is undocumented. A hybrid design is feasible: local VAD and endpointing always, cloud STT when online, and local Parakeet or Whisper as offline fallback or privacy mode.

### Cited Findings

- **Parakeet v3**: FR FLEURS 5.15% / MLS 4.97%; 0.6B; CC-BY-4.0; auto language detection. — [HF card](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3)
- **Voxtral Realtime**: FR FLEURS 6.42% at 480 ms; 4B; ≥16 GB GPU for vLLM; Apache 2.0. The same model is also offered via API at $0.006/min, which allows the same model local and cloud. — [HF card](https://huggingface.co/mistralai/Voxtral-Mini-4B-Realtime-2602); [Mistral announcement](https://mistral.ai/news/voxtral-transcribe-2)
- **Kyutai 1B en_fr**: 0.5 s delay, semantic VAD, CC-BY-4.0; Rust server and standalone builds documented with CUDA. — [delayed-streams-modeling](https://github.com/kyutai-labs/delayed-streams-modeling)
- **Whisper turbo (sherpa-onnx CPU, M5 Pro)**: FR WER ~6.6–6.8%, presumed RTF ~0.19–0.22. — [soniqo ONNX](https://huggingface.co/soniqo/Whisper-Large-v3-Turbo-ONNX)
- **Vosk FR big**: 11.6–14.7% WER. — [Vosk](https://alphacephei.com/vosk/models)

### Inferences

- **Privacy benefit**: with a local pipeline, interview audio from both the interviewer's voice (loopback) and the candidate's never leaves the machine. Interviewer consent and recording laws matter, and avoiding cloud upload reduces exposure. Costs:
  - lower or unknown code-switching quality versus top cloud models;
  - CPU contention with the answering pipeline, especially if the LLM is also local;
  - a larger installer (models of 0.5–2 GB);
  - attribution duties (CC-BY).
- **Shortlist ranked for this exact use case** (inference, to validate by benchmark):
  1. **sherpa-onnx (Rust crate) + Silero or TEN VAD + Parakeet TDT 0.6B v3 int8**. Best French WER found, light, static Windows build, Apache/CC-BY. Risks: no hotwords, code-switching untested, CPU RTF unmeasured, a reported int8 load error.
  2. **whisper-rs + whisper.cpp large-v3-turbo (q5_0/q8_0) + built-in Silero VAD**. Initial prompt for tech terms. On CPU likely 1–2 s per question; with an NVIDIA (CUDA) or Vulkan GPU it should fit easily. Risks: hallucinations, language-token handling for mixed FR/EN.
  3. **Kyutai stt-1b-en_fr** (Rust/candle, moshi-server). Best architectural fit (true streaming at 0.5 s, semantic VAD, EN+FR) if an NVIDIA GPU is available. Must measure French WER; Windows and CPU are unknown.
  4. **Voxtral Mini 4B Realtime**. Strong published French streaming WER, Apache 2.0. Practical locally only on a GPU with plenty of VRAM or via immature quantised community ports. Otherwise better used through Mistral's API.
  5. **Vosk / sherpa-onnx French streaming zipformer**: only as an ultra-light partials or wake layer. French accuracy is too low for the final transcript.
- **Hybrid design**:
  - Local VAD and endpointing always run, which saves cloud cost and gives sharp end-of-question timing.
  - Ship VAD-gated segments to a cloud streaming STT when online.
  - Fall back to local Parakeet v3 (CPU) or Whisper turbo (GPU) when offline or in "privacy mode".
  - Voxtral Realtime in particular exists as both open weights and an API, so local and cloud behaviour can be aligned.
- **Two sources** (mic and loopback): run VAD on both but transcribe the interviewer stream (loopback) with priority. The candidate's own speech can use a cheaper model, or be transcribed only on demand, to halve CPU load.

### Gaps

- No end-to-end measurement (endpoint plus decode) on a consumer Windows x64 laptop for any candidate. Benchmarking Parakeet v3 int8 and Whisper turbo q5 on the target hardware with real FR/EN interview audio is the decisive next step.
- No published FR/EN code-switching evaluation for any local model.
- NPU options on Windows (Intel or AMD Ryzen AI via OpenVINO or VitisAI in whisper.cpp; Qualcomm) were seen only as backend mentions; no latency numbers were found.
