# Answer engine (Phases 5 and 6)

Status: implemented and unit-tested; the answer benchmark (`answer-bench`) has **not been run
yet**. Code: `src-tauri/src/answer/`, `src-tauri/src/profile/`, `src-tauri/src/llm/`, and the
session wiring in `src-tauri/src/pipeline.rs`.

## Flow

```
utterance sent to the classifier (early start) ──┐  held until the decision
question.detected (speech, from the question engine) ─┤  adopts the held answer, or starts one
answer_ask_manual (overlay text field) ───────────────┤
                                                      ▼
                          AnswerEngine (Rust, one task per question)
                              │ prompt answer.v2 + verified profile (cached)
                              │ + rolling summary + last 12 lines
                              ▼
                          Claude Sonnet 5.5, streamed
                              │ text deltas → section splitter (<<<DETAILS>>>)
                              ▼
          answer.started → answer.delta (primary / details, seq 0…) → answer.completed | answer.failed
```

- **In Rust**, like question detection: the Claude key never reaches the webview. The engine
  observes the same pipeline events as the overlay, and the overlay always gets an event first,
  so a card exists before its answer starts.
- **One generation per question, all concurrent.** Each has its own request id
  (`{question}-a{n}`). A retry or a restart aborts only that question's previous request; late
  tokens of an aborted request carry its old id and are rejected by the card lifecycle as
  stale (rules 2 and 6, [UI_STATE_MACHINE.md](UI_STATE_MACHINE.md)).
- **Revisions.** A `question.revised` writes the answer again, even a completed one: either a
  pause split the question, so the answer was for its first half, or the interviewer narrowed
  it ("En production, j'entends"). The card adopts the revised question (rule 4).
- **Reading check.** The question engine asks `reads_answer` whether microphone speech comes
  from one of the 3 latest answers shown (both sections). Testing alone with « Questions au
  micro », the candidate reading an answer out, or speaking from it, is then not a question
  ([QUESTION_DETECTION.md](QUESTION_DETECTION.md)).
- **Manual questions** are sent to Rust in live mode. They get their intent from the first-stage
  rules, `source: "manual"`, and the same answer path, whatever their intent. The demo mode
  keeps the mock pipeline.
- **Scope.** With « Questions techniques uniquement » (the default), spoken non-technical
  questions get no `question.detected`, so no card and no answer. An early answer already
  started for one is aborted unseen when the classifier decides.
- **Cancel all** (clearing the history) aborts every generation and forgets the remembered
  questions, the transcript context and the summary.

## Early start (Phase 6)

Question detection takes about 1.2 s ([QUESTION_DETECTION.md](QUESTION_DETECTION.md)), and the
answer used to start only after it. Now, when the question engine sends an utterance to the
classifier, it also tells the answer engine (`on_escalate`), which starts the answer at once:

- The output is **held**, never shown, until the decision.
- **New question:** the held text is replayed into the card, and the generation goes on live.
  The utterance is matched by its transcript segment ids. No second request is made.
- **Follow-up:** the held answer is dropped. A new one starts with the earlier question and
  its answer, which the early request did not have.
- **Repaired words:** the held answer is also dropped when the classifier's question has a
  content word the utterance lacked, give or take an ending: a term it repaired ("M et REM" →
  "em et rem") or a reference it resolved. The answer then starts from the cleaned question,
  about 1 s later, instead of answering misheard words.
- **Anything else** (statement, repeat, precision of an earlier question): the held answer is
  aborted when `utterance.classified` arrives. It is also dropped after 20 s without a
  decision, and at most 4 are held at once.
- **Failure:** if the early answer failed before the decision, the question starts a normal
  answer.

The early request has no intent and no language: the prompt tells the model to pick the
template that fits. Greetings and bare follow-ups are decided by the rules without the model,
so they are answered as before.

This keeps the measured classifier unchanged. One combined call that classifies and answers
would have needed the detection benchmark redone.

**Cost.** Each statement the rules send to the classifier also starts an answer, aborted about
1 s later. That is about $0.001 with Sonnet: the cached prefix, about 400 input tokens and a few
output tokens. It is on by default; turn off « Réponse anticipée » in Réglages › Audio ›
Réponses to save it.

## Rolling summary (Phase 6)

The answer context keeps the last 12 transcript lines. Older lines are folded, 12 at a time,
into a summary of at most 150 words, written by Claude Haiku 5.5 with prompt
[`prompts/summary.v1.md`](../src-tauri/prompts/summary.v1.md). Every answer gets it as
`<interview_summary>`.

- Only one summary call runs at a time.
- A failed call keeps its lines (48 at most) and is tried again 60 s later.
- A summary that comes back after the history was cleared is dropped.

Cost: about $0.0003 per call.

## Request

| Item          | Value                                                                                                                                                            |
| ------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Model         | Claude Sonnet 5.5 by default; Haiku 5.5 and Opus 5.5 in Réglages › Audio › Réponses                                                                              |
| Thinking      | Off: `{"type": "between_tools"}` on Sonnet, `disabled` on Haiku. Opus 5.5 always thinks                                                                          |
| Effort        | `medium`                                                                                                                                                         |
| Fallback      | `fallbacks: "default"` with the `server-side-fallback-2026-07-01` beta (Sonnet, Opus)                                                                            |
| System prefix | [`prompts/answer.v2.md`](../src-tauri/prompts/answer.v2.md), then the verified profile, with one `cache_control` breakpoint after both                           |
| User message  | The last 12 transcript lines (interviewer and candidate), the earlier question and answer for a follow-up, and the cleaned question with its intent and language |
| Output        | Main answer: 2 or 3 lines « - terme : fait » of ≤ ~12 words, no opener, no code; then optionally `<<<DETAILS>>>` and at most 4 lines or one short code block     |
| Limits        | `max_tokens` 3000; 60 s per response; one retry before the first token for 429/5xx/network errors                                                                |

The prompt sets the rules (PRD §5):

- answer the actual question, in the interviewer's language;
- spoken style, in the first person;
- personal facts **only** from the profile, with `[placeholders]` for missing ones;
- accuracy over confidence;
- answer only the new point of a follow-up;
- one template per intent: technical, coding, system design, experience, behavioral (STAR),
  greeting, follow-up.

The splitter never shows a partial `<<<DETAILS>>>`. Text that could be its start, and
whitespace before it, is held back until the next delta. It is tested for every cut position
and for one character at a time.

Failures keep the partial answer and the card, with a French message (« Connexion à Claude
perdue. », « Pas de clé Claude… »). They are retryable, except for refusals and malformed
requests.

## Verified profile

- **Import:** drag the CV PDF onto Réglages › Audio › Profil vérifié, or pick it. The webview reads
  the file and sends its bytes; Rust never opens a path chosen by the page. HTML5 drag and drop
  needs `dragDropEnabled: false` on Windows, and drops elsewhere in the window are ignored.
- **Extraction:** once, by Claude Opus 5.5, with the PDF as a `document` block and structured
  output. The prompt is [`prompts/profile-extraction.v1.md`](../src-tauri/prompts/profile-extraction.v1.md):
  copy facts only, infer nothing, keep the CV's language.
- **Review:** the draft opens as editable fields, with nothing saved yet. « Valider le profil »
  saves it and marks it verified. Only a saved profile is used. A file on disk without
  `verifiedAt` is ignored.
- **Manual profile:** « remplir le profil à la main » works without a CV.
- **Storage:** `profile.json` in the app data directory (`%APPDATA%\dev.interviewcopilot.desktop`).
  It is plain JSON, protected only by the Windows account. Delete it with « Supprimer ».

## Privacy

Sent to the Anthropic API:

- the CV, once, at import;
- the verified profile, with every answer;
- the question, and with early start every utterance sent to the classifier;
- the last 12 transcript lines, and the older ones once, to be summarized;
- the rolling summary;
- for a follow-up, the earlier question and its answer.

Audio is never sent to Anthropic.

## Evaluation

[`contracts/answer-bench.v1.json`](../contracts/answer-bench.v1.json) holds a fictional verified
profile and 12 questions: greeting, self-introduction, technical FR/EN/mixed, a follow-up,
coding, system design, behavioral, and four questions whose personal facts are not in the
profile (team managed, salary, a disagreement, reasons to leave).

```text
cargo run --features bench --bin answer-bench -- [--model sonnet|haiku|opus]
```

For each answer, the bench reports:

- the time to the first token, to the end of the main answer, and to the end of the response;
- the main answer's length;
- whether the language is the one asked;
- whether a placeholder is used where a fact is missing;
- every personal fact a judge model finds unsupported by the profile. The judge is Claude
  Opus 5.5 with prompt [`prompts/eval-invented-facts.v1.md`](../src-tauri/prompts/eval-invented-facts.v1.md)
  and structured output.

One run costs roughly $0.25: about $0.07 for the Sonnet answers and $0.18 for the Opus judge.
This is an estimate; the bench prints the measured cost.

Results: **not run yet.**

## Known limitations

- **Follow-ups gain nothing from early start:** they need the earlier exchange, so they start
  after the decision.
- An early answer is written from the words as heard. It is kept only when the cleaned question
  adds no content word; a mishearing that the classifier did not repair stays in it.
- The summary is not checked against the transcript: a recognition error can end up in it.
- The intent of a manual question comes from the rules only.
- Only the main answer of a follow-up's earlier question is passed on, not its details.
- The profile is not encrypted at rest.
