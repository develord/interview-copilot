import { beforeEach, describe, expect, it } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { describeLatency } from "@/domain/latency";
import { useSessionStore } from "./session";

const segment = {
  id: "system-1-4",
  sessionId: "s1",
  sourceId: "system",
  startMs: 0,
  endMs: 900,
  text: "Comment tu gères le state ?",
  normalizedText: null,
  isFinal: true,
  confidence: 0.9,
  speakerId: null,
};

describe("question latency", () => {
  beforeEach(() => setActivePinia(createPinia()));

  it("splits end of speech → first word into transcription, detection and answer", () => {
    const store = useSessionStore();
    store.dispatch({ type: "transcript.final", segment }, 10_000);
    store.dispatch(
      {
        type: "stt.status",
        source: "system",
        provider: "assemblyai",
        state: "streaming",
        error: null,
        lastFinalLatencyMs: 700,
        droppedChunks: 0,
      },
      10_005,
    );
    store.dispatch(
      {
        type: "question.detected",
        question: {
          id: "q1",
          sessionId: "s1",
          source: "speech",
          sourceSegmentIds: ["system-1-4"],
          originalUtterance: segment.text,
          normalizedQuestion: "Comment gères-tu le state ?",
          intent: "TECHNICAL_QUESTION",
          language: "fr",
          domain: null,
          contextRef: null,
          detectedAt: 0,
        },
      },
      11_200,
    );
    store.dispatch(
      {
        type: "utterance.classified",
        utterance: {
          id: "s1-u3",
          sourceId: "system",
          segmentIds: ["system-1-4"],
          text: segment.text,
          decision: "question",
          reason: null,
          intent: "TECHNICAL_QUESTION",
          stage: "llm",
          questionId: "q1",
          latencyMs: 1_100,
        },
      },
      11_201,
    );
    expect(store.latencyOf("q1")).toBeNull();

    store.dispatch({ type: "answer.started", questionId: "q1", requestId: "q1-a1" }, 11_202);
    store.dispatch(
      {
        type: "answer.delta",
        questionId: "q1",
        requestId: "q1-a1",
        seq: 0,
        section: "primary",
        delta: "J'utilise Pinia.",
      },
      11_260,
    );
    const latency = store.latencyOf("q1")!;
    expect(latency).toEqual({
      transcriptionMs: 700,
      detectionMs: 1_200,
      classifyMs: 1_100,
      answerMs: 60,
      totalMs: 1_960,
    });
    expect(store.medianLatencyMs).toBe(1_960);
    expect(describeLatency(latency)).toBe(
      "Premier mot 2,0 s après la fin de la question : transcription 0,7 s · détection 1,2 s dont Claude 1,1 s · réponse 0,1 s",
    );

    store.clearHistory();
    expect(store.latencyOf("q1")).toBeNull();
    expect(store.medianLatencyMs).toBeNull();
  });
});
