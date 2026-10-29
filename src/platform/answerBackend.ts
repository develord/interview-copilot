import { invoke } from "@tauri-apps/api/core";
import { asClassifyError } from "@/domain/intent";
import { profileSchema, asProfileError, type Profile } from "@/domain/profile";
import type { AnswerModel } from "@/domain/answer";
import { inTauri } from "./overlayWindow";

/**
 * Answers and the verified profile, both handled in Rust (where the Claude key lives).
 * Answers come back as `answer.*` pipeline events, not as return values.
 */
export const answerBackend = {
  available: inTauri,

  configure: (model: AnswerModel, earlyStart: boolean) =>
    call<void>("answer_configure", { model, earlyStart }),
  askManual: (text: string) => call<string>("answer_ask_manual", { text }),
  retry: (questionId: string) => call<void>("answer_retry", { questionId }),
  cancelAll: () => call<void>("answer_cancel_all"),
};

export const profileBackend = {
  available: inTauri,

  async get(): Promise<Profile | null> {
    if (!inTauri) return null;
    const value = await callProfile<unknown>("profile_get");
    return value === null ? null : profileSchema.parse(value);
  },
  async save(profile: Profile): Promise<Profile> {
    return profileSchema.parse(await callProfile<unknown>("profile_save", { profile }));
  },
  clear: () => callProfile<void>("profile_clear"),
  /** Sends the PDF to Claude once and returns an unverified draft. */
  async extractPdf(pdfBase64: string): Promise<Profile> {
    return profileSchema.parse(await callProfile<unknown>("profile_extract_pdf", { pdfBase64 }));
  },
};

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri) throw asClassifyError(new Error("desktop app required"));
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw asClassifyError(error);
  }
}

async function callProfile<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri) throw asProfileError(new Error("desktop app required"));
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw asProfileError(error);
  }
}
