import type { CardEvent, DetectedQuestion } from "./events";
import type { AnswerSection, AnswerStatus, Intent, Language, QuestionSource } from "./types";

/**
 * Card lifecycle manager — the component that guarantees answers are never lost, mixed up or
 * overwritten. It is a set of pure functions mutating a plain `CardState` object, so it can be
 * unit-tested without Vue and wrapped in `reactive()` by the store for fine-grained updates
 * (mutating one card only re-renders that card).
 *
 * Rules (see docs/UI_STATE_MACHINE.md):
 *  1. A new question always creates a new card; other cards are never touched.
 *  2. Deltas are appended only to the card of the same question AND the same request.
 *  3. Completion is terminal: a completed answer is immutable, unless Rust restarts it for a
 *     revision (rule 4).
 *  4. Question revisions update the question while the answer is pending or streaming (Rust
 *     restarts it). Once it has ended they are recorded as corrections; if Rust still restarts
 *     the answer (the revision came within seconds of the question), the new attempt adopts
 *     the latest one.
 *  5. Follow-ups are new cards linked through `contextRef`.
 *  6. Concurrent generations are independent; a new `answer.started` with a different request
 *     id supersedes the previous request for that same question only.
 *  7. Failures keep the card (and any partial answer) and allow a retry.
 */

export interface AnswerCard {
  readonly id: string;
  readonly questionId: string;
  /** Current question text. Changes while the answer is unfinished, or on a restart (rule 4). */
  question: string;
  readonly originalUtterance: string;
  readonly intent: Intent;
  readonly language: Language;
  readonly source: QuestionSource;
  readonly contextRef: string | null;
  status: AnswerStatus;
  answer: string;
  details: string;
  error: { message: string; retryable: boolean } | null;
  /** Number of generation attempts started for this card (retries increment it). */
  attempt: number;
  /** Corrections received after the answer ended; shown, applied only by a new attempt. */
  questionRevisions: string[];
  readonly createdAt: number;
  updatedAt: number;
  completedAt: number | null;
}

/** Per-question stream bookkeeping. Kept outside the card so the UI never depends on it. */
interface StreamCursor {
  requestId: string | null;
  retiredRequestIds: string[];
  nextSeq: number;
  buffered: Record<number, { section: AnswerSection; delta: string }>;
}

export interface CardState {
  cards: Record<string, AnswerCard>;
  /** Card ids in creation order. Display order is decided by the view. */
  order: string[];
  cardIdByQuestion: Record<string, string>;
  streams: Record<string, StreamCursor>;
}

export type IgnoreReason =
  | "duplicate-question"
  | "unknown-question"
  | "stale-request"
  | "duplicate-start"
  | "duplicate-delta"
  | "already-terminal";

export type ApplyOutcome = { applied: true } | { applied: false; reason: IgnoreReason };

const APPLIED: ApplyOutcome = { applied: true };
const ignore = (reason: IgnoreReason): ApplyOutcome => ({ applied: false, reason });

export function createCardState(): CardState {
  return { cards: {}, order: [], cardIdByQuestion: {}, streams: {} };
}

export function clearCardState(state: CardState): void {
  state.cards = {};
  state.order = [];
  state.cardIdByQuestion = {};
  state.streams = {};
}

export function cardIdFor(questionId: string): string {
  return `card:${questionId}`;
}

export function getCardByQuestion(state: CardState, questionId: string): AnswerCard | undefined {
  const cardId = state.cardIdByQuestion[questionId];
  return cardId === undefined ? undefined : state.cards[cardId];
}

export function applyEvent(state: CardState, event: CardEvent, now: number): ApplyOutcome {
  if (event.type === "question.detected") return addQuestion(state, event.question, now);

  const card = getCardByQuestion(state, event.questionId);
  const stream = state.streams[event.questionId];
  if (!card || !stream) return ignore("unknown-question");

  switch (event.type) {
    case "question.revised":
      return reviseQuestion(card, event.text, now);
    case "answer.started":
      return startAttempt(card, stream, event.requestId, now);
    case "answer.delta": {
      const check = claimRequest(card, stream, event.requestId, now);
      if (!check.applied) return check;
      return acceptDelta(card, stream, event.seq, event.section, event.delta, now);
    }
    case "answer.completed": {
      const check = claimRequest(card, stream, event.requestId, now);
      if (!check.applied) return check;
      flushBuffered(card, stream);
      card.status = "completed";
      card.completedAt = now;
      card.updatedAt = now;
      return APPLIED;
    }
    case "answer.failed": {
      const check = claimRequest(card, stream, event.requestId, now);
      if (!check.applied) return check;
      flushBuffered(card, stream);
      card.status = "failed";
      card.error = { message: event.message, retryable: event.retryable };
      card.updatedAt = now;
      return APPLIED;
    }
  }
}

function addQuestion(state: CardState, question: DetectedQuestion, now: number): ApplyOutcome {
  if (state.cardIdByQuestion[question.id] !== undefined) return ignore("duplicate-question");

  const id = cardIdFor(question.id);
  state.cards[id] = {
    id,
    questionId: question.id,
    question: question.normalizedQuestion,
    originalUtterance: question.originalUtterance,
    intent: question.intent,
    language: question.language,
    source: question.source,
    contextRef: question.contextRef,
    status: "pending",
    answer: "",
    details: "",
    error: null,
    attempt: 0,
    questionRevisions: [],
    createdAt: now,
    updatedAt: now,
    completedAt: null,
  };
  state.cardIdByQuestion[question.id] = id;
  state.streams[question.id] = { requestId: null, retiredRequestIds: [], nextSeq: 0, buffered: {} };
  state.order.push(id);
  return APPLIED;
}

function reviseQuestion(card: AnswerCard, text: string, now: number): ApplyOutcome {
  if (card.status === "pending" || card.status === "streaming") {
    card.question = text;
  } else if (text !== card.question && !card.questionRevisions.includes(text)) {
    card.questionRevisions.push(text);
  }
  card.updatedAt = now;
  return APPLIED;
}

function startAttempt(
  card: AnswerCard,
  stream: StreamCursor,
  requestId: string,
  now: number,
): ApplyOutcome {
  if (stream.retiredRequestIds.includes(requestId)) return ignore("stale-request");
  if (stream.requestId === requestId) return ignore("duplicate-start");
  const revised = card.questionRevisions.at(-1);
  if (card.status === "completed" && revised === undefined) return ignore("already-terminal");

  if (stream.requestId !== null) stream.retiredRequestIds.push(stream.requestId);
  // A new attempt answers the latest question: Rust restarts with the revised text.
  if (revised !== undefined) {
    card.question = revised;
    card.questionRevisions = [];
  }
  stream.requestId = requestId;
  stream.nextSeq = 0;
  stream.buffered = {};

  card.status = "streaming";
  card.answer = "";
  card.details = "";
  card.error = null;
  card.attempt += 1;
  card.completedAt = null;
  card.updatedAt = now;
  return APPLIED;
}

/**
 * Verifies that an event belongs to the card's current request. A delta/completion arriving
 * before `answer.started` on a fresh card is treated as an implicit start (out-of-order delivery).
 */
function claimRequest(
  card: AnswerCard,
  stream: StreamCursor,
  requestId: string,
  now: number,
): ApplyOutcome {
  if (stream.retiredRequestIds.includes(requestId)) return ignore("stale-request");
  if (stream.requestId === null && card.status === "pending") {
    return startAttempt(card, stream, requestId, now);
  }
  if (stream.requestId !== requestId) return ignore("stale-request");
  if (card.status === "completed" || card.status === "failed") return ignore("already-terminal");
  return APPLIED;
}

function acceptDelta(
  card: AnswerCard,
  stream: StreamCursor,
  seq: number,
  section: AnswerSection,
  delta: string,
  now: number,
): ApplyOutcome {
  if (seq < stream.nextSeq || stream.buffered[seq] !== undefined) return ignore("duplicate-delta");

  if (seq > stream.nextSeq) {
    stream.buffered[seq] = { section, delta };
    return APPLIED;
  }

  appendSection(card, section, delta);
  stream.nextSeq += 1;
  for (let next = stream.buffered[stream.nextSeq]; next; next = stream.buffered[stream.nextSeq]) {
    appendSection(card, next.section, next.delta);
    delete stream.buffered[stream.nextSeq];
    stream.nextSeq += 1;
  }
  card.updatedAt = now;
  return APPLIED;
}

/** On a terminal event, append whatever is still buffered (in order) rather than dropping it. */
function flushBuffered(card: AnswerCard, stream: StreamCursor): void {
  const pending = Object.keys(stream.buffered)
    .map(Number)
    .sort((a, b) => a - b);
  for (const seq of pending) {
    const entry = stream.buffered[seq]!;
    appendSection(card, entry.section, entry.delta);
    stream.nextSeq = seq + 1;
  }
  stream.buffered = {};
}

function appendSection(card: AnswerCard, section: AnswerSection, delta: string): void {
  if (section === "primary") card.answer += delta;
  else card.details += delta;
}
