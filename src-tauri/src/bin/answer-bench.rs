//! Answer-engine benchmark (Phase 5). Sends each case of `contracts/answer-bench.v1.json`
//! through the app's answer prompt and model, then:
//!
//! - times the first token, the end of the main answer and the end of the response;
//! - checks the format (main answer length, details marker) and the language;
//! - asks a judge model (Claude Opus 5.5, structured output) to list every personal fact the
//!   answer states that the fictional verified profile does not support.
//!
//! ```text
//! cargo run --features bench --bin answer-bench -- [--model sonnet|haiku|opus] [--out bench-answers.json]
//! ```
//!
//! The key comes from `ANTHROPIC_API_KEY` or the key saved in the app. One run makes 12 answer
//! calls and 12 judge calls.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use interview_copilot_lib::answer::prompt::{AnswerInput, PROMPT_VERSION, request_body};
use interview_copilot_lib::answer::sections::Splitter;
use interview_copilot_lib::events::AnswerSection;
use interview_copilot_lib::intent::text;
use interview_copilot_lib::intent::types::{Intent, Language};
use interview_copilot_lib::llm::{ClaudeClient, ClaudeModel, Usage, credentials, millis};
use interview_copilot_lib::profile::Profile;
use serde::{Deserialize, Serialize};

const BENCH: &str = include_str!("../../../contracts/answer-bench.v1.json");
const JUDGE_PROMPT: &str = include_str!("../../prompts/eval-invented-facts.v1.md");
const JUDGE: ClaudeModel = ClaudeModel::Opus55;
/// The main answer should be readable in a few seconds (prompt: "at most about 90 words").
const MAX_PRIMARY_WORDS: usize = 110;

#[derive(Deserialize)]
struct Bench {
    profile: Profile,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    id: String,
    question: String,
    intent: Intent,
    language: Language,
    #[serde(default)]
    missing_fact: bool,
    #[serde(default)]
    earlier: Option<Earlier>,
}

#[derive(Deserialize)]
struct Earlier {
    question: String,
    answer: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Outcome {
    id: String,
    primary: String,
    details: String,
    first_token_ms: Option<u32>,
    primary_done_ms: u32,
    total_ms: u32,
    primary_words: usize,
    language_ok: bool,
    has_placeholder: bool,
    invented: Vec<String>,
    error: Option<String>,
    usage: Usage,
}

fn api_key() -> Result<String, String> {
    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        return Ok(key);
    }
    credentials::get()
        .map_err(|e| e.to_string())?
        .ok_or("no key: set ANTHROPIC_API_KEY or save one in the app".into())
}

fn percentile(values: &[u32], p: f64) -> Option<u32> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let rank = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
    Some(sorted[rank.clamp(1, sorted.len()) - 1])
}

fn price(model: ClaudeModel) -> (f64, f64) {
    // List prices per million tokens (2026-10); cache reads 0.1×, 5-minute writes 1.25×.
    match model {
        ClaudeModel::Haiku55 => (0.10, 0.50),
        ClaudeModel::Sonnet55 => (2.00, 10.00),
        ClaudeModel::Opus55 => (4.00, 20.00),
    }
}

fn cost(model: ClaudeModel, usage: &Usage) -> f64 {
    let (input, output) = price(model);
    (f64::from(usage.input_tokens)
        + f64::from(usage.cache_read_tokens) * 0.1
        + f64::from(usage.cache_write_tokens) * 1.25)
        * input
        / 1e6
        + f64::from(usage.output_tokens) * output / 1e6
}

async fn judge(
    client: &ClaudeClient,
    profile: &Profile,
    answer: &str,
) -> Result<(Vec<String>, Usage), String> {
    let mut body = serde_json::json!({
        "model": JUDGE.id(),
        "max_tokens": 4_000,
        "stream": true,
        "system": JUDGE_PROMPT,
        "messages": [{
            "role": "user",
            "content": format!("<profile>\n{}\n</profile>\n<answer>\n{answer}\n</answer>", profile.render())
        }],
        "output_config": {
            "effort": "medium",
            "format": { "type": "json_schema", "schema": {
                "type": "object",
                "properties": { "unsupported": { "type": "array", "items": { "type": "string" } } },
                "required": ["unsupported"],
                "additionalProperties": false
            } }
        }
    });
    JUDGE.apply_settings(&mut body);
    let streamed = client
        .stream(&body, JUDGE.beta(), &mut |_, _, _| {})
        .await
        .map_err(|e| e.to_string())?;
    #[derive(Deserialize)]
    struct Verdict {
        unsupported: Vec<String>,
    }
    let verdict: Verdict = serde_json::from_str(streamed.text.trim()).map_err(|e| e.to_string())?;
    Ok((verdict.unsupported, streamed.usage))
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    let mut model = ClaudeModel::Sonnet55;
    let mut out = PathBuf::from("bench-answers.json");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--model" => {
                model = match args.next().as_deref() {
                    Some("sonnet") => ClaudeModel::Sonnet55,
                    Some("haiku") => ClaudeModel::Haiku55,
                    Some("opus") => ClaudeModel::Opus55,
                    other => {
                        eprintln!("--model sonnet|haiku|opus, got {other:?}");
                        std::process::exit(2);
                    }
                }
            }
            "--out" => out = args.next().map(PathBuf::from).unwrap_or(out),
            other => {
                eprintln!("unknown argument {other}");
                std::process::exit(2);
            }
        }
    }
    let bench: Bench = serde_json::from_str(BENCH).expect("contracts/answer-bench.v1.json");
    let key = match api_key() {
        Ok(key) => key,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let client = ClaudeClient::new(key.clone(), Duration::from_secs(90), None).expect("client");
    let judge_client = ClaudeClient::new(key, Duration::from_secs(120), None).expect("client");

    println!("Answer bench — {} — prompt {PROMPT_VERSION}\n", model.id());
    let mut outcomes = Vec::new();
    let (mut answer_usage, mut judge_usage) = (Usage::default(), Usage::default());
    for case in &bench.cases {
        let input = AnswerInput {
            question: case.question.clone(),
            as_heard: case.question.clone(),
            intent: Some(case.intent),
            language: Some(case.language),
            summary: None,
            transcript: Vec::new(),
            earlier: case
                .earlier
                .as_ref()
                .map(|e| (e.question.clone(), e.answer.clone())),
        };
        let body = request_body(model, &input, Some(&bench.profile));
        let started = Instant::now();
        let mut splitter = Splitter::default();
        let (mut primary, mut details, mut primary_done) = (String::new(), String::new(), None);
        let mut route = |parts: Vec<(AnswerSection, String)>, at: u32| {
            for (section, text) in parts {
                match section {
                    AnswerSection::Primary => primary.push_str(&text),
                    AnswerSection::Details => {
                        primary_done.get_or_insert(at);
                        details.push_str(&text);
                    }
                }
            }
        };
        let result = client
            .stream(&body, model.beta(), &mut |piece, _, at| {
                route(splitter.push(piece), at)
            })
            .await;
        let end = millis(started);
        route(splitter.finish(), end);

        let mut outcome = Outcome {
            id: case.id.clone(),
            primary: primary.trim().to_string(),
            details: details.trim().to_string(),
            first_token_ms: None,
            primary_done_ms: primary_done.unwrap_or(end),
            total_ms: end,
            primary_words: primary.split_whitespace().count(),
            language_ok: true,
            has_placeholder: primary.contains('[') || details.contains('['),
            invented: Vec::new(),
            error: None,
            usage: Usage::default(),
        };
        match result {
            Ok(streamed) => {
                outcome.first_token_ms = streamed.first_token_ms;
                outcome.usage = streamed.usage;
                let detected = text::guess_language(&text::tokens(&outcome.primary));
                outcome.language_ok = case.language == Language::Mixed || detected == case.language;
                for (sum, add) in [
                    (&mut answer_usage.input_tokens, streamed.usage.input_tokens),
                    (
                        &mut answer_usage.cache_read_tokens,
                        streamed.usage.cache_read_tokens,
                    ),
                    (
                        &mut answer_usage.cache_write_tokens,
                        streamed.usage.cache_write_tokens,
                    ),
                    (
                        &mut answer_usage.output_tokens,
                        streamed.usage.output_tokens,
                    ),
                ] {
                    *sum += add;
                }
                let full = format!("{}\n{}", outcome.primary, outcome.details);
                match judge(&judge_client, &bench.profile, &full).await {
                    Ok((invented, usage)) => {
                        outcome.invented = invented;
                        judge_usage.input_tokens += usage.input_tokens;
                        judge_usage.cache_read_tokens += usage.cache_read_tokens;
                        judge_usage.cache_write_tokens += usage.cache_write_tokens;
                        judge_usage.output_tokens += usage.output_tokens;
                    }
                    Err(e) => outcome.error = Some(format!("judge: {e}")),
                }
            }
            Err(e) => outcome.error = Some(e.to_string()),
        }
        println!(
            "  {:<18} first {:>5} ms · main {:>5} ms · end {:>5} ms · {:>3} words{}{}{}{}",
            outcome.id,
            outcome.first_token_ms.map_or("–".into(), |v| v.to_string()),
            outcome.primary_done_ms,
            outcome.total_ms,
            outcome.primary_words,
            if outcome.primary_words > MAX_PRIMARY_WORDS {
                " (too long)"
            } else {
                ""
            },
            if outcome.language_ok {
                ""
            } else {
                " · WRONG LANGUAGE"
            },
            if case.missing_fact && !outcome.has_placeholder {
                " · no placeholder"
            } else {
                ""
            },
            outcome
                .error
                .as_ref()
                .map_or(String::new(), |e| format!(" · ERROR {e}")),
        );
        for fact in &outcome.invented {
            println!("      invented: {fact}");
        }
        outcomes.push(outcome);
    }

    let firsts: Vec<u32> = outcomes.iter().filter_map(|o| o.first_token_ms).collect();
    let mains: Vec<u32> = outcomes
        .iter()
        .filter(|o| o.error.is_none())
        .map(|o| o.primary_done_ms)
        .collect();
    let fmt = |v: Option<u32>| v.map_or("–".into(), |x| format!("{x} ms"));
    let invented: usize = outcomes.iter().map(|o| o.invented.len()).sum();
    let clean = outcomes
        .iter()
        .filter(|o| o.invented.is_empty() && o.error.is_none())
        .count();
    let missing: Vec<&Outcome> = bench
        .cases
        .iter()
        .zip(&outcomes)
        .filter(|(c, _)| c.missing_fact)
        .map(|(_, o)| o)
        .collect();
    println!("\nSummary");
    println!(
        "  first token                  p50 {} · p95 {}",
        fmt(percentile(&firsts, 50.0)),
        fmt(percentile(&firsts, 95.0))
    );
    println!(
        "  main answer complete         p50 {} · p95 {}",
        fmt(percentile(&mains, 50.0)),
        fmt(percentile(&mains, 95.0))
    );
    println!(
        "  answers without invented fact {clean}/{} ({invented} unsupported facts in total)",
        outcomes.len()
    );
    println!(
        "  placeholders on missing facts {}/{}",
        missing.iter().filter(|o| o.has_placeholder).count(),
        missing.len()
    );
    println!(
        "  main answers within {MAX_PRIMARY_WORDS} words  {}/{}",
        outcomes
            .iter()
            .filter(|o| o.primary_words <= MAX_PRIMARY_WORDS)
            .count(),
        outcomes.len()
    );
    println!(
        "  language as asked            {}/{}",
        outcomes.iter().filter(|o| o.language_ok).count(),
        outcomes.len()
    );
    println!(
        "  estimated cost               answers ${:.4} · judge ${:.4}",
        cost(model, &answer_usage),
        cost(JUDGE, &judge_usage)
    );
    match serde_json::to_string_pretty(&outcomes) {
        Ok(json) => match std::fs::write(&out, json) {
            Ok(()) => println!("\nReport: {}", out.display()),
            Err(e) => eprintln!("cannot write {}: {e}", out.display()),
        },
        Err(e) => eprintln!("{e}"),
    }
}
