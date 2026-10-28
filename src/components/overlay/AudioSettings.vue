<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import Icon from "@/components/icons/Icon";
import { SOURCE_LABEL } from "@/components/cards/labels";
import { asAudioError, type AudioDevices, type DeviceInfo, type SourceKind } from "@/domain/audio";
import { audioBackend } from "@/platform/audioBackend";
import { useOverlaySettings } from "@/stores/overlaySettings";
import { useSessionStore } from "@/stores/session";
import QuestionSettings from "./QuestionSettings.vue";
import TranscriptionSettings from "./TranscriptionSettings.vue";

const settings = useOverlaySettings();
const store = useSessionStore();

const devices = ref<AudioDevices>({ outputs: [], inputs: [] });
const loading = ref(false);
const loadError = ref<string | null>(null);
/** Devices and mode can only change between sessions; mute stays available while running. */
const locked = computed(() => store.status !== "idle");

const SOURCES: Array<{ kind: SourceKind; hint: string; list: () => DeviceInfo[] }> = [
  {
    kind: "system",
    hint: "Ce que l'ordinateur joue (vos interlocuteurs), sans accès au micro.",
    list: () => devices.value.outputs,
  },
  {
    kind: "microphone",
    hint: "Votre voix. Désactivé par défaut.",
    list: () => devices.value.inputs,
  },
];

function defaultLabel(list: DeviceInfo[]): string {
  const current = list.find((d) => d.isDefault);
  return current ? `Par défaut — ${current.name}` : "Par défaut du système";
}

function isMissing(kind: SourceKind, list: DeviceInfo[]): boolean {
  const id = settings.audio[kind].deviceId;
  return id !== null && !list.some((d) => d.id === id);
}

async function refresh() {
  if (!audioBackend.available) return;
  loading.value = true;
  loadError.value = null;
  try {
    devices.value = await audioBackend.listDevices();
  } catch (error) {
    loadError.value = asAudioError(error).detail;
  } finally {
    loading.value = false;
  }
}

onMounted(refresh);
</script>

<template>
  <div class="group">
    <span class="label">Mode de session</span>
    <div class="segmented" role="radiogroup" aria-label="Mode de session">
      <button
        type="button"
        role="radio"
        :aria-checked="settings.sessionMode === 'demo'"
        :class="{ on: settings.sessionMode === 'demo' }"
        :disabled="locked"
        @click="settings.sessionMode = 'demo'"
      >
        Démo (simulée)
      </button>
      <button
        type="button"
        role="radio"
        :aria-checked="settings.sessionMode === 'live'"
        :class="{ on: settings.sessionMode === 'live' }"
        :disabled="locked || !audioBackend.available"
        @click="settings.sessionMode = 'live'"
      >
        Audio réel
      </button>
    </div>
    <p v-if="!audioBackend.available" class="hint">
      La capture audio n'est disponible que dans l'application desktop.
    </p>
    <p v-else-if="settings.sessionMode === 'live'" class="hint">
      La capture démarre uniquement avec « Démarrer ». Rien n'est enregistré sur le disque. Les
      réponses restent simulées jusqu'à l'arrivée du moteur de réponse (phase 5).
    </p>
  </div>

  <div v-if="settings.sessionMode === 'live' && audioBackend.available" class="group">
    <div class="row">
      <span class="label">Sources audio</span>
      <button type="button" class="btn small" :disabled="loading" @click="refresh">
        <Icon name="refresh" :size="12" /> Actualiser
      </button>
    </div>
    <p v-if="loadError" class="hint error">
      Liste des périphériques indisponible : {{ loadError }}
    </p>
    <p v-if="locked" class="hint">Arrêtez la session pour changer de source ou de périphérique.</p>

    <fieldset v-for="source in SOURCES" :key="source.kind" class="source" :disabled="locked">
      <label class="row check">
        <input v-model="settings.audio[source.kind].enabled" type="checkbox" />
        <span>{{ SOURCE_LABEL[source.kind] }}</span>
      </label>
      <p class="hint">{{ source.hint }}</p>
      <select
        v-model="settings.audio[source.kind].deviceId"
        class="device"
        :aria-label="`Périphérique : ${SOURCE_LABEL[source.kind]}`"
        :disabled="locked || !settings.audio[source.kind].enabled"
      >
        <option :value="null">{{ defaultLabel(source.list()) }}</option>
        <option v-for="device in source.list()" :key="device.id" :value="device.id">
          {{ device.name }}
        </option>
        <option
          v-if="isMissing(source.kind, source.list())"
          :value="settings.audio[source.kind].deviceId"
        >
          Périphérique enregistré non connecté
        </option>
      </select>
    </fieldset>
  </div>

  <TranscriptionSettings v-if="settings.sessionMode === 'live' && audioBackend.available" />
  <QuestionSettings v-if="settings.sessionMode === 'live' && audioBackend.available" />
</template>

<style scoped>
.group {
  padding: 10px 0;
  border-bottom: 1px solid var(--line);
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
.row .label {
  margin-bottom: 0;
}
.row.check {
  justify-content: flex-start;
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
.segmented button:disabled {
  cursor: default;
  opacity: 0.5;
}
.btn.small {
  height: 24px;
  padding: 0 8px;
  font-size: 11.5px;
}
.source {
  margin: 10px 0 0;
  padding: 0;
  border: 0;
}
.device {
  width: 100%;
  height: 30px;
  margin-top: 6px;
  padding: 0 8px;
  border: 1px solid var(--line-strong);
  border-radius: 7px;
  background: rgb(0 0 0 / 0.3);
  color: var(--text);
  font: inherit;
  font-size: 12.5px;
}
.device:disabled {
  opacity: 0.5;
}
.hint {
  margin: 4px 0 0;
  color: var(--text-3);
  font-size: 11.5px;
}
.hint.error {
  color: var(--danger);
}
</style>
