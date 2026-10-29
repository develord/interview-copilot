<script setup lang="ts">
import { computed } from "vue";
import type { AnswerCard } from "@/domain/cardLifecycle";
import { describeLatency, formatSeconds, type LatencyBreakdown } from "@/domain/latency";
import Icon from "@/components/icons/Icon";
import RichText from "./RichText.vue";
import { ANSWER_STATUS_LABEL, INTENT_META, LANGUAGE_LABEL, formatClock } from "./labels";

const props = withDefaults(
  defineProps<{
    card: AnswerCard;
    /** Text of the question this card follows up on, if any. */
    contextQuestion?: string | null;
    /** How long the first word took, once known. */
    latency?: LatencyBreakdown | null;
    /** `compact` hides the question (shown in the active panel); `focus` is the enlarged view. */
    variant?: "default" | "compact" | "focus";
  }>(),
  { contextQuestion: null, latency: null, variant: "default" },
);
defineEmits<{ retry: []; enlarge: [] }>();

const intent = computed(() => INTENT_META[props.card.intent]);
const isLive = computed(() => props.card.status === "streaming");
const latestRevision = computed(() => props.card.questionRevisions.at(-1) ?? null);
</script>

<template>
  <article
    class="card"
    :class="[`is-${card.status}`, `variant-${variant}`]"
    :data-card-id="card.id"
    :aria-busy="card.status === 'pending' || card.status === 'streaming'"
  >
    <header class="card-head">
      <span class="chip" :class="`tone-${intent.tone}`">{{ intent.label }}</span>
      <span class="meta">{{ LANGUAGE_LABEL[card.language] }}</span>
      <span v-if="card.source === 'manual'" class="meta" title="Question saisie manuellement">
        <Icon name="keyboard" :size="13" />
      </span>
      <span class="meta status" :class="`status-${card.status}`">
        {{ ANSWER_STATUS_LABEL[card.status] }}
      </span>
      <span class="spacer" />
      <span v-if="latency" class="meta latency" :title="describeLatency(latency)">
        {{ formatSeconds(latency.totalMs) }}
      </span>
      <time class="meta" :datetime="new Date(card.createdAt).toISOString()">
        {{ formatClock(card.createdAt) }}
      </time>
      <button
        v-if="variant !== 'focus'"
        type="button"
        class="icon-btn small"
        title="Agrandir (Ctrl+Shift+↑)"
        @click="$emit('enlarge')"
      >
        <Icon name="expand" :size="13" />
      </button>
    </header>

    <p v-if="contextQuestion" class="context">
      <Icon name="follow-up" :size="12" />
      <span>Suite de : {{ contextQuestion }}</span>
    </p>

    <h3 v-if="variant !== 'compact'" class="question">{{ card.question }}</h3>

    <p v-if="latestRevision" class="revision">
      Transcription corrigée après réponse : « {{ latestRevision }} »
    </p>

    <div v-if="card.status === 'pending'" class="placeholder">
      <span class="pulse" /> Préparation de la réponse…
    </div>
    <div v-else-if="card.answer" class="answer">
      <RichText :text="card.answer" />
      <span v-if="isLive && !card.details" class="caret" aria-hidden="true" />
    </div>

    <details v-if="card.details" class="details" :open="variant === 'focus'">
      <summary>Détails</summary>
      <div class="details-body">
        <RichText :text="card.details" />
        <span v-if="isLive" class="caret" aria-hidden="true" />
      </div>
    </details>

    <footer v-if="card.status === 'failed'" class="failure" role="alert">
      <Icon name="alert" :size="14" />
      <span class="failure-msg">
        {{ card.error?.message ?? "La génération a échoué." }}
        <template v-if="card.answer">— réponse incomplète.</template>
      </span>
      <button v-if="card.error?.retryable" type="button" class="btn retry" @click="$emit('retry')">
        <Icon name="retry" :size="13" /> Réessayer
      </button>
    </footer>
  </article>
</template>

<style scoped>
.card {
  position: relative;
  min-width: 0;
  padding: 10px 12px 11px;
  border: 1px solid var(--line);
  border-radius: 11px;
  background: var(--raise);
  overflow-wrap: anywhere;
  animation: card-in 180ms ease-out;
}
.card.is-streaming,
.card.is-pending {
  border-color: rgb(138 180 255 / 0.28);
}
.card.is-failed {
  border-color: rgb(255 107 107 / 0.4);
}
.card.variant-focus {
  font-size: 15px;
  border: 0;
  background: transparent;
  padding: 0;
  animation: none;
}
@keyframes card-in {
  from {
    opacity: 0;
    transform: translateY(3px);
  }
}

.card-head {
  display: flex;
  align-items: center;
  gap: 6px;
  min-height: 22px;
  margin-bottom: 5px;
}
.meta {
  display: inline-flex;
  flex: none;
  align-items: center;
  color: var(--text-3);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}
/* On a narrow overlay the clock gives way first; labels never wrap. */
time.meta {
  flex: 0 1 auto;
  min-width: 0;
  overflow: hidden;
}
.latency {
  cursor: help;
}
.status-streaming,
.status-pending {
  color: var(--accent);
}
.status-failed {
  color: var(--danger);
}
.status-completed {
  color: var(--ok);
}
.spacer {
  flex: 1;
}
.icon-btn.small {
  width: 22px;
  height: 22px;
}

.context {
  display: flex;
  align-items: center;
  gap: 5px;
  margin: 0 0 4px;
  color: var(--text-3);
  font-size: 11.5px;
}
.context span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.question {
  margin: 0 0 7px;
  color: var(--text-2);
  font-size: 13px;
  font-weight: 600;
  line-height: 1.4;
}
.variant-focus .question {
  font-size: 15px;
}

.revision {
  margin: -2px 0 7px;
  padding: 4px 8px;
  border-left: 2px solid var(--warn);
  color: var(--warn);
  font-size: 11.5px;
}

.answer {
  font-size: 14px;
  line-height: 1.55;
}
.variant-focus .answer {
  font-size: 16px;
}

.placeholder {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--text-3);
  font-size: 12.5px;
}
.pulse {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--accent);
  animation: pulse 1s ease-in-out infinite;
}
@keyframes pulse {
  50% {
    opacity: 0.25;
  }
}

/* While streaming, the caret follows the last line of text instead of sitting below it. */
.answer :deep(.rt-p:has(+ .caret)),
.details-body :deep(.rt-p:has(+ .caret)) {
  display: inline;
}
.caret {
  display: inline-block;
  width: 7px;
  height: 14px;
  margin-left: 2px;
  vertical-align: -2px;
  border-radius: 2px;
  background: var(--accent);
  animation: blink 0.9s steps(2, start) infinite;
}
@keyframes blink {
  to {
    visibility: hidden;
  }
}

.details {
  margin-top: 8px;
  border-top: 1px solid var(--line);
  padding-top: 6px;
}
.details summary {
  width: fit-content;
  color: var(--text-2);
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  user-select: none;
}
.details-body {
  margin-top: 6px;
  font-size: 13px;
}

.failure {
  display: flex;
  align-items: center;
  gap: 7px;
  margin-top: 9px;
  padding-top: 8px;
  border-top: 1px solid rgb(255 107 107 / 0.25);
  color: #ffb4b0;
  font-size: 12px;
}
.failure-msg {
  flex: 1;
  min-width: 0;
}
.btn.retry {
  height: 26px;
  border-color: rgb(255 107 107 / 0.45);
  color: #ffd0cd;
}
</style>
