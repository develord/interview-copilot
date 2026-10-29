//! Question-detection benchmark (Phase 4). Replays the labeled dialogues of
//! `contracts/intent-corpus.v1.json` through the app's assembler, rules and Claude classifier
//! (one real API call per escalated utterance), then prints recall, false positives, intent and
//! follow-up accuracy, and classification latency.
//!
//! ```text
//! cargo run --features bench --bin intent-bench -- [--model haiku|opus] [--rules-only] [--holdout] [--out bench-intent.json]
//! cargo run --features bench --bin intent-bench -- --probe 10   # where the first-token delay goes
//! ```
//!
//! The key comes from `ANTHROPIC_API_KEY`, or from the key saved in the app (Windows Credential
//! Manager). `--rules-only` makes no API call and costs nothing.

use std::path::PathBuf;

use interview_copilot_lib::intent::classifier::{
    CallTiming, Classifier, ClassifyRequest, PROMPT_VERSION, Speaker,
};
use interview_copilot_lib::intent::claude::ClaudeClassifier;
use interview_copilot_lib::intent::eval::{self, Tally};
use interview_copilot_lib::intent::types::ClaudeModel;
use interview_copilot_lib::llm::credentials;
use serde::Serialize;

struct Args {
    model: Option<ClaudeModel>,
    out: Option<PathBuf>,
    only: Option<String>,
    /// `contracts/intent-corpus-holdout.v1.json` instead of the main corpus.
    holdout: bool,
    /// Latency probe: this many calls per request variant, no corpus.
    probe: Option<usize>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = std::env::args().skip(1);
    let mut parsed = Args {
        model: Some(ClaudeModel::Haiku55),
        out: None,
        only: None,
        probe: None,
        holdout: false,
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--model" => {
                parsed.model = Some(match args.next().as_deref() {
                    Some("opus") => ClaudeModel::Opus55,
                    Some("haiku") => ClaudeModel::Haiku55,
                    Some("sonnet") => ClaudeModel::Sonnet55,
                    other => return Err(format!("--model haiku|sonnet|opus, got {other:?}")),
                })
            }
            "--rules-only" => parsed.model = None,
            "--out" => parsed.out = args.next().map(PathBuf::from),
            "--dialogue" => parsed.only = args.next(),
            "--holdout" => parsed.holdout = true,
            "--probe" => {
                parsed.probe = Some(
                    args.next()
                        .and_then(|n| n.parse().ok())
                        .ok_or("--probe N (calls per variant)")?,
                )
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(parsed)
}

fn api_key() -> Result<String, String> {
    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        return Ok(key);
    }
    credentials::get()
        .map_err(|e| e.to_string())?
        .ok_or("no key: set ANTHROPIC_API_KEY or save one in the app".into())
}

/// Initial targets. PRD §12: thresholds are set once the corpus exists; §9: classification
/// median < 500 ms after finalization.
const MIN_RECALL: f64 = 0.90;
const MIN_STATEMENT_SUPPRESSION: f64 = 0.90;
const MIN_INTENT_ACCURACY: f64 = 0.80;
const MIN_FOLLOW_UP_ACCURACY: f64 = 0.75;
const MAX_P50_MS: u32 = 500;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    model: Option<&'static str>,
    prompt: &'static str,
    scores: eval::Scores,
    latency_p50_ms: Option<u32>,
    latency_p95_ms: Option<u32>,
    model_calls: usize,
    model_errors: Vec<String>,
    outcomes: Vec<eval::DialogueOutcome>,
}

fn line(name: &str, tally: Tally, min: Option<f64>) {
    let rate = tally.rate();
    let verdict = match (rate, min) {
        (Some(r), Some(m)) if r >= m => "pass",
        (Some(_), Some(_)) => "FAIL",
        _ => "",
    };
    let shown = rate.map_or("   –  ".into(), |r| format!("{:5.1} %", r * 100.0));
    println!(
        "  {name:<34} {shown}  ({}/{})  {verdict}",
        tally.correct, tally.expected
    );
}

/// Splits the first-token delay: a bare request (network + model start), then the
/// classification prompt without the JSON schema, then the full classification request.
async fn probe(classifier: &ClaudeClassifier, calls: usize) {
    let request = ClassifyRequest {
        utterance: "Comment tu gères le state management dans Vue ?".into(),
        speaker: Speaker::Interviewer,
        transcript: Vec::new(),
        recent_questions: Vec::new(),
    };
    let full = classifier.request_body(&request);
    let mut no_schema = full.clone();
    no_schema["output_config"]
        .as_object_mut()
        .expect("output_config")
        .remove("format");
    let mut bare = full.clone();
    let object = bare.as_object_mut().expect("body");
    object.remove("system");
    object.remove("output_config");
    object.insert("max_tokens".into(), 16.into());
    object.insert(
        "messages".into(),
        serde_json::json!([{ "role": "user", "content": "Réponds juste : OK" }]),
    );
    for (name, body) in [
        ("bare request (network + model start)", &bare),
        ("prompt, no JSON schema", &no_schema),
        ("full classification request", &full),
    ] {
        let mut first = Vec::new();
        let mut total = Vec::new();
        for _ in 0..calls {
            match classifier.send_once(body).await {
                Ok(s) => {
                    first.extend(s.first_token_ms);
                    total.push(s.total_ms);
                }
                Err(e) => println!("  {name}: {e}"),
            }
        }
        let p = |v: &[u32], q: f64| eval::percentile(v, q).map_or("–".into(), |x| x.to_string());
        println!(
            "  {name:<38} first token p50 {} ms · p95 {} ms · end p50 {} ms ({} calls)",
            p(&first, 50.0),
            p(&first, 95.0),
            p(&total, 50.0),
            total.len()
        );
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    let corpus = match eval::load(if args.holdout {
        eval::HOLDOUT
    } else {
        eval::CORPUS
    }) {
        Ok(corpus) => corpus,
        Err(e) => {
            eprintln!("invalid corpus: {e}");
            std::process::exit(2);
        }
    };
    let claude = match args.model {
        None => None,
        Some(model) => {
            let key = match api_key() {
                Ok(key) => key,
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(2);
                }
            };
            match ClaudeClassifier::new(key, model, None) {
                Ok(c) => Some(c),
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(2);
                }
            }
        }
    };
    if let Some(calls) = args.probe {
        let Some(claude) = &claude else {
            eprintln!("--probe needs a model");
            std::process::exit(2);
        };
        println!("Latency probe — {}", claude.model());
        probe(claude, calls).await;
        return;
    }
    let classifier: Option<Box<dyn Classifier>> =
        claude.map(|c| Box::new(c) as Box<dyn Classifier>);

    let dialogues: Vec<_> = corpus
        .dialogues
        .iter()
        .filter(|d| args.only.as_ref().is_none_or(|id| *id == d.id))
        .collect();
    let mut outcomes = Vec::new();
    for d in &dialogues {
        println!("dialogue {} ({} turns)", d.id, d.turns.len());
        let outcome = eval::simulate(d, classifier.as_deref()).await;
        // Scores after a key or billing error would only measure the rules: stop here.
        if let Some(error) = &outcome.fatal_error {
            eprintln!("\nstopped: the model is unusable ({error})");
            std::process::exit(1);
        }
        outcomes.push(outcome);
    }
    let scores = eval::score(dialogues.iter().copied(), &outcomes);

    let latencies: Vec<u32> = outcomes
        .iter()
        .flat_map(|o| o.model_latencies_ms.iter().copied())
        .collect();
    let errors: Vec<String> = outcomes
        .iter()
        .flat_map(|o| o.model_errors.iter().cloned())
        .collect();
    let p50 = eval::percentile(&latencies, 50.0);
    let p95 = eval::percentile(&latencies, 95.0);

    let model = classifier.as_ref().map(|c| c.model());
    println!(
        "\nQuestion detection — {} — prompt {PROMPT_VERSION} — {} corpus",
        model.unwrap_or("rules only"),
        if args.holdout { "hold-out" } else { "main" }
    );
    line(
        "questions detected (recall)",
        scores.questions,
        Some(MIN_RECALL),
    );
    line(
        "statements without a card",
        scores.statements,
        Some(MIN_STATEMENT_SUPPRESSION),
    );
    line("intent accepted", scores.intents, Some(MIN_INTENT_ACCURACY));
    line(
        "follow-ups on the right card",
        scores.follow_ups,
        Some(MIN_FOLLOW_UP_ACCURACY),
    );
    line("revisions instead of new cards", scores.revisions, None);
    line("repeats without a second card", scores.repeats, None);
    line("cut turns merged", scores.merges, None);
    for (language, s) in &scores.by_language {
        println!(
            "  [{language}] recall {} · statements {} · intents {}",
            s.questions
                .rate()
                .map_or("–".into(), |r| format!("{:.0} %", r * 100.0)),
            s.statements
                .rate()
                .map_or("–".into(), |r| format!("{:.0} %", r * 100.0)),
            s.intents
                .rate()
                .map_or("–".into(), |r| format!("{:.0} %", r * 100.0)),
        );
    }
    if let (Some(p50), Some(p95)) = (p50, p95) {
        let verdict = if p50 <= MAX_P50_MS { "pass" } else { "FAIL" };
        println!(
            "  classification latency            p50 {p50} ms · p95 {p95} ms ({} calls)  {verdict}",
            latencies.len()
        );
    }
    let timings: Vec<_> = outcomes
        .iter()
        .flat_map(|o| o.timings.iter().copied())
        .collect();
    if !timings.is_empty() {
        let series = |pick: &dyn Fn(&CallTiming) -> Option<u32>| -> String {
            let values: Vec<u32> = timings.iter().filter_map(pick).collect();
            match (
                eval::percentile(&values, 50.0),
                eval::percentile(&values, 95.0),
            ) {
                (Some(a), Some(b)) => format!("p50 {a} ms · p95 {b} ms"),
                _ => "–".into(),
            }
        };
        println!(
            "    first token                     {}",
            series(&|t| t.first_token_ms)
        );
        println!(
            "    decisive fields complete        {}",
            series(&|t| t.decision_ms)
        );
        println!(
            "    end of response                 {}",
            series(&|t| Some(t.total_ms))
        );
        let sum = |pick: fn(&CallTiming) -> u32| -> u64 {
            timings.iter().map(|t| u64::from(pick(t))).sum()
        };
        let (input, read, write, output) = (
            sum(|t| t.input_tokens),
            sum(|t| t.cache_read_tokens),
            sum(|t| t.cache_write_tokens),
            sum(|t| t.output_tokens),
        );
        println!(
            "    tokens: {input} input, {read} cache read, {write} cache write, {output} output"
        );
        if let Some(model) = args.model {
            // List prices per million tokens (2026-10); cache reads 0.1×, 5-minute writes 1.25×.
            let (price_in, price_out) = match model {
                ClaudeModel::Haiku55 => (0.10, 0.50),
                ClaudeModel::Sonnet55 => (2.00, 10.00),
                ClaudeModel::Opus55 => (4.00, 20.00),
            };
            let cost = (input as f64 + read as f64 * 0.1 + write as f64 * 1.25) * price_in / 1e6
                + output as f64 * price_out / 1e6;
            println!("    estimated cost of this run      ${cost:.4}");
        }
    }
    if !errors.is_empty() {
        println!(
            "\n{} model errors (the rules decided instead):",
            errors.len()
        );
        for e in errors.iter().take(10) {
            println!("  {e}");
        }
    }
    if !scores.failures.is_empty() {
        println!("\nWrong decisions:");
        for f in &scores.failures {
            println!("  {f}");
        }
    }

    let out = args
        .out
        .unwrap_or_else(|| PathBuf::from("bench-intent.json"));
    let report = Report {
        model,
        prompt: PROMPT_VERSION,
        scores,
        latency_p50_ms: p50,
        latency_p95_ms: p95,
        model_calls: latencies.len(),
        model_errors: errors,
        outcomes,
    };
    match serde_json::to_string_pretty(&report) {
        Ok(json) => match std::fs::write(&out, json) {
            Ok(()) => println!("\nReport: {}", out.display()),
            Err(e) => eprintln!("cannot write {}: {e}", out.display()),
        },
        Err(e) => eprintln!("cannot serialize the report: {e}"),
    }
}
