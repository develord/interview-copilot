import { beforeEach, describe, expect, it } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import sttStatusFixture from "../../contracts/fixtures/stt-status.json";
import transcriptFinalFixture from "../../contracts/fixtures/transcript-final.json";
import { audioConfig, FAST_ENDPOINT_MS } from "@/services/createSession";
import { useOverlaySettings } from "@/stores/overlaySettings";
import { TRANSCRIPT_LIMIT, useSessionStore } from "@/stores/session";
import { parsePipelineEvent } from "./events";
import { parseTerms } from "./stt";

// The same fixtures are asserted against Rust's serde output in src-tauri/src/events.rs.
describe("Rust ↔ TypeScript transcription contract", () => {
  it("accepts transcript.final and stt.status exactly as Rust emits them", () => {
    expect(parsePipelineEvent(transcriptFinalFixture).ok).toBe(true);
    expect(parsePipelineEvent(sttStatusFixture).ok).toBe(true);
  });

  it("rejects an unknown STT state or provider", () => {
    expect(parsePipelineEvent({ ...sttStatusFixture, state: "dreaming" }).ok).toBe(false);
    expect(parsePipelineEvent({ ...sttStatusFixture, provider: "whisper" }).ok).toBe(false);
  });
});

describe("parseTerms", () => {
  it("splits on commas, semicolons and newlines, trims and deduplicates", () => {
    expect(parseTerms(" Kafka, Spring Boot ;kafka\nDatadog,, ")).toEqual([
      "Kafka",
      "Spring Boot",
      "Datadog",
    ]);
  });

  it("drops over-long entries and caps the list", () => {
    expect(parseTerms("x".repeat(51))).toEqual([]);
    expect(parseTerms(Array.from({ length: 150 }, (_, i) => `t${i}`).join(","))).toHaveLength(100);
  });
});

describe("live transcript in the session store", () => {
  beforeEach(() => setActivePinia(createPinia()));

  const segment = (id: string, text: string, isFinal: boolean) => ({
    ...transcriptFinalFixture.segment,
    id,
    text,
    isFinal,
  });

  it("updates a line in place from partial to final (no duplicate lines)", () => {
    const store = useSessionStore();
    store.dispatch({
      type: "transcript.partial",
      segment: segment("system-1-0", "Comment tu", false),
    });
    store.dispatch({
      type: "transcript.partial",
      segment: segment("system-1-0", "Comment tu gères", false),
    });
    store.dispatch({
      type: "transcript.final",
      segment: segment("system-1-0", "Comment tu gères ?", true),
    });

    expect(store.transcriptLines).toHaveLength(1);
    expect(store.transcriptLines[0]).toMatchObject({ text: "Comment tu gères ?", isFinal: true });
  });

  it("never lets a late partial reopen a final line", () => {
    const store = useSessionStore();
    store.dispatch({ type: "transcript.final", segment: segment("system-1-0", "Final.", true) });
    store.dispatch({ type: "transcript.partial", segment: segment("system-1-0", "Fin", false) });
    expect(store.transcriptLines[0]).toMatchObject({ text: "Final.", isFinal: true });
  });

  it("keeps only the most recent lines", () => {
    const store = useSessionStore();
    for (let i = 0; i < TRANSCRIPT_LIMIT + 5; i++) {
      store.dispatch({
        type: "transcript.final",
        segment: segment(`system-1-${i}`, `line ${i}`, true),
      });
    }
    expect(store.transcriptLines).toHaveLength(TRANSCRIPT_LIMIT);
    expect(store.transcriptLines[0]!.text).toBe("line 5");
  });

  it("tracks the STT status per source", () => {
    const store = useSessionStore();
    store.dispatch(sttStatusFixture);
    expect(store.sttStatus.microphone).toMatchObject({
      state: "reconnecting",
      lastFinalLatencyMs: 420,
    });
    expect(store.sttStatus.system).toBeUndefined();
  });
});

describe("audioConfig (what live start sends to Rust)", () => {
  beforeEach(() => {
    localStorage.clear();
    setActivePinia(createPinia());
  });

  it("captures without transcription until it is enabled", () => {
    expect(audioConfig(useOverlaySettings()).transcription).toBeNull();
  });

  it("sends the provider, parsed terms and the fast endpoint when enabled", () => {
    const settings = useOverlaySettings();
    settings.transcription.enabled = true;
    settings.transcription.provider = "soniox";
    settings.transcription.extraTerms = "Kafka, Datadog";
    expect(audioConfig(settings).transcription).toEqual({
      provider: "soniox",
      extraTerms: ["Kafka", "Datadog"],
      forceEndpointMs: FAST_ENDPOINT_MS,
      builtinVocabulary: true,
    });

    settings.transcription.fastEndpoint = false;
    expect(audioConfig(settings).transcription?.forceEndpointMs).toBeNull();
  });
});
