<script setup lang="ts">
import { computed, useTemplateRef } from "vue";
import Icon from "@/components/icons/Icon";
import { useStickyScroll } from "@/composables/useStickyScroll";
import { useSessionController } from "@/services/sessionController";
import { useOverlaySettings } from "@/stores/overlaySettings";
import { useSessionStore } from "@/stores/session";
import AnswerCardView from "./AnswerCard.vue";

const emit = defineEmits<{ enlarge: [cardId: string] }>();

const store = useSessionStore();
const settings = useOverlaySettings();
const controller = useSessionController();

const newestFirst = computed(() => settings.order === "newest-first");
const ordered = computed(() => (newestFirst.value ? [...store.cards].reverse() : store.cards));
const questionTextById = computed(
  () => new Map(store.cards.map((card) => [card.questionId, card.question])),
);

const scroller = useTemplateRef<HTMLElement>("scroller");
const content = useTemplateRef<HTMLElement>("content");
const { unseen, jumpToEdge } = useStickyScroll(
  scroller,
  content,
  computed(() => (newestFirst.value ? "top" : "bottom")),
  computed(() => store.cards.length),
);
</script>

<template>
  <div class="history">
    <div ref="scroller" class="history-scroller" tabindex="0" aria-label="Historique des réponses">
      <div ref="content" class="history-list" role="feed">
        <!-- Stable keys from card ids: a card's DOM and component instance survive every update. -->
        <AnswerCardView
          v-for="card in ordered"
          :key="card.id"
          :card="card"
          :context-question="
            card.contextRef ? (questionTextById.get(card.contextRef) ?? null) : null
          "
          @retry="controller.retry(card.questionId)"
          @enlarge="emit('enlarge', card.id)"
        />
        <p v-if="ordered.length === 0" class="history-empty">
          Les réponses apparaîtront ici — une carte par question détectée.
        </p>
      </div>
    </div>
    <button
      v-if="unseen > 0"
      type="button"
      class="jump-pill"
      :class="newestFirst ? 'at-top' : 'at-bottom'"
      @click="jumpToEdge"
    >
      <Icon :name="newestFirst ? 'arrow-up' : 'arrow-down'" :size="13" />
      {{ unseen }} {{ unseen > 1 ? "nouvelles réponses" : "nouvelle réponse" }}
    </button>
  </div>
</template>

<style scoped>
.history {
  position: relative;
  display: grid;
  min-height: 0;
}
.history-scroller {
  min-height: 0;
  overflow-x: hidden;
  overflow-y: auto;
  overscroll-behavior: contain;
  overflow-anchor: auto;
  scrollbar-gutter: stable;
  padding: 8px 6px 10px 10px;
  outline: none;
}
.history-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}
.history-empty {
  margin: 28px 16px;
  color: var(--text-3);
  font-size: 12.5px;
  text-align: center;
}
.jump-pill {
  position: absolute;
  left: 50%;
  transform: translateX(-50%);
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 28px;
  padding: 0 12px;
  border: 1px solid rgb(138 180 255 / 0.4);
  border-radius: 999px;
  background: #1b2436;
  color: var(--accent);
  font-size: 12px;
  font-weight: 600;
  box-shadow: 0 6px 18px rgb(0 0 0 / 0.4);
}
.jump-pill.at-bottom {
  bottom: 10px;
}
.jump-pill.at-top {
  top: 10px;
}
</style>
