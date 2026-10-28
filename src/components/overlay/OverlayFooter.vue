<script setup lang="ts">
import { onBeforeUnmount, ref, useTemplateRef } from "vue";
import Icon from "@/components/icons/Icon";
import { useSessionController } from "@/services/sessionController";

const emit = defineEmits<{ clear: [] }>();
const controller = useSessionController();

const draft = ref("");
const input = useTemplateRef<HTMLInputElement>("input");
const confirmingClear = ref(false);
let confirmTimer: ReturnType<typeof setTimeout> | undefined;

function submit() {
  const text = draft.value.trim();
  if (!text) return;
  controller.askManual(text);
  draft.value = "";
}

/** Global shortcut target: submits a typed question, or focuses the input when it is empty. */
function focusOrSubmit() {
  if (draft.value.trim()) submit();
  else input.value?.focus();
}

/** Escape clears a draft first; on an empty field it bubbles up to close overlay panels. */
function onEscape(event: KeyboardEvent) {
  if (!draft.value) return;
  draft.value = "";
  event.stopPropagation();
}

/** Clearing the history is destructive, so it takes a second click within 3 seconds. */
function requestClear() {
  if (confirmingClear.value) {
    clearTimeout(confirmTimer);
    confirmingClear.value = false;
    emit("clear");
    return;
  }
  confirmingClear.value = true;
  confirmTimer = setTimeout(() => (confirmingClear.value = false), 3000);
}

onBeforeUnmount(() => clearTimeout(confirmTimer));
defineExpose({ focusOrSubmit });
</script>

<template>
  <form class="composer" @submit.prevent="submit">
    <input
      ref="input"
      v-model="draft"
      class="composer-input"
      type="text"
      maxlength="1000"
      placeholder="Poser une question… (Ctrl+Shift+Entrée)"
      aria-label="Question manuelle"
      @keydown.esc="onEscape"
    />
    <button type="submit" class="icon-btn send" :disabled="!draft.trim()" title="Envoyer">
      <Icon name="send" />
    </button>
    <button
      type="button"
      class="icon-btn"
      :class="{ confirming: confirmingClear }"
      :title="
        confirmingClear ? 'Cliquer à nouveau pour effacer l\'historique' : 'Effacer l\'historique'
      "
      @click="requestClear"
    >
      <Icon name="trash" />
      <span v-if="confirmingClear" class="confirm-label">Confirmer</span>
    </button>
  </form>
</template>

<style scoped>
.composer {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 8px 10px 10px;
  border-top: 1px solid var(--line);
}
.composer-input {
  flex: 1;
  min-width: 0;
  height: 32px;
  padding: 0 10px;
  border: 1px solid var(--line-strong);
  border-radius: 8px;
  background: rgb(0 0 0 / 0.25);
  outline: none;
  font-size: 13px;
}
.composer-input::placeholder {
  color: var(--text-3);
}
.composer-input:focus {
  border-color: rgb(138 180 255 / 0.6);
}
.send:not(:disabled) {
  color: var(--accent);
}
.icon-btn.confirming {
  width: auto;
  gap: 5px;
  padding: 0 8px;
  background: rgb(255 95 87 / 0.18);
  color: #ffd0cd;
}
.confirm-label {
  font-size: 12px;
  font-weight: 600;
}
</style>
