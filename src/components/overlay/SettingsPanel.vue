<script setup lang="ts">
import { onMounted, ref } from "vue";
import Icon from "@/components/icons/Icon";
import AudioSettings from "./AudioSettings.vue";
import { inTauri, overlayWindow, type ShortcutBinding } from "@/platform/overlayWindow";
import { WINDOW_PRESETS, type WindowPreset } from "@/platform/windowGeometry";
import { useOverlaySettings } from "@/stores/overlaySettings";
import { useSessionStore } from "@/stores/session";

defineEmits<{ close: [] }>();

const settings = useOverlaySettings();
const store = useSessionStore();
const shortcuts = ref<ShortcutBinding[]>([]);
const lastPreset = ref<WindowPreset>("top-right");
const monitorNote = ref<string | null>(null);

const PRESET_LABEL: Record<WindowPreset, string> = {
  "top-left": "↖",
  "top-right": "↗",
  "bottom-left": "↙",
  "bottom-right": "↘",
};
const ACTION_LABEL: Record<ShortcutBinding["action"], string> = {
  "toggle-overlay": "Afficher / masquer",
  "toggle-capture": "Pause / reprise",
  "submit-manual": "Question manuelle",
  "enlarge-active": "Agrandir la réponse",
};

async function placeAt(preset: WindowPreset) {
  lastPreset.value = preset;
  await overlayWindow.moveToPreset(preset);
}

async function nextMonitor() {
  const moved = await overlayWindow.moveToNextMonitor(lastPreset.value);
  monitorNote.value = moved ? null : "Un seul écran détecté.";
}

onMounted(async () => {
  shortcuts.value = await overlayWindow.shortcutStatus();
});
</script>

<template>
  <section class="sheet" role="dialog" aria-label="Réglages">
    <header class="sheet-head">
      <h2>Réglages</h2>
      <button type="button" class="icon-btn" title="Fermer (Échap)" @click="$emit('close')">
        <Icon name="close" />
      </button>
    </header>

    <AudioSettings />

    <div class="group">
      <label class="row" for="opacity">
        <span>Opacité du fond</span>
        <output>{{ Math.round(settings.opacity * 100) }} %</output>
      </label>
      <input
        id="opacity"
        v-model.number="settings.opacity"
        type="range"
        min="0.4"
        max="1"
        step="0.02"
      />
    </div>

    <div class="group">
      <label class="row check">
        <input v-model="settings.latestOnly" type="checkbox" />
        <span>Afficher seulement la dernière question</span>
      </label>
    </div>

    <div v-if="!settings.latestOnly" class="group">
      <span class="label">Ordre des réponses</span>
      <div class="segmented" role="radiogroup">
        <button
          type="button"
          role="radio"
          :aria-checked="settings.order === 'newest-last'"
          :class="{ on: settings.order === 'newest-last' }"
          @click="settings.order = 'newest-last'"
        >
          Récentes en bas
        </button>
        <button
          type="button"
          role="radio"
          :aria-checked="settings.order === 'newest-first'"
          :class="{ on: settings.order === 'newest-first' }"
          @click="settings.order = 'newest-first'"
        >
          Récentes en haut
        </button>
      </div>
    </div>

    <div class="group">
      <label class="row check">
        <input v-model="settings.alwaysOnTop" type="checkbox" :disabled="!inTauri" />
        <span>Toujours au premier plan</span>
      </label>
    </div>

    <div class="group">
      <span class="label">Position de la fenêtre</span>
      <div class="row positions">
        <button
          v-for="preset in WINDOW_PRESETS"
          :key="preset"
          type="button"
          class="btn corner"
          :disabled="!inTauri"
          :title="preset"
          @click="placeAt(preset)"
        >
          {{ PRESET_LABEL[preset] }}
        </button>
        <button type="button" class="btn" :disabled="!inTauri" @click="nextMonitor">
          <Icon name="monitor" :size="13" /> Écran suivant
        </button>
      </div>
      <p v-if="monitorNote" class="hint">{{ monitorNote }}</p>
    </div>

    <div class="group">
      <span class="label">Raccourcis globaux</span>
      <p v-if="!inTauri" class="hint">
        Navigateur : Ctrl+Shift+P / Entrée / ↑ fonctionnent localement. Les raccourcis globaux sont
        actifs dans l'application desktop.
      </p>
      <ul v-else class="shortcuts">
        <li v-for="binding in shortcuts" :key="binding.action">
          <span>{{ ACTION_LABEL[binding.action] }}</span>
          <kbd v-if="binding.accelerator">{{ binding.accelerator }}</kbd>
          <span v-else class="conflict">indisponible</span>
          <span v-if="binding.error" class="conflict-note" :title="binding.error">{{
            binding.accelerator ? `(${binding.preferred} déjà pris)` : ""
          }}</span>
        </li>
      </ul>
    </div>

    <div class="group">
      <span class="label">Diagnostics du pipeline</span>
      <p class="hint mono">
        appliqués {{ store.diagnostics.applied }} · ignorés {{ store.diagnostics.ignored }} ·
        invalides {{ store.diagnostics.invalid }}
        <template v-if="store.diagnostics.lastIgnored">
          <br />dernier ignoré : {{ store.diagnostics.lastIgnored.type }} ({{
            store.diagnostics.lastIgnored.reason
          }})
        </template>
      </p>
      <p class="hint">Phase 1 : questions et réponses simulées — pas d'audio ni de LLM.</p>
    </div>
  </section>
</template>

<style scoped>
.sheet {
  position: absolute;
  inset: 42px 0 0;
  z-index: 20;
  overflow-y: auto;
  padding: 12px 16px 20px;
  background: rgb(var(--bg-rgb) / 0.98);
  animation: sheet-in 140ms ease-out;
}
@keyframes sheet-in {
  from {
    opacity: 0;
    transform: translateY(-4px);
  }
}
.sheet-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 6px;
}
h2 {
  margin: 0;
  font-size: 15px;
}
.group {
  padding: 10px 0;
  border-bottom: 1px solid var(--line);
}
.group:last-child {
  border-bottom: 0;
}
.label {
  display: block;
  margin-bottom: 8px;
  color: var(--text-2);
  font-size: 12px;
  font-weight: 600;
}
.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  font-size: 12.5px;
}
.row.check {
  justify-content: flex-start;
}
output {
  color: var(--text-2);
  font-variant-numeric: tabular-nums;
}
input[type="range"] {
  width: 100%;
  margin-top: 8px;
  accent-color: var(--accent);
}
input[type="checkbox"] {
  accent-color: var(--accent);
}
.segmented {
  display: grid;
  grid-template-columns: 1fr 1fr;
  padding: 3px;
  border-radius: 8px;
  background: rgb(0 0 0 / 0.3);
}
.segmented button {
  height: 28px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--text-2);
  font-size: 12px;
  font-weight: 600;
}
.segmented button.on {
  background: var(--raise-hover);
  color: var(--text);
}
.positions {
  justify-content: flex-start;
  flex-wrap: wrap;
}
.btn.corner {
  width: 32px;
  justify-content: center;
  padding: 0;
  font-size: 14px;
}
.btn:disabled {
  opacity: 0.4;
  cursor: default;
}
.hint {
  margin: 6px 0 0;
  color: var(--text-3);
  font-size: 11.5px;
}
.mono {
  font-family: var(--font-mono);
  font-size: 11px;
}
.shortcuts {
  margin: 0;
  padding: 0;
  list-style: none;
  font-size: 12.5px;
}
.shortcuts li {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 3px 0;
}
.shortcuts li > span:first-child {
  flex: 1;
}
kbd {
  padding: 1px 6px;
  border: 1px solid var(--line-strong);
  border-radius: 5px;
  background: rgb(0 0 0 / 0.3);
  font: 11px var(--font-mono);
}
.conflict {
  color: var(--danger);
  font-size: 11.5px;
}
.conflict-note {
  color: var(--warn);
  font-size: 11px;
}
</style>
