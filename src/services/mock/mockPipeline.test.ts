import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useSessionStore } from "@/stores/session";
import type { PipelineEvent } from "@/domain/events";
import { DEMO_SCENARIO } from "./demoScenario";
import { MockPipeline, tokenize } from "./mockPipeline";

function setup() {
  const store = useSessionStore();
  const log: PipelineEvent[] = [];
  const pipeline = new MockPipeline({
    emit: (event) => {
      log.push(event);
      store.dispatch(event);
    },
    tokenDelayMs: 40,
  });
  return { store, pipeline, log };
}

describe("mock pipeline + session store (end-to-end with fake timers)", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("plays the demo: one card per question, correctly associated, nothing ignored", async () => {
    const { store, pipeline } = setup();
    pipeline.start();
    await vi.runAllTimersAsync();

    expect(store.cards).toHaveLength(DEMO_SCENARIO.length);
    DEMO_SCENARIO.forEach((step, i) => {
      const card = store.cards[i]!;
      expect(card.question).toBe(step.utterance);
      expect(card.intent).toBe(step.intent);
      if (step.answer.failFirstAttemptAfterTokens === undefined) {
        expect(card.status).toBe("completed");
        expect(card.answer).toBe(step.answer.primary);
        expect(card.details).toBe(step.answer.details ?? "");
      }
    });
    expect(store.diagnostics).toMatchObject({ invalid: 0, ignored: 0 });
  });

  it("streams two answers concurrently with interleaved deltas", async () => {
    const { store, pipeline, log } = setup();
    pipeline.start();
    await vi.runAllTimersAsync();

    const intro = store.cards.find((c) => c.question.startsWith("Can you introduce"))!;
    const perf = store.cards.find((c) => c.question.startsWith("Et comment tu gères"))!;
    const deltaOwners = log
      .filter((e) => e.type === "answer.delta")
      .map((e) => (e.type === "answer.delta" ? e.questionId : ""))
      .filter((id) => id === intro.questionId || id === perf.questionId);

    const firstPerf = deltaOwners.indexOf(perf.questionId);
    const lastIntro = deltaOwners.lastIndexOf(intro.questionId);
    expect(firstPerf).toBeGreaterThan(-1);
    expect(firstPerf).toBeLessThan(lastIntro); // genuinely overlapping streams
    expect(intro.answer).toBe(DEMO_SCENARIO[1]!.answer.primary);
    expect(perf.answer).toBe(DEMO_SCENARIO[2]!.answer.primary);
  });

  it("links the follow-up to the question it refers to", async () => {
    const { store, pipeline } = setup();
    pipeline.start();
    await vi.runAllTimersAsync();

    const perf = store.cards.find((c) => c.question.startsWith("Et comment tu gères"))!;
    const why = store.cards.find((c) => c.question === "Pourquoi ?")!;
    expect(why.contextRef).toBe(perf.questionId);
  });

  it("keeps the failed card visible and recovers it on retry without touching other cards", async () => {
    const { store, pipeline } = setup();
    pipeline.start();
    await vi.runAllTimersAsync();

    const failed = store.cards.find((c) => c.intent === "SYSTEM_DESIGN")!;
    expect(failed.status).toBe("failed");
    expect(failed.error?.retryable).toBe(true);
    expect(failed.answer.length).toBeGreaterThan(0);
    // JSON snapshot: structuredClone cannot clone Vue reactive proxies.
    const others = JSON.parse(JSON.stringify(store.cards.filter((c) => c.id !== failed.id)));

    pipeline.retry(failed.questionId);
    await vi.runAllTimersAsync();

    expect(failed.status).toBe("completed");
    expect(failed.attempt).toBe(2);
    expect(failed.answer).toBe(DEMO_SCENARIO.at(-1)!.answer.primary);
    expect(store.cards.filter((c) => c.id !== failed.id)).toEqual(others);
  });

  it("creates no cards while paused and continues after resume", async () => {
    const { store, pipeline } = setup();
    pipeline.start();
    pipeline.pause();
    await vi.advanceTimersByTimeAsync(10_000);

    expect(store.status).toBe("paused");
    expect(store.cards).toHaveLength(0);

    pipeline.resume();
    await vi.advanceTimersByTimeAsync(100);
    expect(store.cards).toHaveLength(1);
  });

  it("stop ends detection but lets in-flight answers finish", async () => {
    const { store, pipeline } = setup();
    pipeline.start();
    await vi.advanceTimersByTimeAsync(1_200); // greeting detected and streaming
    pipeline.stop();
    await vi.runAllTimersAsync();

    expect(store.status).toBe("idle");
    expect(store.cards).toHaveLength(1);
    expect(store.cards[0]!.status).toBe("completed");
  });

  it("handles a manual question while the scripted session is running", async () => {
    const { store, pipeline } = setup();
    pipeline.start();
    await vi.advanceTimersByTimeAsync(4_000);
    pipeline.askManual("  Comment tu gères les erreurs dans une API Node.js ?  ");
    await vi.runAllTimersAsync();

    const manual = store.cards.find((c) => c.source === "manual")!;
    expect(manual.question).toBe("Comment tu gères les erreurs dans une API Node.js ?");
    expect(manual.status).toBe("completed");
    expect(store.cards).toHaveLength(DEMO_SCENARIO.length + 1);
  });

  it("ignores empty manual input", () => {
    const { store, pipeline } = setup();
    pipeline.askManual("   ");
    expect(store.cards).toHaveLength(0);
  });

  it("cancelAll stops in-flight generations so nothing reaches a cleared history", async () => {
    const { store, pipeline, log } = setup();
    pipeline.start();
    await vi.advanceTimersByTimeAsync(1_200);
    pipeline.stop();
    pipeline.cancelAll();
    store.clearHistory();
    const eventsAtClear = log.length;
    await vi.runAllTimersAsync();

    expect(store.cards).toHaveLength(0);
    expect(log.slice(eventsAtClear).filter((e) => e.type.startsWith("answer."))).toEqual([]);
  });
});

describe("tokenize", () => {
  it("is lossless", () => {
    const text = "Hello  world,\n```ts\nconst a = 1;\n```\n";
    expect(tokenize(text).join("")).toBe(text);
  });
});
