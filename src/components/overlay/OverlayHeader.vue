<script setup lang="ts">
import Icon from "@/components/icons/Icon";
import { SESSION_STATUS_LABEL } from "@/components/cards/labels";
import { inTauri, overlayWindow } from "@/platform/overlayWindow";
import { useSessionController } from "@/services/sessionController";
import { useOverlaySettings } from "@/stores/overlaySettings";
import { useSessionStore } from "@/stores/session";

defineEmits<{ "toggle-settings": []; start: [] }>();

const store = useSessionStore();
const settings = useOverlaySettings();
const controller = useSessionController();
</script>

<template>
  <!-- data-tauri-drag-region only applies to the element carrying it, so buttons stay clickable. -->
  <header class="bar" data-tauri-drag-region>
    <div class="status" data-tauri-drag-region>
      <span class="dot" :class="`dot-${store.status}`" aria-hidden="true" />
      <span class="status-label" data-tauri-drag-region>{{
        SESSION_STATUS_LABEL[store.status]
      }}</span>
      <span v-if="store.inFlightCount > 0" class="inflight" data-tauri-drag-region>
        · {{ store.inFlightCount }} en cours
      </span>
    </div>

    <div class="actions">
      <button
        v-if="store.status === 'idle'"
        type="button"
        class="btn primary"
        title="Démarrer la session (Ctrl+Shift+P)"
        @click="$emit('start')"
      >
        <Icon name="play" :size="13" /> Démarrer
      </button>
      <template v-else>
        <button
          v-if="store.status === 'paused'"
          type="button"
          class="icon-btn"
          title="Reprendre (Ctrl+Shift+P)"
          @click="controller.resume()"
        >
          <Icon name="play" />
        </button>
        <button
          v-else
          type="button"
          class="icon-btn"
          title="Pause (Ctrl+Shift+P)"
          @click="controller.pause()"
        >
          <Icon name="pause" />
        </button>
        <button type="button" class="icon-btn" title="Arrêter l'écoute" @click="controller.stop()">
          <Icon name="stop" />
        </button>
      </template>

      <span class="divider" />
      <button
        type="button"
        class="icon-btn"
        :title="settings.compact ? 'Mode étendu' : 'Mode compact'"
        @click="settings.compact = !settings.compact"
      >
        <Icon :name="settings.compact ? 'expand' : 'shrink'" />
      </button>
      <button type="button" class="icon-btn" title="Réglages" @click="$emit('toggle-settings')">
        <Icon name="settings" />
      </button>
      <template v-if="inTauri">
        <button type="button" class="icon-btn" title="Réduire" @click="overlayWindow.minimize()">
          <Icon name="minus" />
        </button>
        <button
          type="button"
          class="icon-btn danger"
          title="Quitter"
          @click="overlayWindow.close()"
        >
          <Icon name="close" />
        </button>
      </template>
    </div>
  </header>
</template>

<style scoped>
.bar {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 42px;
  padding: 0 8px 0 14px;
  border-bottom: 1px solid var(--line);
  user-select: none;
}
.status {
  display: flex;
  flex: 1;
  align-items: center;
  gap: 7px;
  min-width: 0;
  height: 100%;
  font-size: 12.5px;
  font-weight: 600;
}
.status-label,
.inflight {
  white-space: nowrap;
}
.inflight {
  color: var(--accent);
  font-weight: 500;
}
.dot {
  flex: none;
  width: 9px;
  height: 9px;
  border-radius: 50%;
  background: var(--text-3);
}
/* Capture indicator: clearly visible whenever the session is listening (PRD §11). */
.dot-listening {
  background: var(--live);
  box-shadow: 0 0 0 3px rgb(255 95 87 / 0.22);
  animation: live 1.6s ease-in-out infinite;
}
.dot-paused {
  background: var(--warn);
}
.dot-reconnecting {
  background: var(--warn);
  animation: live 0.8s ease-in-out infinite;
}
.dot-error {
  background: var(--danger);
}
@keyframes live {
  50% {
    box-shadow: 0 0 0 5px rgb(255 95 87 / 0.08);
  }
}
.actions {
  display: flex;
  align-items: center;
  gap: 2px;
}
.actions .btn {
  margin-right: 4px;
}
.divider {
  width: 1px;
  height: 18px;
  margin: 0 4px;
  background: var(--line-strong);
}
</style>
