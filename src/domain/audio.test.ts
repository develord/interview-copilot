import { describe, expect, it } from "vitest";
import audioDiagnosticsFixture from "../../contracts/fixtures/audio-diagnostics.json";
import sessionStatusFixture from "../../contracts/fixtures/session-status.json";
import { asAudioError, meterPercent } from "./audio";
import { parsePipelineEvent } from "./events";

// The same fixtures are asserted against Rust's serde output in src-tauri/src/events.rs.
describe("Rust ↔ TypeScript event contract", () => {
  it("accepts the audio.diagnostics event emitted by the Rust engine", () => {
    const result = parsePipelineEvent(audioDiagnosticsFixture);
    expect(result.ok).toBe(true);
    if (result.ok && result.event.type === "audio.diagnostics") {
      expect(result.event.sources.map((s) => s.source)).toEqual(["system", "microphone"]);
      expect(result.event.sources[1]!.error?.code).toBe("permission-denied");
    }
  });

  it("accepts the session.status event emitted by the Rust engine", () => {
    expect(parsePipelineEvent(sessionStatusFixture).ok).toBe(true);
  });

  it("rejects diagnostics with an unknown state or error code", () => {
    const source = audioDiagnosticsFixture.sources[0]!;
    const badState = { ...audioDiagnosticsFixture, sources: [{ ...source, state: "exploded" }] };
    const badCode = {
      ...audioDiagnosticsFixture,
      sources: [{ ...source, error: { code: "gremlins", detail: "" } }],
    };
    expect(parsePipelineEvent(badState).ok).toBe(false);
    expect(parsePipelineEvent(badCode).ok).toBe(false);
  });
});

describe("meterPercent", () => {
  it("maps -60…0 dBFS onto 0…100 and clamps outside it", () => {
    expect(meterPercent(-100)).toBe(0);
    expect(meterPercent(-60)).toBe(0);
    expect(meterPercent(-30)).toBe(50);
    expect(meterPercent(0)).toBe(100);
    expect(meterPercent(6)).toBe(100);
    expect(meterPercent(Number.NaN)).toBe(0);
  });
});

describe("asAudioError", () => {
  it("keeps a structured backend error", () => {
    expect(asAudioError({ code: "device-busy", detail: "in use" })).toEqual({
      code: "device-busy",
      detail: "in use",
    });
  });

  it("wraps anything else as a backend error", () => {
    expect(asAudioError(new Error("ipc down"))).toEqual({ code: "backend", detail: "ipc down" });
    expect(asAudioError("plain")).toEqual({ code: "backend", detail: "plain" });
  });
});
