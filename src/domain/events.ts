import { z } from "zod";
import { sourceDiagnosticsSchema } from "./audio";
import { sttStatusSchema } from "./stt";
import { ANSWER_SECTIONS, INTENTS, LANGUAGES, QUESTION_SOURCES, SESSION_STATUSES } from "./types";

/**
 * Typed pipeline event contracts. The zod schemas are the single source of truth: TypeScript
 * types are inferred from them, and every event entering the UI from outside (mock pipeline
 * today, Rust backend later) is validated with `parsePipelineEvent` before it touches state.
 */

const id = z.string().min(1).max(128);
const text = z.string().max(4000);

export const transcriptSegmentSchema = z.object({
  id,
  sessionId: id,
  sourceId: id,
  startMs: z.number().nonnegative(),
  endMs: z.number().nonnegative(),
  text,
  normalizedText: text.nullable(),
  isFinal: z.boolean(),
  confidence: z.number().min(0).max(1).nullable(),
  speakerId: id.nullable(),
});
export type TranscriptSegment = z.infer<typeof transcriptSegmentSchema>;

export const detectedQuestionSchema = z.object({
  id,
  sessionId: id,
  source: z.enum(QUESTION_SOURCES),
  sourceSegmentIds: z.array(id),
  originalUtterance: text.min(1),
  normalizedQuestion: text.min(1),
  intent: z.enum(INTENTS),
  language: z.enum(LANGUAGES),
  domain: z.string().max(64).nullable(),
  /** questionId of the earlier question this one follows up on, if any. */
  contextRef: id.nullable(),
  detectedAt: z.number().nonnegative(),
});
export type DetectedQuestion = z.infer<typeof detectedQuestionSchema>;

export const pipelineEventSchema = z.discriminatedUnion("type", [
  z.object({
    type: z.literal("session.status"),
    status: z.enum(SESSION_STATUSES),
    message: z.string().max(500).nullable(),
  }),
  z.object({
    type: z.literal("audio.diagnostics"),
    sources: z.array(sourceDiagnosticsSchema).max(4),
  }),
  z.object({ type: z.literal("stt.status") }).extend(sttStatusSchema.shape),
  z.object({ type: z.literal("transcript.partial"), segment: transcriptSegmentSchema }),
  z.object({ type: z.literal("transcript.final"), segment: transcriptSegmentSchema }),
  z.object({ type: z.literal("question.detected"), question: detectedQuestionSchema }),
  z.object({ type: z.literal("question.revised"), questionId: id, text: text.min(1) }),
  z.object({ type: z.literal("answer.started"), questionId: id, requestId: id }),
  z.object({
    type: z.literal("answer.delta"),
    questionId: id,
    requestId: id,
    /** Per-request sequence number starting at 0; used to drop duplicates and reorder. */
    seq: z.number().int().nonnegative(),
    section: z.enum(ANSWER_SECTIONS),
    delta: z.string().max(20000),
  }),
  z.object({ type: z.literal("answer.completed"), questionId: id, requestId: id }),
  z.object({
    type: z.literal("answer.failed"),
    questionId: id,
    requestId: id,
    message: z.string().max(500),
    retryable: z.boolean(),
  }),
]);
export type PipelineEvent = z.infer<typeof pipelineEventSchema>;
export type PipelineEventType = PipelineEvent["type"];

/** Events that create or modify answer cards. Every one of them carries a stable question ID. */
export type CardEvent = Extract<PipelineEvent, { type: `question.${string}` | `answer.${string}` }>;

export function isCardEvent(event: PipelineEvent): event is CardEvent {
  return event.type.startsWith("question.") || event.type.startsWith("answer.");
}

export type ParseResult = { ok: true; event: PipelineEvent } | { ok: false; error: string };

/** Never throws: malformed payloads are reported, not propagated into the session. */
export function parsePipelineEvent(input: unknown): ParseResult {
  const result = pipelineEventSchema.safeParse(input);
  if (result.success) return { ok: true, event: result.data };
  return { ok: false, error: z.prettifyError(result.error) };
}
