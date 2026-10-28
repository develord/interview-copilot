<script setup lang="ts">
import { computed, ref, useTemplateRef, watch } from "vue";
import AnswerCardView from "@/components/cards/AnswerCard.vue";
import AnswerCardList from "@/components/cards/AnswerCardList.vue";
import FocusView from "@/components/cards/FocusView.vue";
import ActiveQuestionPanel from "@/components/overlay/ActiveQuestionPanel.vue";
import AudioStatusStrip from "@/components/overlay/AudioStatusStrip.vue";
import ConsentDialog from "@/components/overlay/ConsentDialog.vue";
import OverlayFooter from "@/components/overlay/OverlayFooter.vue";
import OverlayHeader from "@/components/overlay/OverlayHeader.vue";
import SettingsPanel from "@/components/overlay/SettingsPanel.vue";
import TranscriptPanel from "@/components/overlay/TranscriptPanel.vue";
import { PROVIDER_LABEL } from "@/components/cards/labels";
import { useShortcuts } from "@/composables/useShortcuts";
import { inTauri, overlayWindow } from "@/platform/overlayWindow";
import { useSessionController } from "@/services/sessionController";
import { useOverlaySettings } from "@/stores/overlaySettings";
import { useSessionStore } from "@/stores/session";

const COMPACT_HEIGHT = 300;
const DEFAULT_HEIGHT = 680;
/** A confirmed "the other party is informed" stays valid for one interview-length window. */
const CONSENT_VALIDITY_MS = 30 * 60 * 1000;

const store = useSessionStore();
const settings = useOverlaySettings();
const controller = useSessionController();

const settingsOpen = ref(false);
const consentOpen = ref(false);
let consentConfirmedAt = Number.NEGATIVE_INFINITY;
const focusedCardId = ref<string | null>(null);
const footer = useTemplateRef<InstanceType<typeof OverlayFooter>>("footer");
const focusedCard = computed(() =>
  focusedCardId.value ? (store.cardState.cards[focusedCardId.value] ?? null) : null,
);

watch(
  () => settings.alwaysOnTop,
  (value) => void overlayWindow.setAlwaysOnTop(value),
  {
    immediate: true,
  },
);

// Compact mode shrinks the native window from its top edge, so its screen position never moves.
watch(
  () => settings.compact,
  async (compact) => {
    const size = await overlayWindow.logicalSize();
    if (!size) return;
    if (compact) {
      settings.expandedHeight = size.height;
      await overlayWindow.setLogicalSize(size.width, COMPACT_HEIGHT);
    } else {
      await overlayWindow.setLogicalSize(size.width, settings.expandedHeight ?? DEFAULT_HEIGHT);
    }
  },
);

// Capturing the other party's voice requires their informed consent (art. 226-1 Code pénal):
// live capture of system audio goes through a confirmation step first.
const consentRequired = computed(
  () => settings.sessionMode === "live" && settings.audio.system.enabled,
);
const transcribedBy = computed(() =>
  settings.transcription.enabled ? PROVIDER_LABEL[settings.transcription.provider] : null,
);

function requestStart() {
  if (consentRequired.value && Date.now() - consentConfirmedAt > CONSENT_VALIDITY_MS) {
    closeLayers();
    consentOpen.value = true;
    return;
  }
  controller.start();
}

function confirmConsent() {
  consentConfirmedAt = Date.now();
  consentOpen.value = false;
  controller.start();
}

function toggleCapture() {
  if (store.status === "idle") requestStart();
  else if (store.status === "paused") controller.resume();
  else controller.pause();
}

// Focus view, settings sheet and consent dialog are mutually exclusive layers, so Escape
// always closes the one that is visible.
function closeLayers() {
  focusedCardId.value = null;
  settingsOpen.value = false;
  consentOpen.value = false;
}

function openFocus(cardId: string | null) {
  closeLayers();
  focusedCardId.value = cardId;
}

function toggleSettings() {
  const open = !settingsOpen.value;
  closeLayers();
  settingsOpen.value = open;
}

function toggleFocusOnActive() {
  openFocus(focusedCardId.value ? null : (store.activeCard?.id ?? null));
}

function clearHistory() {
  controller.cancelAll();
  store.clearHistory();
  focusedCardId.value = null;
}

useShortcuts({
  "toggle-capture": toggleCapture,
  "submit-manual": () => footer.value?.focusOrSubmit(),
  "enlarge-active": toggleFocusOnActive,
  escape: closeLayers,
});
</script>

<template>
  <div
    class="overlay"
    :class="{ 'is-compact': settings.compact }"
    :style="{ '--surface-alpha': String(settings.opacity) }"
  >
    <div class="top">
      <OverlayHeader @toggle-settings="toggleSettings" @start="requestStart" />
      <AudioStatusStrip />
      <ActiveQuestionPanel />
    </div>

    <AnswerCardList v-if="!settings.compact" @enlarge="openFocus" />
    <div v-else class="compact-body">
      <AnswerCardView
        v-if="store.activeCard"
        :key="store.activeCard.id"
        :card="store.activeCard"
        variant="compact"
        @retry="controller.retry(store.activeCard.questionId)"
        @enlarge="toggleFocusOnActive"
      />
    </div>

    <TranscriptPanel />
    <OverlayFooter ref="footer" @clear="clearHistory" />

    <FocusView
      v-if="focusedCard"
      :card="focusedCard"
      @close="focusedCardId = null"
      @retry="controller.retry(focusedCard.questionId)"
    />
    <SettingsPanel v-if="settingsOpen" @close="settingsOpen = false" />
    <ConsentDialog
      v-if="consentOpen"
      :transcribed-by="transcribedBy"
      @confirm="confirmConsent"
      @cancel="consentOpen = false"
    />

    <button
      v-if="inTauri"
      type="button"
      class="resize-grip"
      aria-label="Redimensionner"
      title="Redimensionner"
      @mousedown.prevent="overlayWindow.startResize()"
    />
  </div>
</template>

<style scoped>
.overlay {
  position: relative;
  display: grid;
  grid-template-rows: auto minmax(0, 1fr) auto auto;
  height: 100%;
  overflow: hidden;
  border: 1px solid var(--line-strong);
  border-radius: var(--radius);
  background: rgb(var(--bg-rgb) / var(--surface-alpha));
}
.compact-body {
  min-height: 0;
  overflow-y: auto;
  padding: 8px 10px;
}
.resize-grip {
  position: absolute;
  right: 2px;
  bottom: 2px;
  z-index: 30;
  width: 14px;
  height: 14px;
  padding: 0;
  border: 0;
  background:
    linear-gradient(135deg, transparent 55%, var(--text-3) 55%, var(--text-3) 62%, transparent 62%),
    linear-gradient(135deg, transparent 75%, var(--text-3) 75%, var(--text-3) 82%, transparent 82%);
  cursor: se-resize;
  opacity: 0.6;
}
.resize-grip:hover {
  opacity: 1;
}
</style>
