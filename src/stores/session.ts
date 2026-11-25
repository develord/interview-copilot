import { defineStore } from "pinia";
import { computed, reactive, ref } from "vue";
import {
  applyEvent,
  clearCardState,
  createCardState,
  getCardByQuestion,
  type AnswerCard,
  type CardState,
  type IgnoreReason,
} from "@/domain/cardLifecycle";
import type { AudioError, SourceDiagnostics, SourceKind } from "@/domain/audio";
import type { ClassifierStatus, UtteranceReport } from "@/domain/intent";
import type { SttStatus } from "@/domain/stt";
import {
  isCardEvent,
  parsePipelineEvent,
  type PipelineEvent,
  type PipelineEventType,
  type TranscriptSegment,
} from "@/domain/events";
import type { Intent, Language, SessionStatus } from "@/domain/types";
import { breakdown, median, type LatencyBreakdown, type LatencyMarks } from "@/domain/latency";

/**
 * The question currently being processed. Deliberately a separate, copied object — never the
 * card itself — so the active panel and the history can evolve independently (PRD §6.5).
 */
export interface ActiveQuestion {
  questionId: string;
  question: string;
  intent: Intent;
  language: Language;
  detectedAt: number;
}

/** Transcript lines kept for the diagnostics panel (oldest dropped first). */
export const TRANSCRIPT_LIMIT = 200;
/** Answered questions whose latency feeds the median. */
const LATENCY_WINDOW = 20;
/** The `stt.status` carrying a final's latency is emitted right after that final. */
const STATUS_AFTER_FINAL_MS = 250;

export interface SessionDiagnostics {
  applied: number;
  ignored: number;
  invalid: number;
  lastIgnored: { type: PipelineEventType; reason: IgnoreReason } | null;
  lastInvalid: string | null;
}

export const useSessionStore = defineStore("session", () => {
  const cardState: CardState = reactive(createCardState());
  const status = ref<SessionStatus>("idle");
  const statusMessage = ref<string | null>(null);
  const active = ref<ActiveQuestion | null>(null);
  /** Latest per-source capture report (replaced ~10×/s while capturing). */
  const audioSources = ref<SourceDiagnostics[]>([]);
  /** Last failed capture command (e.g. start with nothing enabled); cleared once capture runs. */
  const commandError = ref<AudioError | null>(null);
  /** Live transcript, keyed by segment id: partial updates replace the same entry. */
  const transcript = reactive<{ byId: Record<string, TranscriptSegment>; order: string[] }>({
    byId: {},
    order: [],
  });
  const sttStatus = ref<Partial<Record<SourceKind, SttStatus>>>({});
  /** Question-detection decisions, keyed by the last transcript segment of each utterance. */
  const utteranceBySegment = reactive<Record<string, UtteranceReport>>({});
  const classifierStatus = ref<ClassifierStatus | null>(null);
  /** Arrival time of each final transcript, and its end-of-speech → final latency. */
  const finals = new Map<string, { at: number; transcriptionMs: number | null }>();
  const lastFinal: Partial<Record<string, string>> = {};
  /** Event times per question (not reactive), and the latency shown once the first word is in. */
  const latencyMarks = new Map<string, LatencyMarks>();
  const latencies = reactive<Record<string, LatencyBreakdown>>({});
  const diagnostics = reactive<SessionDiagnostics>({
    applied: 0,
    ignored: 0,
    invalid: 0,
    lastIgnored: null,
    lastInvalid: null,
  });

  const cards = computed<AnswerCard[]>(() => cardState.order.map((id) => cardState.cards[id]!));
  const activeCard = computed(() =>
    active.value ? (getCardByQuestion(cardState, active.value.questionId) ?? null) : null,
  );
  const inFlightCount = computed(
    () => cards.value.filter((c) => c.status === "pending" || c.status === "streaming").length,
  );

  /**
   * Single entry point for pipeline events. Accepts `unknown`: everything is validated here.
   * Returns the validated event once it has been applied, or `null` when it was malformed or
   * ignored by the card lifecycle (a duplicate question, a stale token...).
   */
  function dispatch(input: unknown, now: number = Date.now()): PipelineEvent | null {
    const parsed = parsePipelineEvent(input);
    if (!parsed.ok) {
      diagnostics.invalid += 1;
      diagnostics.lastInvalid = parsed.error;
      return null;
    }
    const event = parsed.event;

    if (event.type === "session.status") {
      status.value = event.status;
      statusMessage.value = event.message;
      if (event.status === "listening") commandError.value = null;
      return event;
    }
    if (event.type === "audio.diagnostics") {
      audioSources.value = event.sources;
      return event;
    }
    if (event.type === "classifier.status") {
      classifierStatus.value = {
        state: event.state,
        model: event.model,
        error: event.error,
        lastLatencyMs: event.lastLatencyMs,
        classified: event.classified,
        failed: event.failed,
      };
      return event;
    }
    if (event.type === "utterance.classified") {
      const last = event.utterance.segmentIds.at(-1);
      if (last) utteranceBySegment[last] = event.utterance;
      const questionId = event.utterance.questionId;
      const marks = questionId ? latencyMarks.get(questionId) : undefined;
      if (marks && event.utterance.decision === "question" && event.utterance.stage === "llm") {
        marks.classifyMs = event.utterance.latencyMs;
        settleLatency(questionId!);
      }
      return event;
    }
    if (event.type === "stt.status") {
      const status: SttStatus = {
        source: event.source,
        provider: event.provider,
        state: event.state,
        error: event.error,
        lastFinalLatencyMs: event.lastFinalLatencyMs,
        droppedChunks: event.droppedChunks,
      };
      sttStatus.value = { ...sttStatus.value, [status.source]: status };
      const segmentId = lastFinal[status.source];
      const final = segmentId ? finals.get(segmentId) : undefined;
      if (
        final &&
        final.transcriptionMs === null &&
        status.lastFinalLatencyMs !== null &&
        now - final.at <= STATUS_AFTER_FINAL_MS
      ) {
        final.transcriptionMs = status.lastFinalLatencyMs;
      }
      return event;
    }
    if (event.type === "transcript.partial" || event.type === "transcript.final") {
      upsertSegment(event.segment);
      if (event.type === "transcript.final" && !finals.has(event.segment.id)) {
        finals.set(event.segment.id, { at: now, transcriptionMs: null });
        lastFinal[event.segment.sourceId] = event.segment.id;
      }
      return event;
    }
    if (!isCardEvent(event)) return event;

    const outcome = applyEvent(cardState, event, now);
    if (!outcome.applied) {
      diagnostics.ignored += 1;
      diagnostics.lastIgnored = { type: event.type, reason: outcome.reason };
      return null;
    }
    diagnostics.applied += 1;

    if (event.type === "question.detected") {
      const q = event.question;
      const lastSegment = q.sourceSegmentIds.at(-1);
      const final = lastSegment ? finals.get(lastSegment) : undefined;
      latencyMarks.set(q.id, {
        transcriptionMs: final?.transcriptionMs ?? null,
        finalAt: final?.at ?? null,
        detectedAt: now,
        classifyMs: null,
        firstWordAt: null,
      });
      active.value = {
        questionId: q.id,
        question: q.normalizedQuestion,
        intent: q.intent,
        language: q.language,
        detectedAt: q.detectedAt,
      };
    } else if (event.type === "answer.delta" && event.delta.trim()) {
      const marks = latencyMarks.get(event.questionId);
      if (marks && marks.firstWordAt === null) {
        marks.firstWordAt = now;
        settleLatency(event.questionId);
      }
    } else if (
      (event.type === "question.revised" || event.type === "answer.started") &&
      active.value?.questionId === event.questionId
    ) {
      const card = getCardByQuestion(cardState, event.questionId);
      if (card) active.value = { ...active.value, question: card.question };
    }
    return event;
  }

  function upsertSegment(segment: TranscriptSegment): void {
    const existing = transcript.byId[segment.id];
    // A late partial must never reopen a final line.
    if (existing?.isFinal && !segment.isFinal) return;
    if (!existing) {
      transcript.order.push(segment.id);
      while (transcript.order.length > TRANSCRIPT_LIMIT) {
        const dropped = transcript.order.shift()!;
        delete transcript.byId[dropped];
        delete utteranceBySegment[dropped];
        finals.delete(dropped);
      }
    }
    transcript.byId[segment.id] = segment;
  }

  const transcriptLines = computed(() => transcript.order.map((id) => transcript.byId[id]!));

  /** Replaced only when that question's marks change, so other cards never re-render. */
  function settleLatency(questionId: string): void {
    const marks = latencyMarks.get(questionId);
    const value = marks ? breakdown(marks) : null;
    if (value) latencies[questionId] = Object.freeze(value);
  }

  /** Stage by stage, once the first word of the answer is on screen. */
  function latencyOf(questionId: string): LatencyBreakdown | null {
    return latencies[questionId] ?? null;
  }

  /** Median end of speech → first word over the latest spoken questions. */
  const medianLatencyMs = computed(() =>
    median(
      Object.values(latencies)
        .filter((l) => l.detectionMs !== null)
        .slice(-LATENCY_WINDOW)
        .map((l) => l.totalMs),
    ),
  );

  function reportCommandError(error: AudioError): void {
    commandError.value = error;
  }

  function findCard(questionId: string): AnswerCard | undefined {
    return getCardByQuestion(cardState, questionId);
  }

  function clearHistory(): void {
    clearCardState(cardState);
    active.value = null;
    transcript.byId = {};
    transcript.order = [];
    for (const key of Object.keys(utteranceBySegment)) delete utteranceBySegment[key];
    for (const key of Object.keys(latencies)) delete latencies[key];
    latencyMarks.clear();
    finals.clear();
  }

  return {
    cardState,
    status,
    statusMessage,
    active,
    activeCard,
    audioSources,
    commandError,
    reportCommandError,
    transcriptLines,
    sttStatus,
    utteranceBySegment,
    classifierStatus,
    latencyOf,
    medianLatencyMs,
    cards,
    inFlightCount,
    diagnostics,
    dispatch,
    findCard,
    clearHistory,
  };
});
