/**
 * Closed enumerations shared by every pipeline stage. Kept as `as const` tuples so the same
 * list drives both the TypeScript union and the runtime validator in `events.ts`.
 */

export const INTENTS = [
  "TECHNICAL_QUESTION",
  "CODING_PROBLEM",
  "SYSTEM_DESIGN",
  "EXPERIENCE_QUESTION",
  "BEHAVIORAL_QUESTION",
  "GREETING_OR_SMALL_TALK",
  "CLARIFICATION_OR_FOLLOW_UP",
  "NON_QUESTION_STATEMENT",
  "AMBIGUOUS",
] as const;
export type Intent = (typeof INTENTS)[number];

export const LANGUAGES = ["fr", "en", "mixed"] as const;
export type Language = (typeof LANGUAGES)[number];

export const ANSWER_STATUSES = ["pending", "streaming", "completed", "failed"] as const;
export type AnswerStatus = (typeof ANSWER_STATUSES)[number];

/** Where a question came from: live speech recognition or the overlay's text input. */
export const QUESTION_SOURCES = ["speech", "manual"] as const;
export type QuestionSource = (typeof QUESTION_SOURCES)[number];

/**
 * An answer streams into two independent sections: a short primary answer readable in a few
 * seconds, and optional details (code, complexity, trade-offs) shown in an expandable block.
 */
export const ANSWER_SECTIONS = ["primary", "details"] as const;
export type AnswerSection = (typeof ANSWER_SECTIONS)[number];

export const SESSION_STATUSES = ["idle", "listening", "paused", "reconnecting", "error"] as const;
export type SessionStatus = (typeof SESSION_STATUSES)[number];
