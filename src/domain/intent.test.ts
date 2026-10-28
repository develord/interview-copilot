import { describe, expect, it } from "vitest";
import classifierStatusFixture from "../../contracts/fixtures/classifier-status.json";
import questionDetectedFixture from "../../contracts/fixtures/question-detected.json";
import questionRevisedFixture from "../../contracts/fixtures/question-revised.json";
import utteranceClassifiedFixture from "../../contracts/fixtures/utterance-classified.json";
import { parsePipelineEvent } from "./events";
import { asClassifyError } from "./intent";

// The same fixtures are asserted against Rust's serde output in src-tauri/src/events.rs.
describe("question-detection contracts shared with Rust", () => {
  it.each([
    ["question.detected", questionDetectedFixture],
    ["question.revised", questionRevisedFixture],
    ["utterance.classified", utteranceClassifiedFixture],
    ["classifier.status", classifierStatusFixture],
  ])("accepts the %s fixture", (type, fixture) => {
    const parsed = parsePipelineEvent(fixture);
    expect(parsed.ok).toBe(true);
    expect(parsed.ok && parsed.event.type).toBe(type);
  });

  it("rejects unknown decisions, models and error codes", () => {
    const bad = [
      {
        ...utteranceClassifiedFixture,
        utterance: { ...utteranceClassifiedFixture.utterance, decision: "maybe" },
      },
      { ...classifierStatusFixture, model: "gpt-5" },
      { ...classifierStatusFixture, error: { code: "teapot", detail: "" } },
    ];
    for (const event of bad) expect(parsePipelineEvent(event).ok).toBe(false);
  });

  it("narrows command rejections to classify errors", () => {
    expect(asClassifyError({ code: "unauthorized", detail: "bad key" })).toEqual({
      code: "unauthorized",
      detail: "bad key",
    });
    expect(asClassifyError(new Error("boom"))).toEqual({ code: "protocol", detail: "boom" });
  });
});
