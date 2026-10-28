import { z } from "zod";
import type { QuestionDetectionConfig } from "./intent";
import type { TranscriptionConfig } from "./stt";

/**
 * Audio capture contracts. Mirrors `src-tauri/src/audio/types.rs`; the shared fixtures in
 * `contracts/fixtures` are validated by both the Rust and the TypeScript test suites.
 */

export const SOURCE_KINDS = ["system", "microphone"] as const;
export type SourceKind = (typeof SOURCE_KINDS)[number];

export const SOURCE_STATES = [
  "starting",
  "capturing",
  "paused",
  "muted",
  "recovering",
  "error",
  "stopped",
] as const;
export type SourceState = (typeof SOURCE_STATES)[number];

export const AUDIO_ERROR_CODES = [
  "permission-denied",
  "device-not-found",
  "device-lost",
  "device-busy",
  "unsupported-format",
  "no-source",
  "transcription-key-missing",
  "backend",
] as const;
export type AudioErrorCode = (typeof AUDIO_ERROR_CODES)[number];

export const audioErrorSchema = z.object({
  code: z.enum(AUDIO_ERROR_CODES),
  detail: z.string().max(1000),
});
export type AudioError = z.infer<typeof audioErrorSchema>;

export const sourceDiagnosticsSchema = z.object({
  source: z.enum(SOURCE_KINDS),
  state: z.enum(SOURCE_STATES),
  deviceName: z.string().max(256).nullable(),
  followsDefault: z.boolean(),
  sampleRate: z.number().int().positive().nullable(),
  channels: z.number().int().positive().nullable(),
  levelDbfs: z.number().min(-100).max(20),
  peakDbfs: z.number().min(-100).max(20),
  receiving: z.boolean(),
  capturedSeconds: z.number().nonnegative(),
  droppedFrames: z.number().int().nonnegative(),
  recoveries: z.number().int().nonnegative(),
  error: audioErrorSchema.nullable(),
});
export type SourceDiagnostics = z.infer<typeof sourceDiagnosticsSchema>;

const deviceInfoSchema = z.object({
  id: z.string().min(1).max(512),
  name: z.string().max(256),
  isDefault: z.boolean(),
});
export type DeviceInfo = z.infer<typeof deviceInfoSchema>;

export const audioDevicesSchema = z.object({
  outputs: z.array(deviceInfoSchema),
  inputs: z.array(deviceInfoSchema),
});
export type AudioDevices = z.infer<typeof audioDevicesSchema>;

export const sourceConfigSchema = z.object({
  enabled: z.boolean(),
  /** `null` follows the system default device. */
  deviceId: z.string().max(512).nullable(),
  muted: z.boolean(),
});
export type SourceConfig = z.infer<typeof sourceConfigSchema>;

export interface AudioConfig {
  system: SourceConfig;
  microphone: SourceConfig;
  /** Speech-to-text for the captured audio; `null` captures without transcribing. */
  transcription: TranscriptionConfig | null;
  /** Question detection on the transcripts; needs transcription. */
  questions: QuestionDetectionConfig | null;
}

/** Narrows an `invoke` rejection to an `AudioError` when the backend sent one. */
export function asAudioError(value: unknown): AudioError {
  const parsed = audioErrorSchema.safeParse(value);
  if (parsed.success) return parsed.data;
  return { code: "backend", detail: value instanceof Error ? value.message : String(value) };
}

/** Maps a dBFS level to a 0–100 meter position over a speech-friendly -60…0 dB range. */
export function meterPercent(dbfs: number): number {
  const FLOOR = -60;
  if (!Number.isFinite(dbfs) || dbfs <= FLOOR) return 0;
  return Math.min(100, Math.round(((dbfs - FLOOR) / -FLOOR) * 100));
}
