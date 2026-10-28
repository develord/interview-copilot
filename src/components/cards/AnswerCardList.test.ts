import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia, type Pinia } from "pinia";
import type { ComponentPublicInstance } from "vue";
import type { AnswerCard } from "@/domain/cardLifecycle";
import type { DetectedQuestion } from "@/domain/events";
import { SESSION_CONTROLLER, type SessionController } from "@/services/sessionController";
import { useOverlaySettings } from "@/stores/overlaySettings";
import { useSessionStore } from "@/stores/session";
import AnswerCardList from "./AnswerCardList.vue";

function question(id: string, text: string, contextRef: string | null = null): DetectedQuestion {
  return {
    id,
    sessionId: "s1",
    source: "speech",
    sourceSegmentIds: [],
    originalUtterance: text,
    normalizedQuestion: text,
    intent: "TECHNICAL_QUESTION",
    language: "en",
    domain: null,
    contextRef,
    detectedAt: 0,
  };
}

/** Counts re-renders per card component instance, keyed by card id. */
const renders = new Map<string, number>();
const renderCounter = {
  updated(this: ComponentPublicInstance) {
    const card = (this.$props as { card?: AnswerCard }).card;
    if (card && this.$options.name !== "RichText") {
      renders.set(card.id, (renders.get(card.id) ?? 0) + 1);
    }
  },
};

let pinia: Pinia;
let controller: SessionController;

function mountList() {
  return mount(AnswerCardList, {
    global: {
      plugins: [pinia],
      provide: { [SESSION_CONTROLLER as symbol]: controller },
      mixins: [renderCounter],
    },
  });
}

describe("AnswerCardList", () => {
  beforeEach(() => {
    localStorage.clear();
    pinia = createPinia();
    setActivePinia(pinia);
    renders.clear();
    controller = {
      start: vi.fn(),
      pause: vi.fn(),
      resume: vi.fn(),
      stop: vi.fn(),
      askManual: vi.fn(),
      retry: vi.fn(),
      cancelAll: vi.fn(),
      dispose: vi.fn(),
    };
  });

  it("streams into card A without re-rendering or replacing card B", async () => {
    const store = useSessionStore();
    const wrapper = mountList();
    store.dispatch({ type: "question.detected", question: question("qA", "Question A") });
    store.dispatch({ type: "question.detected", question: question("qB", "Question B") });
    store.dispatch({ type: "answer.started", questionId: "qA", requestId: "rA" });
    store.dispatch({ type: "answer.started", questionId: "qB", requestId: "rB" });
    store.dispatch({
      type: "answer.delta",
      questionId: "qB",
      requestId: "rB",
      seq: 0,
      section: "primary",
      delta: "Answer B.",
    });
    await flushPromises();

    const cardB = wrapper.get('[data-card-id="card:qB"]').element;
    const cardBText = cardB.textContent;
    renders.clear();

    for (const [seq, delta] of ["Late ", "tokens ", "for ", "A."].entries()) {
      store.dispatch({
        type: "answer.delta",
        questionId: "qA",
        requestId: "rA",
        seq,
        section: "primary",
        delta,
      });
      await flushPromises();
    }

    expect(wrapper.get('[data-card-id="card:qA"]').text()).toContain("Late tokens for A.");
    expect(renders.get("card:qA")).toBeGreaterThan(0);
    expect(renders.get("card:qB")).toBeUndefined();
    expect(wrapper.get('[data-card-id="card:qB"]').element).toBe(cardB);
    expect(cardB.textContent).toBe(cardBText);
  });

  it("keeps existing card DOM nodes when a new card arrives, in either display order", async () => {
    const store = useSessionStore();
    const settings = useOverlaySettings();
    const wrapper = mountList();
    store.dispatch({ type: "question.detected", question: question("q1", "First") });
    await flushPromises();
    const first = wrapper.get('[data-card-id="card:q1"]').element;
    renders.clear();

    store.dispatch({ type: "question.detected", question: question("q2", "Second") });
    await flushPromises();
    expect(wrapper.get('[data-card-id="card:q1"]').element).toBe(first);
    expect(renders.get("card:q1")).toBeUndefined();

    const order = () => wrapper.findAll("[data-card-id]").map((w) => w.attributes("data-card-id"));
    expect(order()).toEqual(["card:q1", "card:q2"]);
    settings.order = "newest-first";
    await flushPromises();
    expect(order()).toEqual(["card:q2", "card:q1"]);
    expect(wrapper.get('[data-card-id="card:q1"]').element).toBe(first);
  });

  it("shows the follow-up link and offers retry on a failed card", async () => {
    const store = useSessionStore();
    const wrapper = mountList();
    store.dispatch({ type: "question.detected", question: question("q1", "How do you cache?") });
    store.dispatch({ type: "question.detected", question: question("q2", "Why?", "q1") });
    store.dispatch({ type: "answer.started", questionId: "q2", requestId: "r2" });
    store.dispatch({
      type: "answer.failed",
      questionId: "q2",
      requestId: "r2",
      message: "Network interrupted",
      retryable: true,
    });
    await flushPromises();

    const followUp = wrapper.get('[data-card-id="card:q2"]');
    expect(followUp.text()).toContain("Suite de : How do you cache?");
    expect(followUp.text()).toContain("Network interrupted");

    await followUp.get("button.retry").trigger("click");
    expect(controller.retry).toHaveBeenCalledWith("q2");
  });

  it("renders answer text literally, never as HTML", async () => {
    const store = useSessionStore();
    const wrapper = mountList();
    store.dispatch({ type: "question.detected", question: question("q1", "XSS?") });
    store.dispatch({ type: "answer.started", questionId: "q1", requestId: "r1" });
    store.dispatch({
      type: "answer.delta",
      questionId: "q1",
      requestId: "r1",
      seq: 0,
      section: "primary",
      delta: '<img src=x onerror="alert(1)">',
    });
    await flushPromises();

    const card = wrapper.get('[data-card-id="card:q1"]');
    expect(card.find("img").exists()).toBe(false);
    expect(card.text()).toContain('<img src=x onerror="alert(1)">');
  });
});
