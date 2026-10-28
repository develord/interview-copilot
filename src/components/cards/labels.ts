import type { AudioErrorCode, SourceKind, SourceState } from "@/domain/audio";
import type { ProviderKind, SttErrorCode, SttState } from "@/domain/stt";
import type { AnswerStatus, Intent, Language, SessionStatus } from "@/domain/types";

export type Tone = "blue" | "violet" | "teal" | "amber" | "pink" | "green" | "slate";

export const INTENT_META: Record<Intent, { label: string; tone: Tone }> = {
  TECHNICAL_QUESTION: { label: "Technique", tone: "blue" },
  CODING_PROBLEM: { label: "Code", tone: "violet" },
  SYSTEM_DESIGN: { label: "System design", tone: "teal" },
  EXPERIENCE_QUESTION: { label: "Expérience", tone: "amber" },
  BEHAVIORAL_QUESTION: { label: "Comportemental", tone: "pink" },
  GREETING_OR_SMALL_TALK: { label: "Échange", tone: "green" },
  CLARIFICATION_OR_FOLLOW_UP: { label: "Relance", tone: "slate" },
  NON_QUESTION_STATEMENT: { label: "Affirmation", tone: "slate" },
  AMBIGUOUS: { label: "Ambigu", tone: "slate" },
};

export const LANGUAGE_LABEL: Record<Language, string> = { fr: "FR", en: "EN", mixed: "FR·EN" };

export const ANSWER_STATUS_LABEL: Record<AnswerStatus, string> = {
  pending: "Préparation",
  streaming: "Génération",
  completed: "Répondu",
  failed: "Échec",
};

export const SESSION_STATUS_LABEL: Record<SessionStatus, string> = {
  idle: "Inactif",
  listening: "À l'écoute",
  paused: "En pause",
  reconnecting: "Reconnexion…",
  error: "Erreur",
};

export function formatClock(timestamp: number): string {
  return new Date(timestamp).toLocaleTimeString("fr-FR", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

export const SOURCE_LABEL: Record<SourceKind, string> = {
  system: "Son du système",
  microphone: "Microphone",
};

export const SOURCE_STATE_LABEL: Record<SourceState, string> = {
  starting: "Démarrage…",
  capturing: "Capture",
  paused: "En pause",
  muted: "Coupé",
  recovering: "Reconnexion…",
  error: "Erreur",
  stopped: "Arrêté",
};

export const AUDIO_ERROR_LABEL: Record<AudioErrorCode, string> = {
  "permission-denied": "accès refusé par Windows (confidentialité du micro)",
  "device-not-found": "périphérique introuvable",
  "device-lost": "périphérique déconnecté",
  "device-busy": "périphérique utilisé par une autre application",
  "unsupported-format": "format audio non pris en charge",
  "no-source": "aucune source audio activée — activez-en une dans les réglages",
  "transcription-key-missing":
    "aucune clé API enregistrée pour la transcription — ajoutez-la dans les réglages",
  backend: "erreur du système audio",
};

export const PROVIDER_LABEL: Record<ProviderKind, string> = {
  assemblyai: "AssemblyAI",
  soniox: "Soniox",
};

export const STT_STATE_LABEL: Record<SttState, string> = {
  connecting: "connexion…",
  streaming: "transcription",
  reconnecting: "reconnexion…",
  error: "erreur",
  closed: "terminée",
};

export const STT_ERROR_LABEL: Record<SttErrorCode, string> = {
  "missing-key": "clé API manquante",
  unauthorized: "clé API refusée",
  quota: "crédit ou budget épuisé",
  network: "connexion perdue",
  protocol: "réponse inattendue du service",
  provider: "erreur du service de transcription",
};
