<script setup lang="ts">
import type { AnswerCard } from "@/domain/cardLifecycle";
import Icon from "@/components/icons/Icon";
import AnswerCardView from "./AnswerCard.vue";

defineProps<{ card: AnswerCard }>();
defineEmits<{ close: []; retry: [] }>();
</script>

<template>
  <section class="focus" role="dialog" aria-label="Réponse agrandie">
    <header class="focus-head">
      <span class="eyebrow">Réponse agrandie</span>
      <button type="button" class="icon-btn" title="Fermer (Échap)" @click="$emit('close')">
        <Icon name="close" />
      </button>
    </header>
    <div class="focus-body">
      <AnswerCardView :card="card" variant="focus" @retry="$emit('retry')" />
    </div>
  </section>
</template>

<style scoped>
.focus {
  position: absolute;
  inset: 42px 0 0;
  z-index: 15;
  display: grid;
  grid-template-rows: auto minmax(0, 1fr);
  background: rgb(var(--bg-rgb) / 0.985);
  animation: focus-in 140ms ease-out;
}
@keyframes focus-in {
  from {
    opacity: 0;
  }
}
.focus-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 10px 4px 16px;
}
.focus-body {
  overflow-y: auto;
  padding: 4px 16px 20px;
}
</style>
