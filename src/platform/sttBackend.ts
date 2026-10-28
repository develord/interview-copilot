import { invoke } from "@tauri-apps/api/core";
import { asSttError, keyStatusSchema, type KeyStatus, type ProviderKind } from "@/domain/stt";
import { inTauri } from "./overlayWindow";

/**
 * Provider key management. Keys go to the Windows Credential Manager through Rust; the webview
 * can store, check for and delete a key, but never read one back.
 */
export const sttBackend = {
  available: inTauri,

  async keyStatus(): Promise<KeyStatus> {
    if (!inTauri) return { assemblyai: false, soniox: false };
    return keyStatusSchema.parse(await call("stt_key_status"));
  },
  setKey: (provider: ProviderKind, key: string) => call<void>("stt_set_key", { provider, key }),
  clearKey: (provider: ProviderKind) => call<void>("stt_clear_key", { provider }),
};

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri) throw asSttError(new Error("desktop app required"));
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw asSttError(error);
  }
}
