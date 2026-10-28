import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia, type Pinia } from "pinia";
import type { SourceDiagnostics } from "@/domain/audio";
import { useOverlaySettings } from "@/stores/overlaySettings";
import { useSessionStore } from "@/stores/session";
import AudioStatusStrip from "./AudioStatusStrip.vue";

const openPrivacy = vi.hoisted(() => vi.fn(async () => {}));
vi.mock("@/platform/audioBackend", () => ({
  audioBackend: { available: true, openMicrophonePrivacySettings: openPrivacy },
}));

function diagnostics(overrides: Partial<SourceDiagnostics>): SourceDiagnostics {
  return {
    source: "system",
    state: "capturing",
    deviceName: "Haut-parleurs (Realtek(R) Audio)",
    followsDefault: true,
    sampleRate: 48000,
    channels: 2,
    levelDbfs: -30,
    peakDbfs: -20,
    receiving: true,
    capturedSeconds: 3,
    droppedFrames: 0,
    recoveries: 0,
    error: null,
    ...overrides,
  };
}

let pinia: Pinia;

function setup(sources: SourceDiagnostics[], status: "idle" | "listening" = "listening") {
  const store = useSessionStore();
  const settings = useOverlaySettings();
  settings.sessionMode = "live";
  store.dispatch({ type: "session.status", status, message: null });
  store.dispatch({ type: "audio.diagnostics", sources });
  const wrapper = mount(AudioStatusStrip, { global: { plugins: [pinia] } });
  return { wrapper, store, settings };
}

describe("AudioStatusStrip", () => {
  beforeEach(() => {
    localStorage.clear();
    pinia = createPinia();
    setActivePinia(pinia);
    openPrivacy.mockClear();
  });

  it("shows a level meter and state for each capturing source", () => {
    const { wrapper } = setup([
      diagnostics({ source: "system", levelDbfs: -30 }),
      diagnostics({ source: "microphone", levelDbfs: -100, receiving: false }),
    ]);

    const meters = wrapper.findAll('[role="meter"]');
    expect(meters.map((m) => m.attributes("aria-valuenow"))).toEqual(["50", "0"]);
    expect(wrapper.get('[data-source="system"]').text()).toContain("Capture");
    expect(wrapper.get('[data-source="microphone"]').text()).toContain("Silence");
  });

  it("says when loopback is running but nothing is playing", () => {
    const { wrapper } = setup([diagnostics({ receiving: false, levelDbfs: -100 })]);
    expect(wrapper.text()).toContain("Aucun son joué");
  });

  it("mutes a source in one click", async () => {
    const { wrapper, settings } = setup([diagnostics({ source: "microphone" })]);
    await wrapper.get("button.mute").trigger("click");
    expect(settings.audio.microphone.muted).toBe(true);
    expect(wrapper.get("button.mute").attributes("aria-pressed")).toBe("true");
  });

  it("explains a microphone permission error and offers the fix", async () => {
    const { wrapper } = setup([
      diagnostics({ source: "system" }),
      diagnostics({
        source: "microphone",
        state: "error",
        receiving: false,
        error: { code: "permission-denied", detail: "Access is denied. (os error -2147024891)" },
      }),
    ]);

    const alert = wrapper.get('[role="alert"]');
    expect(alert.text()).toContain("Microphone : accès refusé par Windows");
    await alert.get("button.link").trigger("click");
    await flushPromises();
    expect(openPrivacy).toHaveBeenCalledTimes(1);
  });

  it("keeps errors visible after the session stopped", () => {
    const { wrapper } = setup(
      [
        diagnostics({
          state: "stopped",
          receiving: false,
          error: { code: "device-lost", detail: "unplugged" },
        }),
      ],
      "idle",
    );
    expect(wrapper.find('[role="meter"]').exists()).toBe(false);
    expect(wrapper.text()).toContain("périphérique déconnecté");
  });

  it("stays hidden in demo mode and when idle without errors", () => {
    const { wrapper, settings, store } = setup([diagnostics({})], "idle");
    store.dispatch({ type: "audio.diagnostics", sources: [] });
    expect(wrapper.find(".audio-strip").exists()).toBe(false);
    settings.sessionMode = "demo";
    expect(wrapper.find(".audio-strip").exists()).toBe(false);
  });

  it("says when question detection runs without Claude, and why", async () => {
    const { wrapper, store } = setup([diagnostics({})]);
    const status = (state: string, code: string | null) => ({
      type: "classifier.status",
      state,
      model: state === "rules-only" ? null : "claude-opus-5-5",
      error: code ? { code, detail: "detail" } : null,
      lastLatencyMs: null,
      classified: 0,
      failed: 0,
    });

    store.dispatch(status("rules-only", "missing-key"));
    await flushPromises();
    expect(wrapper.text()).toContain("pas de clé Claude");

    store.dispatch(status("ready", null));
    await flushPromises();
    expect(wrapper.text()).not.toContain("Questions :");
  });
});
