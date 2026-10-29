import type { DetectedQuestion, PipelineEvent } from "@/domain/events";
import { createId } from "@/domain/ids";
import type { Intent, Language, SessionStatus } from "@/domain/types";
import type { SessionController } from "@/services/sessionController";
import { DEMO_SCENARIO, type AnswerScript, type ScenarioStep } from "./demoScenario";

export interface MockPipelineOptions {
  emit: (event: PipelineEvent) => void;
  scenario?: ScenarioStep[];
  /** Delay between streamed tokens. */
  tokenDelayMs?: number;
  now?: () => number;
}

class AbortedError extends Error {}

/**
 * Phase 1 stand-in for the real pipeline (audio → STT → classifier → LLM). It plays a scripted
 * conversation and streams answers token by token, emitting exactly the events the real
 * orchestrator will emit. Each generation has its own request id and AbortController, so
 * cancellation and retries behave like the production path.
 */
export class MockPipeline implements SessionController {
  private readonly emit: (event: PipelineEvent) => void;
  private readonly scenario: ScenarioStep[];
  private readonly tokenDelayMs: number;
  private readonly now: () => number;
  private readonly sessionId = createId("session");

  private status: SessionStatus = "idle";
  private scenarioAbort: AbortController | null = null;
  private resumeWaiters: Array<() => void> = [];
  private readonly scripts = new Map<string, AnswerScript>();
  private readonly attempts = new Map<string, number>();
  private readonly inFlight = new Map<string, AbortController>();

  constructor(options: MockPipelineOptions) {
    this.emit = options.emit;
    this.scenario = options.scenario ?? DEMO_SCENARIO;
    this.tokenDelayMs = options.tokenDelayMs ?? 45;
    this.now = options.now ?? Date.now;
  }

  start(): void {
    if (this.status === "listening" || this.status === "paused") return;
    this.setStatus("listening", "Démo : événements simulés");
    this.scenarioAbort = new AbortController();
    void this.runScenario(this.scenarioAbort.signal);
  }

  pause(): void {
    if (this.status !== "listening") return;
    this.setStatus("paused", null);
  }

  resume(): void {
    if (this.status !== "paused") return;
    this.setStatus("listening", "Démo : événements simulés");
    const waiters = this.resumeWaiters;
    this.resumeWaiters = [];
    waiters.forEach((wake) => wake());
  }

  /** Stops listening. Answers already being generated are allowed to finish. */
  stop(): void {
    if (this.status === "idle") return;
    this.scenarioAbort?.abort();
    this.scenarioAbort = null;
    const waiters = this.resumeWaiters;
    this.resumeWaiters = [];
    waiters.forEach((wake) => wake());
    this.setStatus("idle", null);
  }

  askManual(text: string): void {
    const utterance = text.trim();
    if (!utterance) return;
    const language = guessLanguage(utterance);
    const question = this.detect({
      utterance,
      intent: guessIntent(utterance),
      language,
      domain: null,
      source: "manual",
      contextRef: null,
    });
    this.scripts.set(question.id, {
      primary:
        language === "en"
          ? `Mock answer — the LLM engine is connected in Phase 5. Received: "${utterance}"`
          : `Réponse simulée — le moteur LLM sera branché en phase 5. Question reçue : « ${utterance} »`,
    });
    void this.generate(question.id, 300);
  }

  retry(questionId: string): void {
    if (!this.scripts.has(questionId)) return;
    void this.generate(questionId, 300);
  }

  cancelAll(): void {
    this.inFlight.forEach((controller) => controller.abort());
    this.inFlight.clear();
  }

  dispose(): void {
    this.stop();
    this.cancelAll();
  }

  private setStatus(status: SessionStatus, message: string | null): void {
    this.status = status;
    this.emit({ type: "session.status", status, message });
  }

  private async runScenario(signal: AbortSignal): Promise<void> {
    const questionIds = new Map<string, string>();
    try {
      for (const step of this.scenario) {
        await sleep(step.delayMs, signal);
        await this.waitWhilePaused(signal);

        const question = this.detect({
          utterance: step.partialUtterance ?? step.utterance,
          intent: step.intent,
          language: step.language,
          domain: step.domain,
          source: "speech",
          contextRef: step.followUpOf ? (questionIds.get(step.followUpOf) ?? null) : null,
        });
        questionIds.set(step.key, question.id);
        this.scripts.set(question.id, step.answer);

        if (step.partialUtterance) {
          void this.reviseLater(question.id, step.utterance, Math.round(step.thinkMs / 2));
        }
        void this.generate(question.id, step.thinkMs);
      }
    } catch (error) {
      if (!(error instanceof AbortedError)) throw error;
    }
  }

  private async reviseLater(questionId: string, text: string, delayMs: number): Promise<void> {
    await sleep(delayMs);
    this.emit({ type: "question.revised", questionId, text });
  }

  private detect(input: {
    utterance: string;
    intent: Intent;
    language: Language;
    domain: string | null;
    source: DetectedQuestion["source"];
    contextRef: string | null;
  }): DetectedQuestion {
    const question: DetectedQuestion = {
      id: createId("q"),
      sessionId: this.sessionId,
      source: input.source,
      sourceSegmentIds: [],
      originalUtterance: input.utterance,
      normalizedQuestion: input.utterance,
      intent: input.intent,
      language: input.language,
      domain: input.domain,
      contextRef: input.contextRef,
      detectedAt: this.now(),
    };
    this.emit({ type: "question.detected", question });
    return question;
  }

  private async generate(questionId: string, thinkMs: number): Promise<void> {
    const script = this.scripts.get(questionId);
    if (!script) return;

    // Explicit policy: a retry supersedes any previous generation for the same question only.
    this.inFlight.get(questionId)?.abort();
    const controller = new AbortController();
    this.inFlight.set(questionId, controller);
    const attempt = (this.attempts.get(questionId) ?? 0) + 1;
    this.attempts.set(questionId, attempt);
    const requestId = createId("req");
    const { signal } = controller;

    try {
      await sleep(thinkMs, signal);
      this.emit({ type: "answer.started", questionId, requestId });

      const chunks = [
        ...tokenize(script.primary).map((delta) => ({ section: "primary" as const, delta })),
        ...tokenize(script.details ?? "").map((delta) => ({ section: "details" as const, delta })),
      ];
      const failAt = attempt === 1 ? script.failFirstAttemptAfterTokens : undefined;

      for (const [seq, chunk] of chunks.entries()) {
        await sleep(this.tokenDelayMs, signal);
        if (seq === failAt) {
          this.emit({
            type: "answer.failed",
            questionId,
            requestId,
            message: "Connexion interrompue (simulation)",
            retryable: true,
          });
          return;
        }
        this.emit({ type: "answer.delta", questionId, requestId, seq, ...chunk });
      }
      this.emit({ type: "answer.completed", questionId, requestId });
    } catch (error) {
      if (!(error instanceof AbortedError)) throw error;
    } finally {
      if (this.inFlight.get(questionId) === controller) this.inFlight.delete(questionId);
    }
  }

  private waitWhilePaused(signal: AbortSignal): Promise<void> {
    if (this.status !== "paused") return Promise.resolve();
    return new Promise((resolve, reject) => {
      this.resumeWaiters.push(() => (signal.aborted ? reject(new AbortedError()) : resolve()));
    });
  }
}

function sleep(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) return reject(new AbortedError());
    const timer = setTimeout(() => {
      signal?.removeEventListener("abort", onAbort);
      resolve();
    }, ms);
    const onAbort = () => {
      clearTimeout(timer);
      reject(new AbortedError());
    };
    signal?.addEventListener("abort", onAbort, { once: true });
  });
}

/** Word-sized chunks that keep their trailing whitespace, like an LLM token stream. */
export function tokenize(text: string): string[] {
  return text.match(/\S+\s*|\s+/g) ?? [];
}

// Mock-only heuristics for manual questions. The real classifier arrives in Phase 4.
function guessLanguage(text: string): Language {
  const french =
    /\b(le|la|les|un|une|des|est|comment|pourquoi|quoi|tu|vous|je|et|avec|dans)\b|[éèàçù]/i;
  const english = /\b(the|is|are|how|why|what|you|your|with|would|can|do)\b/i;
  const fr = french.test(text);
  const en = english.test(text);
  if (fr && en) return "mixed";
  return fr ? "fr" : "en";
}

function guessIntent(text: string): Intent {
  if (/^(bonjour|salut|hello|hi|hey)\b|comment ça va|how are you/i.test(text)) {
    return "GREETING_OR_SMALL_TALK";
  }
  if (/\b(implement|write a function|code|implémente|écris une fonction)\b/i.test(text)) {
    return "CODING_PROBLEM";
  }
  if (/\b(design|concevoir|architecture)\b/i.test(text)) return "SYSTEM_DESIGN";
  return "TECHNICAL_QUESTION";
}
