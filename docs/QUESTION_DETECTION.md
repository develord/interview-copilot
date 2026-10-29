# Question detection (Phase 4)

Status: implemented, unit-tested and measured. On the hold-out corpus, Claude Haiku 5.5 (the
default model, prompt v3) detects 94 % of the questions, creates no card for statements, and
gets intents and follow-ups right. **Latency is the open point:** about 1.1–1.3 s per
classification, against a 500 ms target. The floor is ~750 ms on the server side (see
[Latency](#latency)).

The understanding engine decides **when** the candidate needs an answer, and what kind. It
turns final transcripts into `question.detected` / `question.revised` events for the card
lifecycle, plus `utterance.classified` diagnostics for the transcript panel. Code:
`src-tauri/src/intent/`.

## Pipeline

```
transcript.final / .partial (per source)
   │  tapped in audio_start, after the webview gets them
   ▼
assembler task  ── merges cut turns, releases utterances on time (never waits for the model)
   │ utterances, in order
   ▼
classifier task ── detector: context, rules (stage 1), repeat check
   │                 └─ escalated → Claude (stage 2), one call at a time, in order
   ▼
question.detected · question.revised · utterance.classified · classifier.status
```

Only the system-audio source (the interviewer) asks questions. Microphone speech (the
candidate) is kept as context for the model. A setting, **Détecter aussi les questions dites
au micro**, treats microphone speech as interviewer speech so the feature can be tested alone.

**Questions techniques uniquement** (on by default) keeps cards and answers for
`TECHNICAL_QUESTION`, `CODING_PROBLEM`, `SYSTEM_DESIGN`, and follow-ups on a question that
has a card. Greetings, career and behavioral questions, and follow-ups on them, are reported as
ignored (`not-technical`): no card, no answer. The detector still tracks them, so their
repeats and follow-ups are recognised. A continuation of a left-out question is judged anew:
it gets a card if the completed question is technical. Typed questions are always answered.

### 1. Assembling utterances (`assembler.rs`)

With the 700 ms fast endpoint, a hesitation can close a turn mid-sentence ("Et du coup, sur
la partie"). A final that looks cut waits up to **1.2 s** for its continuation, and the two
are merged into one utterance with both segment ids. A final looks cut when it ends on a
comma, colon, ellipsis or hyphen, has no final punctuation, or ends on a continuation word
("de", "la", "et", "parce", "the", "and"…). Those words are matched with accents kept, so
"bien sûr." is complete. Partials from the same speaker extend the wait, with a hard limit of
**6 s** from the first segment: the wait always ends. A cut turn still waiting when the
session stops is flushed and classified.

### 2. First stage: rules (`rules.rs`)

The rules are deterministic and explainable. They work on accent-folded tokens, so
"Qu’est-ce que" and "qu'est ce que" match the same pattern. They do three things:

| Case                                       | Example                                                   | Decision                                                        |
| ------------------------------------------ | --------------------------------------------------------- | --------------------------------------------------------------- |
| Hesitation only                            | « Euh… hum. »                                             | ignored (`filler`), no model call                               |
| Acknowledgement                            | « D'accord, très bien. », « Okay, got it. »               | ignored (`backchannel`), no model call                          |
| Short greeting / small talk                | « Bonjour Mohamed, merci d'être là. », « Tu m'entends ? » | card `GREETING_OR_SMALL_TALK` at once                           |
| Bare follow-up within 3 min of a question  | « Pourquoi ? », « Et en mémoire » is **not** bare         | card `CLARIFICATION_OR_FOLLOW_UP` linked to the latest question |
| Same words as a question asked < 2 min ago | the interviewer repeats the question                      | ignored (`repeat`)                                              |
| Anything else                              |                                                           | escalated to the model, with hints                              |

The hints record whether there is an explicit cue (a question mark, an interrogative or
imperative opening such as « comment », « est-ce que », « explique », « walk me through »,
or a request such as « j'aimerais que tu », « je te laisse »), an intent guess from keyword
lexicons, the language (from function words) and a domain from the built-in technical
vocabulary. **The rules are never the only question detector** (PRD §4). The hints decide on
their own only when the model is unavailable.

A greeting said twice (« Bonjour bonjour ! ») is one card; small talk that asks something
(« Est-ce que tu m'entends bien ? ») after a greeting gets its own card. The corpus found
this bug; a regression test now covers it.

### 3. Second stage: Claude (`classifier.rs`, `claude.rs`)

One Messages API call per escalated utterance, in Rust: the key never reaches the webview.

- **Model:** `claude-haiku-5-5` by default (chosen on measurements, see Results), or
  `claude-opus-5-5`, chosen in Réglages.
- **Request:**
  - `anthropic-version: 2026-10-21`, with no sampling parameters (that version rejects them).
  - `stream: true`. The adapter reads the server-sent events, which lets it time the first
    token, the moment the decisive fields are complete, and the end of the response. It also
    records token usage, including cache reads.
  - `output_config.effort: "low"` and `output_config.format` set to a JSON schema, so the
    output is structured. **The model generates fields in schema order.** Without serde_json's
    `preserve_order` feature the keys were sorted alphabetically, so the order was accidental.
    It is now explicit: decisive fields first (`requiresResponse`, `intent`, `relation`,
    `contextReference`, `language`), then `question`, `domain`, `urgency`, `confidence`.
  - The versioned prompt [`prompts/classification.v3.md`](../src-tauri/prompts/classification.v3.md)
    is the system block, marked with `cache_control: ephemeral`. It is 3.9 kB, a third shorter
    than v2. The bench shows cache reads on every call after the first.
  - Opus only: `fallbacks: "default"` with the `server-side-fallback-2026-07-01` beta header.
    If a safety classifier declines, the request is re-run server-side on the model Anthropic
    recommends. Haiku 5.5 has no server-side fallback.
  - Haiku only: `thinking: {"type": "disabled"}`, the fastest setting. On Opus 5.5 thinking
    cannot be turned off; low effort keeps it short.
- **Context:** bounded, never the whole transcript.
  - The last 10 lines, with `[interviewer]` / `[candidate]` labels.
  - The last 6 questions, with short aliases Q1…Q6 mapped back to real ids.
  - The utterance. Angle brackets are stripped so speech cannot close the prompt's tags.
- **Output** (validated at runtime, `deny_unknown_fields`):
  - `question`: a normalized, self-contained question.
  - `language`, `intent`, `requiresResponse`, `relation`, `contextReference`, `domain`,
    `urgency` and `confidence`.
  - `relation` is one of `new`, `follow_up`, `continuation` (completes or corrects a question,
    which becomes `question.revised`) or `repeat` (no new card).
  - A reference to an unknown alias demotes the relation to `new`. Confidence is clamped to
    [0, 1]. An unknown domain becomes null. An empty question falls back to the utterance.
- **Decision:**
  - A `repeat` creates no card.
  - A `continuation` revises its target. Rule 4 of the card lifecycle applies: the text
    updates while the card is pending, and is recorded as a correction afterwards.
  - Otherwise a card is created when `requiresResponse` is true and the intent is not
    `AMBIGUOUS`, and, with « Questions techniques uniquement », when the question is in scope.
  - An ambiguous fragment creates no card but stays in the context of the next call.
- **Errors:** a timeout (10 s on Opus, 6 s on Haiku), 429, 5xx/529, a network failure,
  invalid JSON, a refusal or a truncation never crashes the session.
  - 429, 5xx/529 and network failures are retried once after 0.5 s, or after `retry-after`
    if it is under 3 s. A timeout is not retried.
  - The rules then decide for that utterance: explicit questions still get a card, statements
    do not. `classifier.status` turns `degraded`.
  - A rejected key, exhausted credit or a credential-store failure switches the session to
    rules only (`error`).
  - With no key at all, the session runs on rules only from the start (`rules-only`), and the
    status strip says so.

### Interface

- **Réglages › Audio › Détecter les questions:**
  - on/off, which needs transcription;
  - the model (Haiku 5.5 by default, or Opus 5.5);
  - the Claude key, kept in the Windows Credential Manager under
    `dev.interviewcopilot.desktop/llm` / `anthropic`. The webview can store or delete the key
    but never read it;
  - the microphone test switch.
- **Transcript panel:** the line that ends each utterance gets a tag, for example
  « Question · Technique », « Précision de la question », « Ignoré · affirmation ». Its tooltip
  says who decided: the rules, Claude with its latency, or the rules because Claude was
  unavailable. The panel header shows the classifier state and its last latency.
- **Status strip:** shows why detection is running without Claude.
- **Answers stay simulated until Phase 5.** Each detected question gets a card with a placeholder
  answer that names the detected intent. Cards, retries and cancellation use the production
  paths.

## Privacy

When detection is on and a key is stored, the following is sent to the Anthropic API:

- the interviewer's utterance;
- the last 10 transcript lines, from both speakers;
- the texts of the last 6 detected questions.

Audio is never sent to Anthropic. The consent step now says that the transcript is analyzed by
Claude (Anthropic) to find questions. Anthropic's servers are not EU-only, unlike the STT
endpoints. Check your organization's retention settings before using the feature in real
interviews.

## Evaluation

[`contracts/intent-corpus.v1.json`](../contracts/intent-corpus.v1.json) holds 11 labeled
dialogues (120 turns: FR, EN and mixed). They are written as punctuated live speech.

They cover the PRD §12 fixture categories:

- greetings and small talk;
- explicit and indirect questions (« J'aimerais comprendre… », « Je te laisse te présenter »);
- coding, system design, experience, behavioral and HR questions;
- statements, acknowledgements and rhetorical questions (« Qu'est-ce qu'on disait déjà ? Ah
  oui… », « Comment dire… »);
- bare and contextual follow-ups;
- self-corrections;
- continuations that must revise a card;
- exact and rephrased repeats;
- a cut turn that must be merged;
- similar but different consecutive questions (blue-green, then canary).

The corpus is validated on load: every reference must point to an earlier question.

`intent/eval.rs` replays each dialogue on a virtual clock. It uses the real assembler, rules,
detector and, optionally, the classifier, then scores the cards against the labels. Two unit
tests use it:

- **rules only:** guards the first stage with minimum scores;
- **oracle model:** a fake model that answers from the labels. It must score 100 %, which
  proves the harness and the event plumbing. It found the greeting bug above.

```text
cargo run --features bench --bin intent-bench -- --rules-only               # free, no API call
cargo run --features bench --bin intent-bench                               # Haiku, ~58 calls, ~$0.006
cargo run --features bench --bin intent-bench -- --holdout                  # the honest estimate
cargo run --features bench --bin intent-bench -- --model opus               # Opus 5.5
cargo run --features bench --bin intent-bench -- --probe 10                 # where latency goes
```

The key comes from `ANTHROPIC_API_KEY` or the key saved in the app. The bench stops at the first key or billing error, because the rest of the run would only measure the rules; only successful calls count toward the latency. As with STT, a key saved
from an agent-launched instance stays in the agent's sandbox: launch the app yourself
(`scripts/dev.ps1`) to save it.

### Results

All runs on 2026-10-28, from France to the Claude API.

**Hold-out corpus.** [`contracts/intent-corpus-holdout.v1.json`](../contracts/intent-corpus-holdout.v1.json)
has 4 dialogues and 47 turns. They were written after prompt v3 was frozen and have never been
used to tune anything. **This is the honest estimate.**

| Metric (initial target)                     | Rules only      | Haiku 5.5, v3 (2 runs)               |
| ------------------------------------------- | --------------- | ------------------------------------ |
| Questions detected, recall (≥ 90 %)         | 82.4 % (14/17)  | **94.1 %** (16/17) both runs         |
| Statements without a card (≥ 90 %)          | 100 % (11/11)   | **100 %**                            |
| Intent accepted (≥ 80 %)                    | 71.4 % (10/14)  | **100 %**                            |
| Follow-ups on the right card (≥ 75 %)       | 0 % (0/3)       | **100 %** (3/3)                      |
| Revision / repeat / merge                   | 0/1 · 1/1 · 1/1 | 1/1 · 1/1 · 1/1                      |
| Latency: first token / decision / end (p50) | n/a             | 795–988 / 952–1 152 / 1 104–1 300 ms |

The one miss in both runs is the closing « Merci beaucoup pour cet échange. ». The model judged
that no reply was needed. That is debatable.

**Main corpus** (120 turns). Read these numbers with care:

- v1 and v2 quoted phrases from the corpus. In v1, an example shared 10 consecutive words
  with a turn; in v2, a definition still quoted « Je te laisse te présenter ».
- v3 shares no more than two consecutive words with the corpus.
- But two of v3's rules come from errors observed on this corpus: announcing the next topic
  is a statement, and a self-introduction is an experience question.

| Run (Haiku 5.5 unless stated)                 | Recall     | Statements  | Intents     | Follow-ups | Errors        | End p50            |
| --------------------------------------------- | ---------- | ----------- | ----------- | ---------- | ------------- | ------------------ |
| Rules only                                    | 93.2 %     | 85.7 %      | 78.0 %      | 22.2 %     | 27            | n/a                |
| v1, keys sorted alphabetically (leaky)        | 100 %      | 100 %       | 97.7 %      | 100 %      | 1             | 1 477 ms           |
| v2, keys sorted alphabetically (non-streamed) | 100 %      | 100 %       | 100 %       | 88.9 %     | 1 + 1 timeout | 1 360 ms           |
| v2, decisive fields first ×2                  | 100 %      | 96.4 %      | 95.5–97.7 % | 100 %      | 2–3           | 1 336–1 396 ms     |
| v2, question first ×2                         | 97.7–100 % | 92.9–96.4 % | 95.5–97.7 % | 100 %      | 3–4           | 1 288–1 396 ms     |
| v2, alphabetical ×2                           | 97.7–100 % | 96.4–100 %  | 97.7 %      | 77.8 %     | 3–5           | 1 305–1 320 ms     |
| **v3, decisive fields first ×2**              | **100 %**  | **100 %**   | **100 %**   | **100 %**  | **0**         | **1 223–1 301 ms** |

- The order of the fields changes nothing clear beyond run-to-run variance (2 to 5 errors).
  The decisive-first order is kept: it had the fewest errors with v2, and it makes the decision
  available about 150 ms before the end of the response.
- One full bench run costs about $0.006 with Haiku (measured from token usage).

### Latency

The bench and a three-variant probe (`intent-bench --probe 10`) split the time:

| Request (Haiku 5.5, 10 calls each)                     | First token p50                       |
| ------------------------------------------------------ | ------------------------------------- |
| Bare request: « Réponds juste : OK », no system prompt | ~740–760 ms                           |
| Classification prompt, no JSON schema                  | ~730–1 060 ms                         |
| Full classification request                            | ~1 000–1 300 ms (end: 1 100–1 600 ms) |

- **Not the network.** TCP connects in 15–50 ms (`curl`), and a rejected unauthenticated
  request returns in 180–340 ms.
- **Not the JSON schema.** It adds no measurable time.
- **Not the output.** Generating the whole JSON takes about 300 ms after the first token.
- **The floor is server side:** about 750 ms before the first token, even for a bare request.
  The prompt adds roughly 100–300 ms; v3 recovered part of that.
- **Opus 5.5 cannot be faster.** It always thinks before answering. It was not benched, since
  Haiku already passes every quality metric.

**Conclusion.** The PRD §9 target, a classification median under 500 ms, is not reachable with
any remote model call from here: the floor alone is ~750 ms. One reading of the PRD
(« excluding provider delays ») excludes the API time; under that reading, our own processing
adds well under 10 ms, except the 1.2 s wait on cut turns. The user-facing target is the first
answer text within 2 s of finalization. It needs the following:

1. **Phase 5:** classify and start the answer in **one** streamed call. The decisive fields come
   first, then the answer, which saves a full round trip (~1 s). The partial JSON reader
   (`classifier::complete_fields`) and the decisive-first order are already in place.
2. **Optional:** start classifying on the latest partial transcript at local end of speech,
   while the STT finalizes (about 600 ms p50, see TRANSCRIPTION.md). Keep the result if the
   final text matches. The PRD allows partials to prepare classification as long as they never
   create cards.
3. **Not done:** creating the card at the decision, about 150 ms before the end. It saves too
   little for a new event type in the card lifecycle.

## Known limitations

- The corpus is text only and written by hand. Real recorded interviews will show other errors:
  recognition mistakes and turns split differently. Grow the corpus from the real recordings
  planned in Phase 3, with consent.
- The thresholds are initial. PRD §12 sets them once the corpus is representative.
- `urgency` and `confidence` are validated but not used yet. Confidence is not calibrated.
  Prioritization waits for the answer engine.
- Manual questions typed in the overlay still use the mock pipeline's heuristics. They will go
  through the classifier when generation moves to Rust (Phase 5).
- Classification is sequential. If the model is slow, utterances queue, so the order and the
  context stay correct. A burst of short utterances adds latency.

## Next steps

1. Live test in the app with Haiku: play an interview video, or turn on the microphone switch
   and ask questions yourself.
2. Phase 5: generation in the same streamed call as the classification (see Latency).
3. Grow both corpora from real recordings (with consent). Keep the hold-out set out of any
   tuning.
