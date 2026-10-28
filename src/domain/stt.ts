import { z } from "zod";
import { SOURCE_KINDS } from "./audio";

/** Speech-to-text contracts. Mirrors `src-tauri/src/stt/types.rs` (fixtures in `contracts/`). */

export const PROVIDER_KINDS = ["assemblyai", "soniox"] as const;
export type ProviderKind = (typeof PROVIDER_KINDS)[number];

export const STT_STATES = ["connecting", "streaming", "reconnecting", "error", "closed"] as const;
export type SttState = (typeof STT_STATES)[number];

export const STT_ERROR_CODES = [
  "missing-key",
  "unauthorized",
  "quota",
  "network",
  "protocol",
  "provider",
] as const;
export type SttErrorCode = (typeof STT_ERROR_CODES)[number];

export const sttErrorSchema = z.object({
  code: z.enum(STT_ERROR_CODES),
  detail: z.string().max(1000),
});
export type SttError = z.infer<typeof sttErrorSchema>;

export const sttStatusSchema = z.object({
  source: z.enum(SOURCE_KINDS),
  provider: z.enum(PROVIDER_KINDS),
  state: z.enum(STT_STATES),
  error: sttErrorSchema.nullable(),
  lastFinalLatencyMs: z.number().int().nonnegative().nullable(),
  droppedChunks: z.number().int().nonnegative(),
});
export type SttStatus = z.infer<typeof sttStatusSchema>;

export const keyStatusSchema = z.object({ assemblyai: z.boolean(), soniox: z.boolean() });
export type KeyStatus = z.infer<typeof keyStatusSchema>;

/** Sent with `audio_start`; mirrors `TranscriptionConfig` in Rust. */
export interface TranscriptionConfig {
  provider: ProviderKind;
  extraTerms: string[];
  forceEndpointMs: number | null;
  builtinVocabulary: boolean;
}

export function asSttError(value: unknown): SttError {
  const parsed = sttErrorSchema.safeParse(value);
  if (parsed.success) return parsed.data;
  return { code: "provider", detail: value instanceof Error ? value.message : String(value) };
}

/** Splits a comma/newline separated list of user terms, trimmed and without duplicates. */
export function parseTerms(input: string): string[] {
  const seen = new Set<string>();
  return input
    .split(/[,\n;]/)
    .map((t) => t.trim())
    .filter((t) => t.length > 0 && t.length <= 50)
    .filter((t) => {
      const key = t.toLowerCase();
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    })
    .slice(0, 100);
}
