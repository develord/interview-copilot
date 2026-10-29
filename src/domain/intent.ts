import { z } from "zod";
import { INTENTS } from "./types";

/**
 * Question-detection contracts. Mirrors `src-tauri/src/intent/types.rs`; the shared fixtures in
 * `contracts/fixtures` are validated by both the Rust and the TypeScript test suites.
 */

/** Claude models offered for classification; Haiku is the default (fastest, measured best). */
export const CLAUDE_MODELS = ["claude-haiku-5-5", "claude-opus-5-5"] as const;
export type ClaudeModel = (typeof CLAUDE_MODELS)[number];

export const DECISIONS = ["question", "revision", "ignored"] as const;
export type Decision = (typeof DECISIONS)[number];

export const IGNORE_REASONS = [
  "filler",
  "backchannel",
  "statement",
  "ambiguous",
  "repeat",
  "not-technical",
] as const;
export type UtteranceIgnoreReason = (typeof IGNORE_REASONS)[number];

export const STAGES = ["rules", "llm", "fallback"] as const;
export type Stage = (typeof STAGES)[number];

export const CLASSIFIER_STATES = ["ready", "rules-only", "degraded", "error"] as const;
export type ClassifierState = (typeof CLASSIFIER_STATES)[number];

export const CLASSIFY_ERROR_CODES = [
  "missing-key",
  "unauthorized",
  "quota",
  "rate-limited",
  "overloaded",
  "network",
  "timeout",
  "invalid-response",
  "refused",
  "protocol",
  "credential-store",
] as const;
export type ClassifyErrorCode = (typeof CLASSIFY_ERROR_CODES)[number];

const id = z.string().min(1).max(128);

export const classifyErrorSchema = z.object({
  code: z.enum(CLASSIFY_ERROR_CODES),
  detail: z.string().max(1000),
});
export type ClassifyError = z.infer<typeof classifyErrorSchema>;

export const utteranceReportSchema = z.object({
  id,
  sourceId: id,
  segmentIds: z.array(id).max(64),
  text: z.string().max(4000),
  decision: z.enum(DECISIONS),
  reason: z.enum(IGNORE_REASONS).nullable(),
  intent: z.enum(INTENTS).nullable(),
  stage: z.enum(STAGES),
  questionId: id.nullable(),
  latencyMs: z.number().int().nonnegative().nullable(),
});
export type UtteranceReport = z.infer<typeof utteranceReportSchema>;

export const classifierStatusSchema = z.object({
  state: z.enum(CLASSIFIER_STATES),
  model: z.enum(CLAUDE_MODELS).nullable(),
  error: classifyErrorSchema.nullable(),
  lastLatencyMs: z.number().int().nonnegative().nullable(),
  classified: z.number().int().nonnegative(),
  failed: z.number().int().nonnegative(),
});
export type ClassifierStatus = z.infer<typeof classifierStatusSchema>;

export const llmKeyStatusSchema = z.object({ anthropic: z.boolean() });
export type LlmKeyStatus = z.infer<typeof llmKeyStatusSchema>;

/** Sent with `audio_start`; mirrors `QuestionDetectionConfig` in Rust. */
export interface QuestionDetectionConfig {
  model: ClaudeModel;
  includeMicrophone: boolean;
  technicalOnly: boolean;
}

export function asClassifyError(value: unknown): ClassifyError {
  const parsed = classifyErrorSchema.safeParse(value);
  if (parsed.success) return parsed.data;
  return {
    code: "protocol",
    detail: value instanceof Error ? value.message : String(value),
  };
}
