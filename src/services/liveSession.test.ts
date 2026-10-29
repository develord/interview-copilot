import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { nextTick } from "vue";
import type { AudioConfig, AudioError } from "@/domain/audio";
import type { AudioBackend } from "@/platform/audioBackend";
import { useOverlaySettings } from "@/stores/overlaySettings";
import { useSessionStore } from "@/stores/session";
import { audioConfig, createSession } from "./createSession";
import { LiveAudioSession } from "./liveAudioSession";
import type { SessionController } from "./sessionController";
import { SessionRouter } from "./sessionRouter";

function fakeBackend() {
  let pipelineHandler: ((payload: unknown) => void) | null = null;
  const backend = {
    available: true,
    listDevices: vi.fn(async () => ({ outputs: [], inputs: [] })),
    start: vi.fn<(config: AudioConfig) => Promise<void>>(async () => {}),
    pause: vi.fn(async () => {}),
    resume: vi.fn(async () => {}),
    stop: vi.fn(async () => {}),
    setMuted: vi.fn(async () => {}),
    openMicrophonePrivacySettings: vi.fn(async () => {}),
    onPipelineEvent: vi.fn(async (handler: (payload: unknown) => void) => {
      pipelineHandler = handler;
      return () => (pipelineHandler = null);
    }),
  } satisfies AudioBackend;
  return { backend, emitFromRust: (payload: unknown) => pipelineHandler?.(payload) };
}

function stubController(): SessionController {
  return {
    start: vi.fn(),
    pause: vi.fn(),
    resume: vi.fn(),
    stop: vi.fn(),
    askManual: vi.fn(),
    retry: vi.fn(),
    cancelAll: vi.fn(),
    dispose: vi.fn(),
  };
}

const config: AudioConfig = {
  system: { enabled: true, deviceId: null, muted: false },
  microphone: { enabled: false, deviceId: null, muted: false },
  transcription: null,
  questions: null,
};

describe("LiveAudioSession", () => {
  it("starts capture with the configuration read at start time", async () => {
    const { backend } = fakeBackend();
    let current = config;
    const live = new LiveAudioSession({
      backend,
      getConfig: () => current,
      onCommandError: vi.fn(),
      answers: stubController(),
    });
    current = { ...config, microphone: { enabled: true, deviceId: "mic-1", muted: true } };

    live.start();
    await Promise.resolve();

    expect(backend.start).toHaveBeenCalledWith(current);
  });

  it("reports a failed command instead of throwing", async () => {
    const { backend } = fakeBackend();
    const failure: AudioError = { code: "no-source", detail: "no audio source is enabled" };
    backend.start.mockRejectedValueOnce(failure);
    const onCommandError = vi.fn();
    const live = new LiveAudioSession({
      backend,
      getConfig: () => config,
      onCommandError,
      answers: stubController(),
    });

    live.start();
    await vi.waitFor(() => expect(onCommandError).toHaveBeenCalledWith(failure));
  });

  it("delegates questions to the (simulated) answer engine", () => {
    const { backend } = fakeBackend();
    const answers = stubController();
    const live = new LiveAudioSession({
      backend,
      getConfig: () => config,
      onCommandError: vi.fn(),
      answers,
    });

    live.askManual("What is a closure?");
    live.retry("q1");

    expect(answers.askManual).toHaveBeenCalledWith("What is a closure?");
    expect(answers.retry).toHaveBeenCalledWith("q1");
    expect(backend.start).not.toHaveBeenCalled();
  });
});

describe("SessionRouter", () => {
  it("routes commands to the selected mode and stops the old one on switch", () => {
    const demo = stubController();
    const live = stubController();
    const router = new SessionRouter({ demo, live }, "demo");

    router.start();
    expect(demo.start).toHaveBeenCalled();

    router.setMode("live");
    expect(demo.stop).toHaveBeenCalled();
    router.start();
    expect(live.start).toHaveBeenCalled();
    expect(router.activeMode).toBe("live");
  });

  it("falls back to the demo when live capture is unavailable", () => {
    const demo = stubController();
    const router = new SessionRouter({ demo, live: null }, "live");
    router.start();
    expect(router.activeMode).toBe("demo");
    expect(demo.start).toHaveBeenCalled();
  });
});

describe("createSession (desktop wiring)", () => {
  beforeEach(() => {
    localStorage.clear();
    setActivePinia(createPinia());
  });

  function fakeAnswers() {
    return {
      configure: vi.fn(async () => {}),
      askManual: vi.fn(async () => "manual-1"),
      retry: vi.fn(async () => {}),
      cancelAll: vi.fn(async () => {}),
    };
  }

  function setup(answers = fakeAnswers()) {
    const session = useSessionStore();
    const settings = useOverlaySettings();
    settings.sessionMode = "live";
    const fake = fakeBackend();
    const created = createSession({ session, settings, backend: fake.backend, answers });
    return { session, settings, created, ...fake };
  }

  it("stops any orphaned capture at startup", () => {
    const { backend } = setup();
    expect(backend.stop).toHaveBeenCalledTimes(1);
  });

  it("feeds Rust pipeline events into the validated store", async () => {
    const { session, emitFromRust } = setup();
    await vi.waitFor(() => expect(session.status).toBe("idle"));
    emitFromRust({ type: "session.status", status: "listening", message: null });
    emitFromRust({ type: "session.status", status: "not-a-status", message: null });

    expect(session.status).toBe("listening");
    expect(session.diagnostics.invalid).toBe(1);
  });

  it("applies mute changes live only while a live session runs", async () => {
    const { settings, session, backend, emitFromRust } = setup();
    await Promise.resolve();

    settings.audio.microphone.muted = true;
    await nextTick();
    expect(backend.setMuted).not.toHaveBeenCalled();

    emitFromRust({ type: "session.status", status: "listening", message: null });
    settings.audio.microphone.muted = false;
    await nextTick();
    expect(backend.setMuted).toHaveBeenCalledWith("microphone", false);
    expect(session.commandError).toBeNull();
  });

  it("surfaces a start failure and clears it once capture runs", async () => {
    const { created, session, backend, emitFromRust } = setup();
    backend.start.mockRejectedValueOnce({ code: "no-source", detail: "nothing enabled" });

    created.controller.start();
    await vi.waitFor(() => expect(session.commandError?.code).toBe("no-source"));

    emitFromRust({ type: "session.status", status: "listening", message: null });
    expect(session.commandError).toBeNull();
  });

  it("detects questions only when transcription runs, with the chosen model", () => {
    const { settings } = setup();
    expect(audioConfig(settings).questions).toBeNull();

    settings.transcription.enabled = true;
    expect(audioConfig(settings).questions).toEqual({
      model: "claude-haiku-5-5",
      includeMicrophone: false,
    });

    settings.questions.model = "claude-opus-5-5";
    settings.questions.includeMicrophone = true;
    expect(audioConfig(settings).questions).toEqual({
      model: "claude-opus-5-5",
      includeMicrophone: true,
    });

    settings.questions.enabled = false;
    expect(audioConfig(settings).questions).toBeNull();
  });

  it("sends live manual questions, retries and cancellation to the Rust answer engine", () => {
    const answers = fakeAnswers();
    const { created } = setup(answers);
    created.controller.setMode("live");
    created.controller.askManual("Qu'est-ce qu'une closure ?");
    created.controller.retry("q1");
    created.controller.cancelAll();
    expect(answers.askManual).toHaveBeenCalledWith("Qu'est-ce qu'une closure ?");
    expect(answers.retry).toHaveBeenCalledWith("q1");
    expect(answers.cancelAll).toHaveBeenCalled();
  });

  it("configures the answer model at startup and when it changes", async () => {
    const answers = fakeAnswers();
    const { settings } = setup(answers);
    expect(answers.configure).toHaveBeenCalledWith("claude-sonnet-5-5", true);
    settings.answers.model = "claude-haiku-5-5";
    await nextTick();
    expect(answers.configure).toHaveBeenLastCalledWith("claude-haiku-5-5", true);
    settings.answers.earlyStart = false;
    await nextTick();
    expect(answers.configure).toHaveBeenLastCalledWith("claude-haiku-5-5", false);
  });

  it("writes the answer streamed by Rust into the question's card", async () => {
    const { session, emitFromRust } = setup(fakeAnswers());
    await vi.waitFor(() => expect(session.status).toBe("idle"));
    const question = {
      id: "session-1-q1",
      sessionId: "session-1",
      source: "speech",
      sourceSegmentIds: ["system-1-0"],
      originalUtterance: "Comment tu gères le state dans Vue ?",
      normalizedQuestion: "Comment gères-tu le state dans Vue ?",
      intent: "TECHNICAL_QUESTION",
      language: "fr",
      domain: "frontend",
      contextRef: null,
      detectedAt: 1,
    };
    const ids = { questionId: question.id, requestId: "session-1-q1-a1" };
    emitFromRust({ type: "question.detected", question });
    emitFromRust({ type: "answer.started", ...ids });
    emitFromRust({ type: "answer.delta", ...ids, seq: 0, section: "primary", delta: "J'utilise " });
    emitFromRust({ type: "answer.delta", ...ids, seq: 1, section: "primary", delta: "Pinia." });
    const code = "```ts\nconst x = 1\n```";
    emitFromRust({ type: "answer.delta", ...ids, seq: 2, section: "details", delta: code });
    emitFromRust({ type: "answer.completed", ...ids });

    expect(session.cards).toHaveLength(1);
    const card = session.cards[0]!;
    expect(card.status).toBe("completed");
    expect(card.answer).toBe("J'utilise Pinia.");
    expect(card.details).toBe(code);
  });
});
