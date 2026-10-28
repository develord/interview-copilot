import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  asAudioError,
  audioDevicesSchema,
  type AudioConfig,
  type AudioDevices,
  type AudioError,
  type SourceKind,
} from "@/domain/audio";
import { inTauri } from "./overlayWindow";

/** Tauri event carrying every Rust-side `PipelineEvent` (see src-tauri/src/events.rs). */
const PIPELINE_EVENT = "pipeline:event";

/**
 * Typed wrapper over the Rust audio commands. Rejections are always normalized to an
 * `AudioError`; outside the desktop app every command rejects with a `backend` error.
 */
export const audioBackend = {
  available: inTauri,

  async listDevices(): Promise<AudioDevices> {
    return audioDevicesSchema.parse(await call("audio_list_devices"));
  },
  start: (config: AudioConfig) => call<void>("audio_start", { config }),
  pause: () => call<void>("audio_pause"),
  resume: () => call<void>("audio_resume"),
  stop: () => call<void>("audio_stop"),
  setMuted: (source: SourceKind, muted: boolean) =>
    call<void>("audio_set_muted", { source, muted }),
  openMicrophonePrivacySettings: () => call<void>("open_microphone_privacy_settings"),

  /** Forwards raw Rust pipeline events; the session store validates every payload. */
  onPipelineEvent(handler: (payload: unknown) => void): Promise<() => void> {
    if (!inTauri) return Promise.resolve(() => {});
    return listen<unknown>(PIPELINE_EVENT, (event) => handler(event.payload));
  },
};

export type AudioBackend = typeof audioBackend;

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri) {
    const unavailable: AudioError = { code: "backend", detail: "desktop app required" };
    throw unavailable;
  }
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw asAudioError(error);
  }
}
