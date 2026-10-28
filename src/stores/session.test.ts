import { beforeEach, describe, expect, it } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useSessionStore } from "./session";
import type { DetectedQuestion } from "@/domain/events";

function question(id: string, text: string): DetectedQuestion {
  return {
    id,
    sessionId: "s1",
    source: "speech",
    sourceSegmentIds: [],
    originalUtterance: text,
    normalizedQuestion: text,
    intent: "TECHNICAL_QUESTION",
    language: "fr",
    domain: null,
    contextRef: null,
    detectedAt: 0,
  };
}

describe("session store", () => {
  beforeEach(() => setActivePinia(createPinia()));

  it("counts malformed events as invalid without changing state or throwing", () => {
    const store = useSessionStore();
    expect(() => store.dispatch({ type: "answer.delta", questionId: 42 })).not.toThrow();
    expect(() => store.dispatch("garbage")).not.toThrow();

    expect(store.diagnostics.invalid).toBe(2);
    expect(store.diagnostics.lastInvalid).toBeTruthy();
    expect(store.cards).toHaveLength(0);
  });

  it("keeps the active question separate from the history cards", () => {
    const store = useSessionStore();
    store.dispatch({ type: "question.detected", question: question("q1", "Question A") });
    store.dispatch({ type: "question.detected", question: question("q2", "Question B") });

    expect(store.active?.questionId).toBe("q2");
    expect(store.active?.question).toBe("Question B");
    expect(store.findCard("q1")?.question).toBe("Question A");
    // Not the same object: mutating the active panel can never leak into a card.
    expect(store.active).not.toBe(store.findCard("q2"));
  });

  it("follows pending transcript revisions in the active panel", () => {
    const store = useSessionStore();
    store.dispatch({ type: "question.detected", question: question("q1", "SSR et") });
    store.dispatch({ type: "question.revised", questionId: "q1", text: "SSR et CSR ?" });

    expect(store.active?.question).toBe("SSR et CSR ?");
    expect(store.findCard("q1")?.question).toBe("SSR et CSR ?");
  });

  it("records ignored events with their reason", () => {
    const store = useSessionStore();
    store.dispatch({ type: "answer.completed", questionId: "ghost", requestId: "r" });

    expect(store.diagnostics.ignored).toBe(1);
    expect(store.diagnostics.lastIgnored).toEqual({
      type: "answer.completed",
      reason: "unknown-question",
    });
  });

  it("applies session status events and clears history on demand", () => {
    const store = useSessionStore();
    store.dispatch({ type: "session.status", status: "listening", message: null });
    store.dispatch({ type: "question.detected", question: question("q1", "A") });

    expect(store.status).toBe("listening");
    store.clearHistory();
    expect(store.cards).toHaveLength(0);
    expect(store.active).toBeNull();
  });

  it("returns the applied event, and null for malformed or ignored ones", () => {
    const store = useSessionStore();
    const detected = { type: "question.detected", question: question("q1", "A") };
    expect(store.dispatch(detected)?.type).toBe("question.detected");
    // Replayed: the lifecycle ignores it, so callers must not answer it twice.
    expect(store.dispatch(detected)).toBeNull();
    expect(store.dispatch("garbage")).toBeNull();
  });

  it("keeps the classifier status and each utterance decision until history is cleared", () => {
    const store = useSessionStore();
    store.dispatch({
      type: "classifier.status",
      state: "rules-only",
      model: null,
      error: { code: "missing-key", detail: "no key" },
      lastLatencyMs: null,
      classified: 0,
      failed: 0,
    });
    store.dispatch({
      type: "utterance.classified",
      utterance: {
        id: "s1-u1",
        sourceId: "system",
        segmentIds: ["system-1-0", "system-1-1"],
        text: "Et du coup, sur la partie base de données ?",
        decision: "question",
        reason: null,
        intent: "TECHNICAL_QUESTION",
        stage: "fallback",
        questionId: "s1-q1",
        latencyMs: null,
      },
    });

    expect(store.classifierStatus?.state).toBe("rules-only");
    // Keyed by the last segment, where the utterance ends in the transcript.
    expect(store.utteranceBySegment["system-1-1"]?.questionId).toBe("s1-q1");
    expect(store.utteranceBySegment["system-1-0"]).toBeUndefined();

    store.clearHistory();
    expect(store.utteranceBySegment).toEqual({});
    expect(store.classifierStatus?.state).toBe("rules-only");
  });
});
