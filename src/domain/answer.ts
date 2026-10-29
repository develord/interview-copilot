/** Claude models offered for answers; Sonnet 5.5 is the default (quality and speed). */
export const ANSWER_MODELS = ["claude-sonnet-5-5", "claude-haiku-5-5", "claude-opus-5-5"] as const;
export type AnswerModel = (typeof ANSWER_MODELS)[number];
