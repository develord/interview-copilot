import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia, type Pinia } from "pinia";
import { SESSION_CONTROLLER, type SessionController } from "@/services/sessionController";
import { useOverlaySettings } from "@/stores/overlaySettings";
import { useSessionStore } from "@/stores/session";
import App from "./App.vue";

let pinia: Pinia;

function mountApp() {
  const controller = {
    start: vi.fn(),
    pause: vi.fn(),
    resume: vi.fn(),
    stop: vi.fn(),
    askManual: vi.fn(),
    retry: vi.fn(),
    cancelAll: vi.fn(),
    dispose: vi.fn(),
  } satisfies SessionController;
  const wrapper = mount(App, {
    attachTo: document.body,
    global: { plugins: [pinia], provide: { [SESSION_CONTROLLER as symbol]: controller } },
  });
  return { wrapper, controller };
}

async function pressEscape() {
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
  await flushPromises();
}

describe("App overlay layers", () => {
  beforeEach(() => {
    localStorage.clear();
    pinia = createPinia();
    setActivePinia(pinia);
  });
  afterEach(() => {
    document.body.innerHTML = "";
  });

  // Regression: the enlarged answer opened underneath the settings sheet, and Escape closed the
  // hidden layer first. The two layers are now mutually exclusive.
  it("never shows the settings sheet and the enlarged answer at the same time", async () => {
    const { wrapper } = mountApp();
    useSessionStore().dispatch({
      type: "question.detected",
      question: {
        id: "q1",
        sessionId: "s1",
        source: "manual",
        sourceSegmentIds: [],
        originalUtterance: "What is a closure?",
        normalizedQuestion: "What is a closure?",
        intent: "TECHNICAL_QUESTION",
        language: "en",
        domain: null,
        contextRef: null,
        detectedAt: 0,
      },
    });
    await flushPromises();

    await wrapper.get('button[title="Réglages"]').trigger("click");
    expect(wrapper.find(".sheet").exists()).toBe(true);

    await wrapper.get('[data-card-id="card:q1"] button[title^="Agrandir"]').trigger("click");
    expect(wrapper.find(".focus").exists()).toBe(true);
    expect(wrapper.find(".sheet").exists()).toBe(false);

    await wrapper.get('button[title="Réglages"]').trigger("click");
    expect(wrapper.find(".sheet").exists()).toBe(true);
    expect(wrapper.find(".focus").exists()).toBe(false);

    await pressEscape();
    expect(wrapper.find(".sheet").exists()).toBe(false);
  });

  it("clears the history and cancels in-flight generations together", async () => {
    const { wrapper, controller } = mountApp();
    const store = useSessionStore();
    store.dispatch({
      type: "question.detected",
      question: {
        id: "q1",
        sessionId: "s1",
        source: "speech",
        sourceSegmentIds: [],
        originalUtterance: "Bonjour !",
        normalizedQuestion: "Bonjour !",
        intent: "GREETING_OR_SMALL_TALK",
        language: "fr",
        domain: null,
        contextRef: null,
        detectedAt: 0,
      },
    });
    await flushPromises();

    const clearButton = wrapper.get('button[title="Effacer l\'historique"]');
    await clearButton.trigger("click");
    await clearButton.trigger("click");

    expect(controller.cancelAll).toHaveBeenCalledTimes(1);
    expect(store.cards).toHaveLength(0);
    expect(wrapper.find("[data-card-id]").exists()).toBe(false);
  });
});

describe("App consent before live capture", () => {
  beforeEach(() => {
    localStorage.clear();
    pinia = createPinia();
    setActivePinia(pinia);
  });
  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("asks for the other party's consent before capturing system audio live", async () => {
    const settings = useOverlaySettings();
    settings.sessionMode = "live";
    const { wrapper, controller } = mountApp();

    await wrapper.get("button.btn.primary").trigger("click"); // Démarrer
    expect(controller.start).not.toHaveBeenCalled();
    const dialog = wrapper.get('[role="dialog"][aria-labelledby="consent-title"]');
    const confirm = dialog.get("button.btn.primary");
    expect(confirm.attributes("disabled")).toBeDefined();

    await dialog.get('input[type="checkbox"]').setValue(true);
    await confirm.trigger("click");
    expect(controller.start).toHaveBeenCalledTimes(1);
    expect(wrapper.find('[aria-labelledby="consent-title"]').exists()).toBe(false);

    // Same interview: a second start within the validity window does not ask again.
    await wrapper.get("button.btn.primary").trigger("click");
    expect(controller.start).toHaveBeenCalledTimes(2);
  });

  it("does not ask when only the microphone is captured or in demo mode", async () => {
    const settings = useOverlaySettings();
    settings.sessionMode = "live";
    settings.audio.system.enabled = false;
    const { wrapper, controller } = mountApp();
    await wrapper.get("button.btn.primary").trigger("click");
    expect(controller.start).toHaveBeenCalledTimes(1);

    settings.sessionMode = "demo";
    settings.audio.system.enabled = true;
    await wrapper.get("button.btn.primary").trigger("click");
    expect(controller.start).toHaveBeenCalledTimes(2);
  });

  it("names the transcription service in the consent text when transcription is on", async () => {
    const settings = useOverlaySettings();
    settings.sessionMode = "live";
    settings.transcription.enabled = true;
    settings.transcription.provider = "assemblyai";
    const { wrapper } = mountApp();
    await wrapper.get("button.btn.primary").trigger("click");
    expect(wrapper.get('[aria-labelledby="consent-title"]').text()).toContain(
      "transmise à AssemblyAI",
    );
  });
});
