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
      return event;
    }
    if (event.type === "transcript.partial" || event.type === "transcript.final") {
      upsertSegment(event.segment);
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
      active.value = {
        questionId: q.id,
        question: q.normalizedQuestion,
        intent: q.intent,
        language: q.language,
        detectedAt: q.detectedAt,
      };
    } else if (event.type === "question.revised" && active.value?.questionId === event.questionId) {
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
      }
    }
    transcript.byId[segment.id] = segment;
  }

  const transcriptLines = computed(() => transcript.order.map((id) => transcript.byId[id]!));

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
    cards,
    inFlightCount,
    diagnostics,
    dispatch,
    findCard,
    clearHistory,
  };
});
