import type { SessionController } from "./sessionController";

export type SessionMode = "demo" | "live";

/**
 * Routes UI commands to the demo pipeline or the live audio session. Live mode only exists in
 * the desktop app; requesting it elsewhere falls back to the demo.
 */
export class SessionRouter implements SessionController {
  private mode: SessionMode;

  constructor(
    private readonly controllers: { demo: SessionController; live: SessionController | null },
    initial: SessionMode,
  ) {
    this.mode = this.resolve(initial);
  }

  get activeMode(): SessionMode {
    return this.mode;
  }

  /** Switching stops the current session first, so two capture paths never run together. */
  setMode(mode: SessionMode): void {
    const next = this.resolve(mode);
    if (next === this.mode) return;
    this.current.stop();
    this.mode = next;
  }

  start(): void {
    this.current.start();
  }
  pause(): void {
    this.current.pause();
  }
  resume(): void {
    this.current.resume();
  }
  stop(): void {
    this.current.stop();
  }
  askManual(text: string): void {
    this.current.askManual(text);
  }
  retry(questionId: string): void {
    this.current.retry(questionId);
  }
  cancelAll(): void {
    this.current.cancelAll();
  }
  dispose(): void {
    this.controllers.live?.dispose();
    this.controllers.demo.dispose();
  }

  private get current(): SessionController {
    return this.mode === "live" && this.controllers.live
      ? this.controllers.live
      : this.controllers.demo;
  }

  private resolve(mode: SessionMode): SessionMode {
    return mode === "live" && this.controllers.live ? "live" : "demo";
  }
}
