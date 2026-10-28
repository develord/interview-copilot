//! STT benchmark harness (Phase 3). Replays a WAV corpus in real time through the *same*
//! transcription session the app uses (pacing, VAD, endpointing, reconnection), then scores each
//! provider against the thresholds proposed in `reports/Comparatif STT temps réel bilingue.md`.
//!
//! ```text
//! cargo run --features bench --bin stt-bench -- --provider assemblyai --manifest corpus/manifest.json
//! ```
//!
//! The API key comes from `ASSEMBLYAI_API_KEY` / `SONIOX_API_KEY`, or from the key saved in the
//! app (Windows Credential Manager). Audio is streamed to the provider; nothing is stored.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use interview_copilot_lib::audio::convert::{FRAME_SAMPLES, MonoResampler, to_pcm16};
use interview_copilot_lib::audio::engine::Emit;
use interview_copilot_lib::audio::types::SourceKind;
use interview_copilot_lib::audio::worker::{FrameSink, RecoveryPolicy};
use interview_copilot_lib::events::PipelineEvent;
use interview_copilot_lib::stt::provider::SttProvider;
use interview_copilot_lib::stt::types::{ProviderKind, SttState, TranscriptionConfig};
use interview_copilot_lib::stt::vad::EnergyVad;
use interview_copilot_lib::stt::{self, credentials, eval, session};
use serde::{Deserialize, Serialize};

/// How long to keep the stream open after the last clip (the session pads silence).
const TAIL: Duration = Duration::from_millis(3_500);

#[derive(Deserialize)]
struct Manifest {
    utterances: Vec<Utterance>,
}

#[derive(Deserialize)]
struct Utterance {
    id: String,
    /// Relative to the manifest's directory.
    file: PathBuf,
    /// `mixed`, `fr` or `en` (any label works; thresholds use these three).
    category: String,
    reference: String,
    #[serde(default)]
    terms: Vec<String>,
}

#[derive(Serialize)]
struct Outcome {
    id: String,
    category: String,
    reference: String,
    hypothesis: String,
    errors: usize,
    words: usize,
    terms_found: Vec<String>,
    terms_missed: Vec<String>,
    /// Local end of speech → last final transcript.
    latency_ms: Option<u32>,
    stt_error: Option<String>,
}

struct Args {
    provider: ProviderKind,
    manifest: PathBuf,
    out: Option<PathBuf>,
    builtin_vocabulary: bool,
    /// Local silence (ms) before forcing the end of a turn; `None` = provider only.
    fast_endpoint_ms: Option<u32>,
    endpoint: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = std::env::args().skip(1);
    let (mut provider, mut manifest, mut out, mut endpoint) = (None, None, None, None);
    let mut builtin_vocabulary = true;
    let mut fast_endpoint_ms = Some(700);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--provider" => {
                provider = Some(match args.next().as_deref() {
                    Some("assemblyai") => ProviderKind::Assemblyai,
                    Some("soniox") => ProviderKind::Soniox,
                    other => return Err(format!("unknown provider {other:?}")),
                })
            }
            "--manifest" => manifest = args.next().map(PathBuf::from),
            "--out" => out = args.next().map(PathBuf::from),
            "--endpoint" => endpoint = args.next(),
            "--no-builtin-vocabulary" => builtin_vocabulary = false,
            "--no-fast-endpoint" => fast_endpoint_ms = None,
            "--fast-endpoint-ms" => {
                fast_endpoint_ms = Some(
                    args.next()
                        .and_then(|v| v.parse().ok())
                        .ok_or("--fast-endpoint-ms needs a number of milliseconds")?,
                )
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(Args {
        provider: provider.ok_or("--provider assemblyai|soniox is required")?,
        manifest: manifest.ok_or("--manifest <path> is required")?,
        out,
        builtin_vocabulary,
        fast_endpoint_ms,
        endpoint,
    })
}

fn api_key(provider: ProviderKind) -> Result<String, String> {
    let var = format!("{}_API_KEY", provider.id().to_uppercase());
    if let Ok(key) = std::env::var(&var) {
        return Ok(key);
    }
    credentials::get(provider)
        .map_err(|e| e.to_string())?
        .ok_or(format!("no key: set {var} or save one in the app"))
}

/// Any WAV (rate, channels, 16/24/32-bit int or float) → 16 kHz mono PCM16.
fn read_wav(path: &Path) -> Result<Vec<i16>, String> {
    let mut reader =
        hound::WavReader::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let spec = reader.spec();
    let mut samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>(),
        hound::SampleFormat::Int => {
            let scale = 2f32.powi(i32::from(spec.bits_per_sample) - 1);
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / scale))
                .collect()
        }
    }
    .map_err(|e| format!("{}: {e}", path.display()))?;
    // Flush the resampler's tail with 0.2 s of silence.
    samples.extend(std::iter::repeat_n(
        0.0,
        spec.sample_rate as usize / 5 * spec.channels as usize,
    ));
    let mut resampler = MonoResampler::new(spec.sample_rate, spec.channels)?;
    let mut mono = Vec::new();
    resampler.process(&samples, &mut mono);
    Ok(mono.into_iter().map(to_pcm16).collect())
}

/// Offset of the last voiced frame, as the app's VAD sees it.
fn speech_end(samples: &[i16]) -> Duration {
    let mut vad = EnergyVad::new(700);
    let mut last_voice = 0;
    for (i, frame) in samples.chunks(FRAME_SAMPLES).enumerate() {
        vad.push(frame);
        if vad.last_frame_was_speech() {
            last_voice = i + 1;
        }
    }
    Duration::from_millis(last_voice as u64 * 20)
}

/// Silence between utterances: lets each turn close and keeps late finals attributable.
const GAP: Duration = Duration::from_secs(3);

struct Played {
    start_at: Instant,
    speech_end_at: Instant,
}

struct ClipResult {
    hypothesis: String,
    latency_ms: Option<u32>,
    error: Option<String>,
}

/// Streams every clip through ONE session, like the app does during an interview (one
/// connection per source). Opening a connection per clip trips provider rate limits — AssemblyAI
/// free accounts allow 5 new streaming connections per minute. Finals are attributed to the clip
/// being played when they arrive, which also survives reconnections (provider timestamps restart).
async fn transcribe_corpus(
    provider: Arc<dyn SttProvider>,
    clips: &[Vec<i16>],
    force_endpoint_ms: Option<u32>,
) -> Vec<ClipResult> {
    let events: Arc<Mutex<Vec<(Instant, PipelineEvent)>>> = Arc::default();
    let recorder = events.clone();
    let emit: Emit = Arc::new(move |event| recorder.lock().unwrap().push((Instant::now(), event)));
    let mut sink = session::spawn(
        &tokio::runtime::Handle::current(),
        provider,
        session::SessionOptions {
            source: SourceKind::System,
            session_id: "bench".into(),
            force_endpoint_ms,
            policy: RecoveryPolicy::DEFAULT,
        },
        emit,
    );

    let gap = vec![0i16; (GAP.as_millis() as usize / 20) * FRAME_SAMPLES];
    let started = Instant::now();
    let mut frames_sent: u64 = 0;
    let mut played = Vec::with_capacity(clips.len());
    for (n, clip) in clips.iter().enumerate() {
        let start_at = started + Duration::from_millis(frames_sent * 20);
        played.push(Played {
            start_at,
            speech_end_at: start_at + speech_end(clip),
        });
        println!("  playing clip {}/{}", n + 1, clips.len());
        for frame in clip.chunks(FRAME_SAMPLES).chain(gap.chunks(FRAME_SAMPLES)) {
            sink.push(SourceKind::System, frame, frames_sent);
            frames_sent += 1;
            tokio::time::sleep_until((started + Duration::from_millis(20 * frames_sent)).into())
                .await;
        }
    }
    tokio::time::sleep(TAIL).await;
    drop(sink);
    let closing = Instant::now();
    while closing.elapsed() < Duration::from_secs(10) {
        let closed = events.lock().unwrap().iter().any(|(_, e)| {
            matches!(e, PipelineEvent::SttStatus(s) if matches!(s.state, SttState::Closed | SttState::Error))
        });
        if closed {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let clip_at = |at: Instant| {
        played
            .iter()
            .rposition(|p| p.start_at + Duration::from_millis(300) <= at)
    };
    let mut results: Vec<ClipResult> = (0..clips.len())
        .map(|_| ClipResult {
            hypothesis: String::new(),
            latency_ms: None,
            error: None,
        })
        .collect();
    for (at, event) in events.lock().unwrap().iter() {
        let Some(i) = clip_at(*at) else { continue };
        match event {
            PipelineEvent::TranscriptFinal { segment } => {
                let result = &mut results[i];
                if !result.hypothesis.is_empty() {
                    result.hypothesis.push(' ');
                }
                result.hypothesis.push_str(&segment.text);
                result.latency_ms = Some(
                    at.saturating_duration_since(played[i].speech_end_at)
                        .as_millis() as u32,
                );
            }
            PipelineEvent::SttStatus(status) => {
                if let Some(error) = &status.error {
                    results[i].error = Some(error.to_string());
                }
            }
            _ => {}
        }
    }
    results
}

struct Threshold {
    category: &'static str,
    max_wer: f64,
}

const THRESHOLDS: [Threshold; 3] = [
    Threshold {
        category: "mixed",
        max_wer: 0.10,
    },
    Threshold {
        category: "fr",
        max_wer: 0.08,
    },
    Threshold {
        category: "en",
        max_wer: 0.08,
    },
];
const MIN_TERM_RECALL: f64 = 0.90;
const MAX_P50_MS: u32 = 600;
const MAX_P95_MS: u32 = 1_000;

fn verdict(ok: bool) -> &'static str {
    if ok { "PASS" } else { "FAIL" }
}

fn summarize(outcomes: &[Outcome]) -> serde_json::Value {
    let mut by_category: BTreeMap<&str, Vec<&Outcome>> = BTreeMap::new();
    for outcome in outcomes {
        by_category
            .entry(outcome.category.as_str())
            .or_default()
            .push(outcome);
    }

    println!(
        "\n{:<8} {:>5} {:>7} {:>10} {:>9} {:>9}",
        "category", "n", "WER", "terms", "p50 ms", "p95 ms"
    );
    let mut summary = serde_json::Map::new();
    for (category, items) in &by_category {
        let (errors, words) = items
            .iter()
            .fold((0, 0), |(e, w), o| (e + o.errors, w + o.words));
        let wer = errors as f64 / words.max(1) as f64;
        let found: usize = items.iter().map(|o| o.terms_found.len()).sum();
        let total: usize = found + items.iter().map(|o| o.terms_missed.len()).sum::<usize>();
        let latencies: Vec<u32> = items.iter().filter_map(|o| o.latency_ms).collect();
        let (p50, p95) = (
            eval::percentile(&latencies, 50.0),
            eval::percentile(&latencies, 95.0),
        );
        println!(
            "{:<8} {:>5} {:>6.1}% {:>9} {:>9} {:>9}",
            category,
            items.len(),
            wer * 100.0,
            if total > 0 {
                format!("{:.0}%", found as f64 / total as f64 * 100.0)
            } else {
                "-".into()
            },
            p50.map_or("-".into(), |v| v.to_string()),
            p95.map_or("-".into(), |v| v.to_string()),
        );
        summary.insert(
            category.to_string(),
            serde_json::json!({ "n": items.len(), "wer": wer, "termsFound": found, "termsTotal": total, "p50Ms": p50, "p95Ms": p95 }),
        );
    }

    println!("\nThresholds (proposed in the STT comparison report):");
    for t in THRESHOLDS {
        if let Some(s) = summary.get(t.category) {
            let wer = s["wer"].as_f64().unwrap_or(1.0);
            println!(
                "  WER {:<6} ≤ {:>4.0}%  {:>6.1}%  {}",
                t.category,
                t.max_wer * 100.0,
                wer * 100.0,
                verdict(wer <= t.max_wer)
            );
        }
    }
    let found: usize = outcomes.iter().map(|o| o.terms_found.len()).sum();
    let total: usize = found + outcomes.iter().map(|o| o.terms_missed.len()).sum::<usize>();
    if total > 0 {
        let recall = found as f64 / total as f64;
        println!(
            "  term recall ≥ 90%  {:>6.1}%  {}",
            recall * 100.0,
            verdict(recall >= MIN_TERM_RECALL)
        );
    }
    let latencies: Vec<u32> = outcomes.iter().filter_map(|o| o.latency_ms).collect();
    if let (Some(p50), Some(p95)) = (
        eval::percentile(&latencies, 50.0),
        eval::percentile(&latencies, 95.0),
    ) {
        println!(
            "  time-to-final p50 ≤ {MAX_P50_MS} ms  {p50:>5} ms  {}",
            verdict(p50 <= MAX_P50_MS)
        );
        println!(
            "  time-to-final p95 ≤ {MAX_P95_MS} ms  {p95:>5} ms  {}",
            verdict(p95 <= MAX_P95_MS)
        );
    }
    serde_json::Value::Object(summary)
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!(
                "{message}\n\nusage: stt-bench --provider assemblyai|soniox --manifest <manifest.json> [--out <report.json>] [--no-builtin-vocabulary] [--fast-endpoint-ms <ms> | --no-fast-endpoint] [--endpoint <wss-url>]"
            );
            std::process::exit(2);
        }
    };
    let manifest_text = std::fs::read_to_string(&args.manifest)
        .unwrap_or_else(|e| panic!("{}: {e}", args.manifest.display()));
    let manifest: Manifest = serde_json::from_str(&manifest_text).expect("invalid manifest");
    let root = args.manifest.parent().unwrap_or(Path::new("."));
    let key = api_key(args.provider).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2);
    });

    // Corpus terms are scored (recall), not used as bias: this measures the provider as configured
    // in the app (built-in vocabulary on or off), not one tuned to the answer key.
    let config = TranscriptionConfig {
        provider: args.provider,
        extra_terms: Vec::new(),
        force_endpoint_ms: args.fast_endpoint_ms,
        builtin_vocabulary: args.builtin_vocabulary,
    };
    let provider = stt::build_provider(&config, key, args.endpoint.clone());

    let mut clips = Vec::new();
    let mut utterances = Vec::new();
    for utterance in &manifest.utterances {
        match read_wav(&root.join(&utterance.file)) {
            Ok(samples) => {
                clips.push(samples);
                utterances.push(utterance);
            }
            Err(e) => eprintln!("skip {}: {e}", utterance.id),
        }
    }
    let results = transcribe_corpus(provider, &clips, config.force_endpoint_ms).await;

    let mut outcomes = Vec::new();
    for (n, (utterance, result)) in utterances.iter().zip(results).enumerate() {
        let (errors, words) = eval::word_errors(&utterance.reference, &result.hypothesis);
        let (terms_found, terms_missed): (Vec<String>, Vec<String>) = utterance
            .terms
            .iter()
            .cloned()
            .partition(|t| eval::contains_term(&result.hypothesis, t));
        println!(
            "[{}/{}] {:<10} {:>3} err/{:<3} {:>5} ms  {}",
            n + 1,
            utterances.len(),
            utterance.id,
            errors,
            words,
            result.latency_ms.map_or("-".into(), |v| v.to_string()),
            result.hypothesis
        );
        outcomes.push(Outcome {
            id: utterance.id.clone(),
            category: utterance.category.clone(),
            reference: utterance.reference.clone(),
            hypothesis: result.hypothesis,
            errors,
            words,
            terms_found,
            terms_missed,
            latency_ms: result.latency_ms,
            stt_error: result.error,
        });
    }

    let summary = summarize(&outcomes);
    let out = args
        .out
        .unwrap_or_else(|| root.join(format!("bench-{}.json", args.provider.id())));
    let report = serde_json::json!({
        "provider": args.provider.id(),
        "builtinVocabulary": args.builtin_vocabulary,
        "fastEndpointMs": args.fast_endpoint_ms,
        "summary": summary,
        "utterances": outcomes,
    });
    std::fs::write(&out, serde_json::to_string_pretty(&report).unwrap())
        .expect("cannot write report");
    println!("\nReport: {}", out.display());
}
