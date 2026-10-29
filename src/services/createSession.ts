import { watch } from "vue";
import { asAudioError, type AudioConfig } from "@/domain/audio";
import { parseTerms } from "@/domain/stt";
import type { AnswerModel } from "@/domain/answer";
import type { answerBackend as AnswerBackend } from "@/platform/answerBackend";
import type { AudioBackend } from "@/platform/audioBackend";
import type { useOverlaySettings } from "@/stores/overlaySettings";
import type { useSessionStore } from "@/stores/session";
import { LiveAudioSession } from "./liveAudioSession";
import { MockPipeline } from "./mock/mockPipeline";
import { SessionRouter } from "./sessionRouter";

interface Dependencies {
  session: ReturnType<typeof useSessionStore>;
  settings: ReturnType<typeof useOverlaySettings>;
  /** `null` outside the desktop app: only the demo is available. */
  backend: AudioBackend | null;
  /** The Rust answer engine; required with `backend`. */
  answers?: Pick<typeof AnswerBackend, "configure" | "askManual" | "retry" | "cancelAll">;
}

/**
 * Assembles the session controller and its subscriptions: demo pipeline, live audio session,
 * Rust pipeline events → store, mode switching, live mute changes and the answer model.
 */
export function createSession({ session, settings, backend, answers }: Dependencies) {
  const demo = new MockPipeline({ emit: (event) => session.dispatch(event) });
  // Answer commands report through answer.* events; a rejected command (empty text) is moot.
  const ignore = () => {};
  const live =
    backend && answers
      ? new LiveAudioSession({
          backend,
          getConfig: () => audioConfig(settings),
          onCommandError: (error) => session.reportCommandError(error),
          answers: {
            askManual: (text) => void answers.askManual(text).catch(ignore),
            retry: (questionId) => void answers.retry(questionId).catch(ignore),
            cancelAll: () => void answers.cancelAll().catch(ignore),
          },
        })
      : null;
  const router = new SessionRouter({ demo, live }, settings.sessionMode);
  const cleanups: Array<() => void> = [];

  if (backend) {
    // A webview reload must never leave an invisible capture running in Rust.
    void backend.stop().catch(() => {});
    void backend
      .onPipelineEvent((payload) => session.dispatch(payload))
      .then((unlisten) => cleanups.push(unlisten));
  }
  if (answers) {
    cleanups.push(
      watch(
        () => [settings.answers.model, settings.answers.earlyStart] as const,
        ([model, earlyStart]: readonly [AnswerModel, boolean]) =>
          void answers.configure(model, earlyStart).catch(ignore),
        { immediate: true },
      ),
    );
  }

  cleanups.push(
    watch(
      () => settings.sessionMode,
      (mode) => router.setMode(mode),
    ),
  );

  if (backend) {
    for (const source of ["system", "microphone"] as const) {
      cleanups.push(
        watch(
          () => settings.audio[source].muted,
          (muted) => {
            if (router.activeMode !== "live" || session.status === "idle") return;
            backend
              .setMuted(source, muted)
              .catch((error: unknown) => session.reportCommandError(asAudioError(error)));
          },
        ),
      );
    }
  }

  return {
    controller: router,
    dispose() {
      cleanups.forEach((cleanup) => cleanup());
      router.dispose();
    },
  };
}

/** Fast endpoint: close a question after this much local silence (see AUDIO_PIPELINE.md). */
export const FAST_ENDPOINT_MS = 700;

export function audioConfig(settings: Dependencies["settings"]): AudioConfig {
  const t = settings.transcription;
  const q = settings.questions;
  return {
    system: settings.audio.system,
    microphone: settings.audio.microphone,
    transcription: t.enabled
      ? {
          provider: t.provider,
          extraTerms: parseTerms(t.extraTerms),
          forceEndpointMs: t.fastEndpoint ? FAST_ENDPOINT_MS : null,
          builtinVocabulary: t.builtinVocabulary,
        }
      : null,
    questions:
      t.enabled && q.enabled ? { model: q.model, includeMicrophone: q.includeMicrophone } : null,
  };
}
