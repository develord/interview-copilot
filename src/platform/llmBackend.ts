import { invoke } from "@tauri-apps/api/core";
import { asClassifyError, llmKeyStatusSchema, type LlmKeyStatus } from "@/domain/intent";
import { inTauri } from "./overlayWindow";

/**
 * Claude key management. The key goes to the Windows Credential Manager through Rust; the
 * webview can store, check for and delete it, but never read it back.
 */
export const llmBackend = {
  available: inTauri,

  async keyStatus(): Promise<LlmKeyStatus> {
    if (!inTauri) return { anthropic: false };
    return llmKeyStatusSchema.parse(await call("llm_key_status"));
  },
  setKey: (key: string) => call<void>("llm_set_key", { key }),
  clearKey: () => call<void>("llm_clear_key"),
};

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri) throw asClassifyError(new Error("desktop app required"));
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw asClassifyError(error);
  }
}
