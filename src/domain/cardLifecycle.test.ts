import { describe, expect, it } from "vitest";
import { applyEvent, createCardState, getCardByQuestion, type CardState } from "./cardLifecycle";
import type { CardEvent, DetectedQuestion } from "./events";
import type { AnswerSection } from "./types";

const NOW = 1_000;

function question(id: string, overrides: Partial<DetectedQuestion> = {}): DetectedQuestion {
  return {
    id,
    sessionId: "s1",
    source: "speech",
    sourceSegmentIds: [],
    originalUtterance: `utterance ${id}`,
    normalizedQuestion: `question ${id}`,
    intent: "TECHNICAL_QUESTION",
    language: "en",
    domain: null,
    contextRef: null,
    detectedAt: NOW,
    ...overrides,
  };
}

const detected = (q: DetectedQuestion): CardEvent => ({ type: "question.detected", question: q });
const started = (questionId: string, requestId: string): CardEvent => ({
  type: "answer.started",
  questionId,
  requestId,
});
const delta = (
  questionId: string,
  requestId: string,
  seq: number,
  text: string,
  section: AnswerSection = "primary",
): CardEvent => ({ type: "answer.delta", questionId, requestId, seq, section, delta: text });
const completed = (questionId: string, requestId: string): CardEvent => ({
  type: "answer.completed",
  questionId,
  requestId,
});
const failed = (questionId: string, requestId: string, message = "network"): CardEvent => ({
  type: "answer.failed",
  questionId,
  requestId,
  message,
  retryable: true,
});

function applyAll(state: CardState, events: CardEvent[]) {
  return events.map((event) => applyEvent(state, event, NOW));
}

function card(state: CardState, questionId: string) {
  const found = getCardByQuestion(state, questionId);
  if (!found) throw new Error(`no card for ${questionId}`);
  return found;
}

/** Deterministic PRNG so interleaving tests are reproducible. */
function mulberry32(seed: number) {
  return () => {
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Randomly merges several event streams while preserving each stream's internal order. */
function interleave(streams: CardEvent[][], random: () => number): CardEvent[] {
  const cursors = streams.map(() => 0);
  const merged: CardEvent[] = [];
  for (;;) {
    const live = streams.map((_, i) => i).filter((i) => cursors[i]! < streams[i]!.length);
    if (live.length === 0) return merged;
    const pick = live[Math.floor(random() * live.length)]!;
    merged.push(streams[pick]![cursors[pick]!]!);
    cursors[pick]! += 1;
  }
}

function streamFor(questionId: string, requestId: string, tokens: string[]): CardEvent[] {
  return [
    detected(question(questionId)),
    started(questionId, requestId),
    ...tokens.map((t, i) => delta(questionId, requestId, i, t)),
    completed(questionId, requestId),
  ];
}

describe("card lifecycle — rule 1: new question", () => {
  it("creates a pending card with a unique id for each detected question", () => {
    const state = createCardState();
    applyAll(state, [detected(question("q1")), detected(question("q2"))]);

    expect(state.order).toHaveLength(2);
    const a = card(state, "q1");
    const b = card(state, "q2");
    expect(a.id).not.toBe(b.id);
    expect(a.status).toBe("pending");
    expect(a.question).toBe("question q1");
    expect(a.answer).toBe("");
  });

  it("ignores a duplicate detection of the same question without resetting the card", () => {
    const state = createCardState();
    applyAll(state, [detected(question("q1")), started("q1", "r1"), delta("q1", "r1", 0, "Hi")]);

    const outcome = applyEvent(state, detected(question("q1")), NOW);

    expect(outcome).toEqual({ applied: false, reason: "duplicate-question" });
    expect(state.order).toHaveLength(1);
    expect(card(state, "q1").answer).toBe("Hi");
    expect(card(state, "q1").status).toBe("streaming");
  });

  it("never mutates a previous card when a new question arrives", () => {
    const state = createCardState();
    applyAll(state, streamFor("q1", "r1", ["Answer ", "one."]));
    const before = structuredClone(card(state, "q1"));

    applyAll(state, streamFor("q2", "r2", ["Answer ", "two."]));

    expect(card(state, "q1")).toEqual(before);
  });
});

describe("card lifecycle — rules 2 & 3: streaming and completion", () => {
  it("appends deltas to the card of the same question and completes it", () => {
    const state = createCardState();
    applyAll(state, streamFor("q1", "r1", ["Promise.all ", "rejects ", "fast."]));

    const a = card(state, "q1");
    expect(a.answer).toBe("Promise.all rejects fast.");
    expect(a.status).toBe("completed");
    expect(a.completedAt).toBe(NOW);
    expect(a.attempt).toBe(1);
  });

  it("routes details deltas to the details section", () => {
    const state = createCardState();
    applyAll(state, [
      detected(question("q1")),
      started("q1", "r1"),
      delta("q1", "r1", 0, "Use a hash map."),
      delta("q1", "r1", 1, "```ts\nconst seen = new Map();\n```", "details"),
      completed("q1", "r1"),
    ]);

    expect(card(state, "q1").answer).toBe("Use a hash map.");
    expect(card(state, "q1").details).toContain("new Map()");
  });

  it("ignores late deltas after completion and leaves the answer intact", () => {
    const state = createCardState();
    applyAll(state, streamFor("q1", "r1", ["Done."]));

    const outcome = applyEvent(state, delta("q1", "r1", 1, " extra"), NOW);

    expect(outcome).toEqual({ applied: false, reason: "already-terminal" });
    expect(card(state, "q1").answer).toBe("Done.");
  });

  it("does not let a new generation overwrite a completed answer", () => {
    const state = createCardState();
    applyAll(state, streamFor("q1", "r1", ["Original."]));

    const outcome = applyEvent(state, started("q1", "r9"), NOW);

    expect(outcome).toEqual({ applied: false, reason: "already-terminal" });
    expect(card(state, "q1").answer).toBe("Original.");
  });

  it("drops duplicate deltas and reorders out-of-order deltas by sequence number", () => {
    const state = createCardState();
    applyAll(state, [detected(question("q1")), started("q1", "r1")]);

    const outcomes = applyAll(state, [
      delta("q1", "r1", 0, "a"),
      delta("q1", "r1", 2, "c"),
      delta("q1", "r1", 0, "a"),
      delta("q1", "r1", 1, "b"),
      delta("q1", "r1", 2, "c"),
    ]);

    expect(card(state, "q1").answer).toBe("abc");
    expect(outcomes.map((o) => (o.applied ? "ok" : o.reason))).toEqual([
      "ok",
      "ok",
      "duplicate-delta",
      "ok",
      "duplicate-delta",
    ]);
  });

  it("flushes buffered deltas in order when completion arrives before a gap is filled", () => {
    const state = createCardState();
    applyAll(state, [
      detected(question("q1")),
      started("q1", "r1"),
      delta("q1", "r1", 0, "a"),
      delta("q1", "r1", 3, "d"),
      delta("q1", "r1", 2, "c"),
      completed("q1", "r1"),
    ]);

    expect(card(state, "q1").answer).toBe("acd");
    expect(card(state, "q1").status).toBe("completed");
  });

  it("treats a delta that arrives before answer.started as an implicit start", () => {
    const state = createCardState();
    applyAll(state, [detected(question("q1")), delta("q1", "r1", 0, "early ")]);

    expect(card(state, "q1").status).toBe("streaming");
    const outcome = applyEvent(state, started("q1", "r1"), NOW);
    expect(outcome).toEqual({ applied: false, reason: "duplicate-start" });

    applyAll(state, [delta("q1", "r1", 1, "bird"), completed("q1", "r1")]);
    expect(card(state, "q1").answer).toBe("early bird");
  });

  it("ignores events for unknown questions", () => {
    const state = createCardState();
    expect(applyEvent(state, delta("ghost", "r1", 0, "x"), NOW)).toEqual({
      applied: false,
      reason: "unknown-question",
    });
    expect(state.order).toHaveLength(0);
  });
});

describe("card lifecycle — rule 4: transcript revisions", () => {
  it("updates the question text while the card is still pending", () => {
    const state = createCardState();
    applyAll(state, [
      detected(question("q1", { normalizedQuestion: "C'est quoi la différence entre SSR et" })),
      {
        type: "question.revised",
        questionId: "q1",
        text: "C'est quoi la différence entre SSR et CSR ?",
      },
    ]);

    expect(card(state, "q1").question).toBe("C'est quoi la différence entre SSR et CSR ?");
    expect(card(state, "q1").questionRevisions).toEqual([]);
  });

  it("records a correction instead of overwriting once answering has started", () => {
    const state = createCardState();
    applyAll(state, [
      detected(question("q1", { normalizedQuestion: "What is a closure?" })),
      started("q1", "r1"),
      delta("q1", "r1", 0, "A closure is…"),
      { type: "question.revised", questionId: "q1", text: "What is a closure in Rust?" },
    ]);

    const a = card(state, "q1");
    expect(a.question).toBe("What is a closure?");
    expect(a.questionRevisions).toEqual(["What is a closure in Rust?"]);
    expect(a.answer).toBe("A closure is…");
  });
});

describe("card lifecycle — rule 5: follow-ups", () => {
  it("creates a separate card linked to the earlier question", () => {
    const state = createCardState();
    applyAll(state, streamFor("q1", "r1", ["Lazy-load routes."]));
    const before = structuredClone(card(state, "q1"));

    applyAll(state, [
      detected(question("q2", { normalizedQuestion: "Pourquoi ?", contextRef: "q1" })),
    ]);

    expect(state.order).toHaveLength(2);
    expect(card(state, "q2").contextRef).toBe("q1");
    expect(card(state, "q1")).toEqual(before);
  });
});

describe("card lifecycle — rule 6: concurrent generations", () => {
  it("keeps late tokens for answer A on card A after answer B has started", () => {
    const state = createCardState();
    applyAll(state, [
      detected(question("qA")),
      started("qA", "rA"),
      delta("qA", "rA", 0, "A1 "),
      detected(question("qB")),
      started("qB", "rB"),
      delta("qB", "rB", 0, "B1 "),
      delta("qA", "rA", 1, "A2 "),
      delta("qB", "rB", 1, "B2 "),
      delta("qA", "rA", 2, "A3"),
      completed("qB", "rB"),
      completed("qA", "rA"),
    ]);

    expect(card(state, "qA").answer).toBe("A1 A2 A3");
    expect(card(state, "qB").answer).toBe("B1 B2 ");
    expect(card(state, "qA").status).toBe("completed");
    expect(card(state, "qB").status).toBe("completed");
  });

  it("produces exactly the expected answers for any interleaving of three streams", () => {
    const tokens = {
      qA: ["Closures ", "capture ", "their ", "lexical ", "scope."],
      qB: ["Use ", "Redis ", "for ", "caching."],
      qC: ["Bonjour, ", "très ", "bien ", "merci !"],
    };
    const random = mulberry32(42);

    for (let run = 0; run < 200; run++) {
      const state = createCardState();
      const events = interleave(
        Object.entries(tokens).map(([qid, toks]) => streamFor(qid, `r-${qid}`, toks)),
        random,
      );
      const outcomes = applyAll(state, events);

      expect(outcomes.every((o) => o.applied)).toBe(true);
      for (const [qid, toks] of Object.entries(tokens)) {
        expect(card(state, qid).answer).toBe(toks.join(""));
        expect(card(state, qid).status).toBe("completed");
      }
    }
  });

  it("drops events from a superseded request instead of mixing them into the new answer", () => {
    const state = createCardState();
    applyAll(state, [
      detected(question("q1")),
      started("q1", "old"),
      delta("q1", "old", 0, "stale "),
    ]);

    applyAll(state, [started("q1", "new"), delta("q1", "new", 0, "fresh")]);
    const outcome = applyEvent(state, delta("q1", "old", 1, "LATE"), NOW);

    expect(outcome).toEqual({ applied: false, reason: "stale-request" });
    expect(card(state, "q1").answer).toBe("fresh");
    expect(card(state, "q1").attempt).toBe(2);
  });

  it("ignores events whose request id was never started for the card", () => {
    const state = createCardState();
    applyAll(state, [detected(question("q1")), started("q1", "r1")]);

    expect(applyEvent(state, delta("q1", "intruder", 0, "x"), NOW)).toEqual({
      applied: false,
      reason: "stale-request",
    });
    expect(applyEvent(state, completed("q1", "intruder"), NOW)).toEqual({
      applied: false,
      reason: "stale-request",
    });
    expect(card(state, "q1").status).toBe("streaming");
  });
});

describe("card lifecycle — rule 7: failures and retry", () => {
  it("keeps the card and its partial answer visible when generation fails", () => {
    const state = createCardState();
    applyAll(state, [
      detected(question("q1")),
      started("q1", "r1"),
      delta("q1", "r1", 0, "Partial"),
      failed("q1", "r1", "Network interrupted"),
    ]);

    const a = card(state, "q1");
    expect(a.status).toBe("failed");
    expect(a.answer).toBe("Partial");
    expect(a.error).toEqual({ message: "Network interrupted", retryable: true });
    expect(state.order).toHaveLength(1);
  });

  it("restarts cleanly on retry and rejects late tokens from the failed attempt", () => {
    const state = createCardState();
    applyAll(state, [
      detected(question("q1")),
      started("q1", "r1"),
      delta("q1", "r1", 0, "Partial"),
      failed("q1", "r1"),
      started("q1", "r2"),
    ]);

    expect(card(state, "q1").status).toBe("streaming");
    expect(card(state, "q1").answer).toBe("");
    expect(card(state, "q1").error).toBeNull();
    expect(card(state, "q1").attempt).toBe(2);

    expect(applyEvent(state, delta("q1", "r1", 1, "LATE"), NOW)).toEqual({
      applied: false,
      reason: "stale-request",
    });
    applyAll(state, [delta("q1", "r2", 0, "Complete answer."), completed("q1", "r2")]);
    expect(card(state, "q1").answer).toBe("Complete answer.");
    expect(card(state, "q1").status).toBe("completed");
  });

  it("does not let a late answer.started from the failed attempt hijack the retry", () => {
    const state = createCardState();
    applyAll(state, [
      detected(question("q1")),
      started("q1", "r1"),
      failed("q1", "r1"),
      started("q1", "r2"),
      delta("q1", "r2", 0, "Retry answer"),
    ]);

    expect(applyEvent(state, started("q1", "r1"), NOW)).toEqual({
      applied: false,
      reason: "stale-request",
    });
    expect(card(state, "q1").answer).toBe("Retry answer");
    expect(card(state, "q1").attempt).toBe(2);
  });

  it("does not let a failure event flip a completed card back to failed", () => {
    const state = createCardState();
    applyAll(state, streamFor("q1", "r1", ["Done."]));

    expect(applyEvent(state, failed("q1", "r1"), NOW)).toEqual({
      applied: false,
      reason: "already-terminal",
    });
    expect(card(state, "q1").status).toBe("completed");
  });
});
