import { inject, type InjectionKey } from "vue";

/**
 * Commands the UI can send to the pipeline. The UI never talks to providers directly: in
 * Phase 1 this is implemented by the mock pipeline; later by a Tauri-backed controller that
 * forwards commands to the Rust orchestrator (where credentials live). Results always come
 * back as PipelineEvents through the session store, never as return values.
 */
export interface SessionController {
  start(): void;
  pause(): void;
  resume(): void;
  stop(): void;
  askManual(text: string): void;
  retry(questionId: string): void;
  /** Cancels all in-flight generations (used together with clearing the history). */
  cancelAll(): void;
  dispose(): void;
}

export const SESSION_CONTROLLER: InjectionKey<SessionController> = Symbol("SessionController");

export function useSessionController(): SessionController {
  const controller = inject(SESSION_CONTROLLER);
  if (!controller) throw new Error("SessionController was not provided");
  return controller;
}
