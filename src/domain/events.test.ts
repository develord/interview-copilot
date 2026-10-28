import { describe, expect, it } from "vitest";
import { isCardEvent, parsePipelineEvent, type PipelineEvent } from "./events";
import { createId } from "./ids";

const validQuestionEvent = {
  type: "question.detected",
  question: {
    id: "q1",
    sessionId: "s1",
    source: "speech",
    sourceSegmentIds: ["seg1"],
    originalUtterance: "Tu peux m'expliquer Promise.all ?",
    normalizedQuestion: "Tu peux m'expliquer Promise.all ?",
    intent: "TECHNICAL_QUESTION",
    language: "mixed",
    domain: "javascript",
    contextRef: null,
    detectedAt: 1,
  },
};

describe("parsePipelineEvent", () => {
  it("accepts a well-formed question.detected event", () => {
    const result = parsePipelineEvent(validQuestionEvent);
    expect(result.ok).toBe(true);
  });

  it("rejects an unknown intent instead of crashing", () => {
    const result = parsePipelineEvent({
      ...validQuestionEvent,
      question: { ...validQuestionEvent.question, intent: "SMALL_TALK_ISH" },
    });
    expect(result.ok).toBe(false);
  });

  it("rejects an answer.delta without a question id", () => {
    const result = parsePipelineEvent({
      type: "answer.delta",
      requestId: "r1",
      seq: 0,
      section: "primary",
      delta: "x",
    });
    expect(result.ok).toBe(false);
  });

  it("rejects negative or fractional sequence numbers", () => {
    const base = {
      type: "answer.delta",
      questionId: "q1",
      requestId: "r1",
      section: "primary",
      delta: "x",
    };
    expect(parsePipelineEvent({ ...base, seq: -1 }).ok).toBe(false);
    expect(parsePipelineEvent({ ...base, seq: 1.5 }).ok).toBe(false);
    expect(parsePipelineEvent({ ...base, seq: 0 }).ok).toBe(true);
  });

  it("never throws on arbitrary garbage", () => {
    const garbage: unknown[] = [
      null,
      undefined,
      42,
      "answer.delta",
      [],
      {},
      { type: "nope" },
      { type: 1 },
    ];
    for (const input of garbage) {
      expect(() => parsePipelineEvent(input)).not.toThrow();
      expect(parsePipelineEvent(input).ok).toBe(false);
    }
  });

  it("strips unknown extra keys rather than passing them through", () => {
    const result = parsePipelineEvent({
      type: "answer.completed",
      questionId: "q1",
      requestId: "r1",
      injected: "<script>",
    });
    expect(result.ok && "injected" in result.event).toBe(false);
  });
});

describe("isCardEvent", () => {
  it("separates card events from session and transcript events", () => {
    const status: PipelineEvent = { type: "session.status", status: "listening", message: null };
    const completed: PipelineEvent = { type: "answer.completed", questionId: "q", requestId: "r" };
    expect(isCardEvent(status)).toBe(false);
    expect(isCardEvent(completed)).toBe(true);
  });
});

describe("createId", () => {
  it("generates unique prefixed ids", () => {
    const ids = new Set(Array.from({ length: 1000 }, () => createId("q")));
    expect(ids.size).toBe(1000);
    for (const id of ids) expect(id.startsWith("q_")).toBe(true);
  });
});
