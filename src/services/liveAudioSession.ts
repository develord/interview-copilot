import { asAudioError, type AudioConfig, type AudioError } from "@/domain/audio";
import type { AudioBackend } from "@/platform/audioBackend";
import type { SessionController } from "./sessionController";

export interface LiveAudioSessionOptions {
  backend: Pick<AudioBackend, "start" | "pause" | "resume" | "stop">;
  /** Read at start time, so the latest source/device choices are used. */
  getConfig: () => AudioConfig;
  /** Command failures (e.g. nothing enabled) — capture state itself arrives as events. */
  onCommandError: (error: AudioError) => void;
  /** Answers are still simulated until the answer engine lands (Phase 5). */
  answers: Pick<SessionController, "askManual" | "retry" | "cancelAll">;
}

/**
 * Session controller backed by the Rust audio engine. Commands are fire-and-forget: the
 * resulting state (`session.status`, `audio.diagnostics`) is pushed back as pipeline events.
 */
export class LiveAudioSession implements SessionController {
  constructor(private readonly options: LiveAudioSessionOptions) {}

  start(): void {
    const { system, microphone, transcription } = this.options.getConfig();
    // Plain copies: the config may be reactive, and IPC must receive plain data.
    void this.run(() =>
      this.options.backend.start({
        system: { ...system },
        microphone: { ...microphone },
        transcription: transcription
          ? { ...transcription, extraTerms: [...transcription.extraTerms] }
          : null,
      }),
    );
  }

  pause(): void {
    void this.run(() => this.options.backend.pause());
  }

  resume(): void {
    void this.run(() => this.options.backend.resume());
  }

  stop(): void {
    void this.run(() => this.options.backend.stop());
  }

  askManual(text: string): void {
    this.options.answers.askManual(text);
  }

  retry(questionId: string): void {
    this.options.answers.retry(questionId);
  }

  cancelAll(): void {
    this.options.answers.cancelAll();
  }

  dispose(): void {
    void this.options.backend.stop().catch(() => {});
  }

  private async run(command: () => Promise<void>): Promise<void> {
    try {
      await command();
    } catch (error) {
      this.options.onCommandError(asAudioError(error));
    }
  }
}
