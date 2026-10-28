import { beforeEach, describe, expect, it } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia, type Pinia } from "pinia";
import type { UtteranceReport } from "@/domain/intent";
import { useOverlaySettings } from "@/stores/overlaySettings";
import { useSessionStore } from "@/stores/session";
import TranscriptPanel from "./TranscriptPanel.vue";

let pinia: Pinia;

function final(id: string, text: string) {
  return {
    type: "transcript.final",
    segment: {
      id,
      sessionId: "s1",
      sourceId: "system",
      startMs: 0,
      endMs: 0,
      text,
      normalizedText: null,
      isFinal: true,
      confidence: null,
      speakerId: null,
    },
  };
}

function report(segmentIds: string[], overrides: Partial<UtteranceReport>): UtteranceReport {
  return {
    id: `u-${segmentIds.join("-")}`,
    sourceId: "system",
    segmentIds,
    text: "",
    decision: "question",
    reason: null,
    intent: "TECHNICAL_QUESTION",
    stage: "llm",
    questionId: "q1",
    latencyMs: 640,
    ...overrides,
  };
}

describe("TranscriptPanel", () => {
  beforeEach(() => {
    localStorage.clear();
    pinia = createPinia();
    setActivePinia(pinia);
  });

  it("tags the line that ends each utterance with the detection decision", async () => {
    const store = useSessionStore();
    const settings = useOverlaySettings();
    settings.sessionMode = "live";
    settings.transcription.enabled = true;
    const wrapper = mount(TranscriptPanel, { global: { plugins: [pinia] } });

    store.dispatch(final("system-1-0", "Et du coup, sur la partie"));
    store.dispatch(final("system-1-1", "base de données, tu choisirais quoi ?"));
    store.dispatch(final("system-1-2", "On utilise Kafka chez nous."));
    store.dispatch({
      type: "utterance.classified",
      utterance: report(["system-1-0", "system-1-1"], {}),
    });
    store.dispatch({
      type: "utterance.classified",
      utterance: report(["system-1-2"], {
        decision: "ignored",
        reason: "statement",
        intent: "NON_QUESTION_STATEMENT",
        questionId: null,
      }),
    });
    await flushPromises();

    const lines = wrapper.findAll(".line");
    expect(lines.map((l) => l.find(".tag").exists())).toEqual([false, true, true]);
    const question = lines[1]!.find(".tag");
    expect(question.text()).toBe("Question · Technique");
    expect(question.attributes("title")).toBe("Claude, 640 ms");
    expect(lines[2]!.find(".tag").text()).toBe("Ignoré · affirmation");
  });
});
