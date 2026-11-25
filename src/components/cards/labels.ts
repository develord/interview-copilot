import type { AnswerModel } from "@/domain/answer";
import type { AudioErrorCode, SourceKind, SourceState } from "@/domain/audio";
import type {
  ClaudeModel,
  ClassifierState,
  ClassifyErrorCode,
  UtteranceIgnoreReason,
} from "@/domain/intent";
import type { ProfileErrorCode, ProfileField } from "@/domain/profile";
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

export const CLAUDE_MODEL_LABEL: Record<ClaudeModel, { name: string; hint: string }> = {
  "claude-haiku-5-5": {
    name: "Haiku 5.5",
    hint: "Recommandé : le plus rapide (≈ 1,2 s), environ 40 fois moins cher qu'Opus, 94 à 100 % de bonnes décisions sur nos dialogues de test.",
  },
  "claude-opus-5-5": {
    name: "Opus 5.5",
    hint: "Plus de jugement en théorie, mais plus lent (il réfléchit toujours avant de répondre) et plus cher.",
  },
};

export const CLASSIFIER_STATE_LABEL: Record<ClassifierState, string> = {
  ready: "Claude",
  "rules-only": "règles seules",
  degraded: "Claude indisponible, règles",
  error: "règles seules",
};

export const CLASSIFY_ERROR_LABEL: Record<ClassifyErrorCode, string> = {
  "missing-key": "pas de clé Claude : seules les questions explicites sont détectées",
  unauthorized: "clé Claude refusée : détection par règles seules",
  quota: "crédit Claude épuisé : détection par règles seules",
  "rate-limited": "Claude limite les requêtes : détection par règles en attendant",
  overloaded: "Claude surchargé : détection par règles en attendant",
  network: "Claude injoignable : détection par règles en attendant",
  timeout: "Claude trop lent : détection par règles en attendant",
  "invalid-response": "réponse inattendue de Claude : détection par règles",
  refused: "Claude a refusé d'analyser une phrase",
  protocol: "requête refusée par Claude (mise à jour nécessaire ?)",
  "credential-store": "Gestionnaire d'identification Windows indisponible",
};

export const IGNORE_REASON_LABEL: Record<UtteranceIgnoreReason, string> = {
  filler: "hésitation",
  backchannel: "acquiescement",
  statement: "affirmation",
  ambiguous: "en attente de contexte",
  repeat: "question répétée",
  "not-technical": "question non technique",
  "reading-answer": "lecture de la réponse",
};

export const ANSWER_MODEL_LABEL: Record<AnswerModel, { name: string; hint: string }> = {
  "claude-sonnet-5-5": {
    name: "Sonnet 5.5",
    hint: "Recommandé : bonnes réponses, rapides à démarrer.",
  },
  "claude-haiku-5-5": {
    name: "Haiku 5.5",
    hint: "Le plus rapide et le moins cher ; réponses plus simples.",
  },
  "claude-opus-5-5": {
    name: "Opus 5.5",
    hint: "Le plus fin, mais réfléchit avant d'écrire : la réponse démarre plus tard.",
  },
};

export const PROFILE_FIELD_LABEL: Record<ProfileField, { label: string; rows: number }> = {
  name: { label: "Nom", rows: 1 },
  headline: { label: "Titre", rows: 1 },
  summary: { label: "Résumé", rows: 3 },
  skills: { label: "Compétences", rows: 4 },
  experience: { label: "Expériences", rows: 8 },
  projects: { label: "Projets", rows: 5 },
  education: { label: "Formation", rows: 3 },
  languages: { label: "Langues", rows: 2 },
  targetRole: { label: "Poste visé (optionnel, ex. l'offre)", rows: 3 },
};

export const PROFILE_ERROR_LABEL: Record<ProfileErrorCode, string> = {
  invalid: "fichier refusé",
  storage: "impossible d'enregistrer le profil sur le disque",
  "missing-key": "ajoute d'abord ta clé Claude (Réglages › Audio › Détecter les questions)",
  extraction: "Claude n'a pas pu lire le CV",
};
