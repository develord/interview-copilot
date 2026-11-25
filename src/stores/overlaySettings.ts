import { defineStore } from "pinia";
import { reactive, toRefs, watch } from "vue";
import { z } from "zod";
import { ANSWER_MODELS } from "@/domain/answer";
import { CLAUDE_MODELS } from "@/domain/intent";
import { inTauri } from "@/platform/overlayWindow";

const STORAGE_KEY = "interview-copilot.overlay-settings.v1";

const sourceSettings = (enabled: boolean) => {
  const fallback = { enabled, deviceId: null, muted: false };
  return z
    .object({
      enabled: z.boolean().catch(enabled),
      /** `null` follows the system default device. */
      deviceId: z.string().max(512).nullable().catch(null),
      muted: z.boolean().catch(false),
    })
    .catch(fallback);
};

/** Persisted overlay preferences. Every field self-heals to a default if storage is corrupt. */
const settingsSchema = z.object({
  opacity: z.number().min(0.4).max(1).catch(1),
  alwaysOnTop: z.boolean().catch(true),
  order: z.enum(["newest-last", "newest-first"]).catch("newest-last"),
  /** Show only the newest question's card: a new question replaces the previous one. */
  latestOnly: z.boolean().catch(true),
  compact: z.boolean().catch(false),
  /** Logical window height to restore when leaving compact mode. */
  expandedHeight: z.number().positive().nullable().catch(null),
  /** `demo` plays the scripted interview; `live` captures real audio (desktop app only). */
  sessionMode: z.enum(["demo", "live"]).catch(inTauri ? "live" : "demo"),
  audio: z
    .object({
      // System audio (the other participants) is on by default; the microphone never is.
      system: sourceSettings(true),
      microphone: sourceSettings(false),
    })
    .catch({
      system: { enabled: true, deviceId: null, muted: false },
      microphone: { enabled: false, deviceId: null, muted: false },
    }),
  transcription: z
    .object({
      /** Off until the user stores a provider key and turns it on. */
      enabled: z.boolean().catch(false),
      provider: z.enum(["assemblyai", "soniox"]).catch("assemblyai"),
      /** Comma-separated terms from the job description (stack, products, acronyms). */
      extraTerms: z.string().max(4000).catch(""),
      builtinVocabulary: z.boolean().catch(true),
      /** Close a question after 700 ms of local silence instead of waiting for the provider. */
      fastEndpoint: z.boolean().catch(true),
    })
    .catch({
      enabled: false,
      provider: "assemblyai",
      extraTerms: "",
      builtinVocabulary: true,
      fastEndpoint: true,
    }),
  questions: z
    .object({
      /** Detect questions in the live transcript (needs transcription). */
      enabled: z.boolean().catch(true),
      model: z.enum(CLAUDE_MODELS).catch("claude-haiku-5-5"),
      /** Also treat microphone speech as questions, for testing without an interviewer. */
      includeMicrophone: z.boolean().catch(false),
      /** Cards and answers for technical questions only (and follow-ups on them). */
      technicalOnly: z.boolean().catch(true),
    })
    .catch({
      enabled: true,
      model: "claude-haiku-5-5",
      includeMicrophone: false,
      technicalOnly: true,
    }),
  answers: z
    .object({
      model: z.enum(ANSWER_MODELS).catch("claude-sonnet-5-5"),
      /** Start writing while Claude decides whether the sentence is a question (~1 s sooner). */
      earlyStart: z.boolean().catch(true),
    })
    .catch({ model: "claude-sonnet-5-5", earlyStart: true }),
});
export type OverlaySettings = z.infer<typeof settingsSchema>;
export type CardOrder = OverlaySettings["order"];

function load(): OverlaySettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return settingsSchema.parse(raw ? JSON.parse(raw) : {});
  } catch {
    return settingsSchema.parse({});
  }
}

export const useOverlaySettings = defineStore("overlaySettings", () => {
  const settings = reactive(load());

  watch(
    settings,
    (value) => {
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
      } catch {
        // Storage can be unavailable (private mode, quota); settings then last for the session.
      }
    },
    { deep: true },
  );

  return { ...toRefs(settings) };
});
