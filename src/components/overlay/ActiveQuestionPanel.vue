<script setup lang="ts">
import { computed } from "vue";
import { ANSWER_STATUS_LABEL, INTENT_META } from "@/components/cards/labels";
import { useOverlaySettings } from "@/stores/overlaySettings";
import { useSessionStore } from "@/stores/session";

const store = useSessionStore();
const settings = useOverlaySettings();
const phase = computed(() => store.activeCard?.status ?? null);
</script>

<template>
  <section class="active" aria-live="polite" aria-label="Question en cours">
    <div class="active-head">
      <span class="eyebrow">Question en cours</span>
      <span
        v-if="store.active"
        class="chip"
        :class="`tone-${INTENT_META[store.active.intent].tone}`"
      >
        {{ INTENT_META[store.active.intent].label }}
      </span>
      <span class="spacer" />
      <span v-if="phase" class="phase" :class="`phase-${phase}`">{{
        ANSWER_STATUS_LABEL[phase]
      }}</span>
    </div>
    <p v-if="store.active" class="active-question" :title="store.active.question">
      {{ store.active.question }}
    </p>
    <p v-else class="active-empty">
      {{
        store.status !== "listening"
          ? "Aucune question pour l'instant."
          : settings.sessionMode === "live"
            ? "À l'écoute… les questions de votre interlocuteur apparaîtront ici."
            : "À l'écoute… aucune question détectée."
      }}
    </p>
  </section>
</template>

<style scoped>
.active {
  padding: 9px 14px 10px;
  border-bottom: 1px solid var(--line);
  background: rgb(138 180 255 / 0.05);
}
.active-head {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 4px;
}
.spacer {
  flex: 1;
}
.phase {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-3);
}
.phase-pending,
.phase-streaming {
  color: var(--accent);
}
.phase-completed {
  color: var(--ok);
}
.phase-failed {
  color: var(--danger);
}
.active-question {
  margin: 0;
  font-size: 14.5px;
  font-weight: 600;
  line-height: 1.4;
  /* Pinned area stays compact: long questions are clamped, full text is in the card. */
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
  overflow-wrap: anywhere;
}
.active-empty {
  margin: 0;
  color: var(--text-3);
  font-size: 13px;
}
</style>
