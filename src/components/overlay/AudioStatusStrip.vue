<script setup lang="ts">
import { computed } from "vue";
import Icon, { type IconName } from "@/components/icons/Icon";
import {
  AUDIO_ERROR_LABEL,
  CLASSIFY_ERROR_LABEL,
  PROVIDER_LABEL,
  SOURCE_LABEL,
  SOURCE_STATE_LABEL,
  STT_ERROR_LABEL,
} from "@/components/cards/labels";
import {
  asAudioError,
  meterPercent,
  type SourceDiagnostics,
  type SourceKind,
} from "@/domain/audio";
import { audioBackend } from "@/platform/audioBackend";
import { useOverlaySettings } from "@/stores/overlaySettings";
import { useSessionStore } from "@/stores/session";

/**
 * Live capture indicator (PRD §11: visible capture status, immediate control): one level meter
 * and state per source, a one-click mute, and errors with the action that fixes them.
 */
const store = useSessionStore();
const settings = useOverlaySettings();

const sources = computed(() => store.audioSources.filter((s) => s.state !== "stopped"));
const failing = computed(() => store.audioSources.filter((s) => s.error !== null));
const sttProblems = computed(() =>
  Object.values(store.sttStatus).filter((s) => s.error !== null && s.state !== "closed"),
);
/** Question detection running without the model, with the reason. */
const classifierProblem = computed(() => {
  const status = store.classifierStatus;
  return status && status.state !== "ready" && status.error ? status.error : null;
});
const visible = computed(
  () =>
    settings.sessionMode === "live" &&
    (store.status !== "idle" ||
      store.commandError !== null ||
      failing.value.length > 0 ||
      sttProblems.value.length > 0),
);

function stateLabel(d: SourceDiagnostics): string {
  if (d.state === "capturing" && !d.receiving) {
    return d.source === "system" ? "Aucun son joué" : "Silence";
  }
  return SOURCE_STATE_LABEL[d.state];
}

function icon(source: SourceKind): IconName {
  const muted = settings.audio[source].muted;
  if (source === "system") return muted ? "speaker-off" : "speaker";
  return muted ? "mic-off" : "mic";
}

function tooltip(d: SourceDiagnostics): string {
  const device = d.deviceName ?? "périphérique par défaut";
  const format = d.sampleRate ? ` · ${d.sampleRate / 1000} kHz, ${d.channels} canal(aux)` : "";
  const dropped = d.droppedFrames > 0 ? ` · ${d.droppedFrames} trames perdues` : "";
  return `${SOURCE_LABEL[d.source]} — ${device}${format}${dropped}`;
}

function toggleMute(source: SourceKind) {
  settings.audio[source].muted = !settings.audio[source].muted;
}

async function openPrivacySettings() {
  try {
    await audioBackend.openMicrophonePrivacySettings();
  } catch (error) {
    store.reportCommandError(asAudioError(error));
  }
}
</script>

<template>
  <section v-if="visible" class="audio-strip" aria-label="Capture audio">
    <div v-if="sources.length > 0" class="audio-sources">
      <div
        v-for="d in sources"
        :key="d.source"
        class="audio-source"
        :class="[`is-${d.state}`, { silent: !d.receiving }]"
        :title="tooltip(d)"
        :data-source="d.source"
      >
        <button
          type="button"
          class="icon-btn mute"
          :aria-pressed="settings.audio[d.source].muted"
          :title="
            settings.audio[d.source].muted
              ? `Réactiver : ${SOURCE_LABEL[d.source]}`
              : `Couper : ${SOURCE_LABEL[d.source]}`
          "
          @click="toggleMute(d.source)"
        >
          <Icon :name="icon(d.source)" :size="14" />
        </button>
        <div
          class="meter"
          role="meter"
          :aria-label="`Niveau : ${SOURCE_LABEL[d.source]}`"
          aria-valuemin="0"
          aria-valuemax="100"
          :aria-valuenow="meterPercent(d.levelDbfs)"
        >
          <span
            class="meter-fill"
            :class="{ clipping: d.peakDbfs > -1 }"
            :style="{ width: `${meterPercent(d.levelDbfs)}%` }"
          />
        </div>
        <span class="state">{{ stateLabel(d) }}</span>
      </div>
    </div>
    <p v-else-if="store.status !== 'idle'" class="audio-note">Démarrage de la capture…</p>

    <p v-if="store.commandError" class="audio-error" role="alert">
      <Icon name="alert" :size="13" />
      <span>Impossible de démarrer : {{ AUDIO_ERROR_LABEL[store.commandError.code] }}</span>
    </p>
    <p v-for="d in failing" :key="`error-${d.source}`" class="audio-error" role="alert">
      <Icon name="alert" :size="13" />
      <span :title="d.error!.detail">
        {{ SOURCE_LABEL[d.source] }} : {{ AUDIO_ERROR_LABEL[d.error!.code] }}
        <template v-if="d.state === 'recovering'"> — nouvelle tentative…</template>
      </span>
      <button
        v-if="d.error!.code === 'permission-denied'"
        type="button"
        class="link"
        @click="openPrivacySettings"
      >
        Ouvrir les paramètres
      </button>
    </p>
    <p v-for="s in sttProblems" :key="`stt-${s.source}`" class="audio-error" role="alert">
      <Icon name="alert" :size="13" />
      <span :title="s.error!.detail">
        Transcription ({{ SOURCE_LABEL[s.source] }}, {{ PROVIDER_LABEL[s.provider] }}) :
        {{ STT_ERROR_LABEL[s.error!.code] }}
        <template v-if="s.state === 'reconnecting'"> — reconnexion…</template>
      </span>
    </p>
    <p v-if="classifierProblem && store.status !== 'idle'" class="audio-error warn" role="status">
      <Icon name="alert" :size="13" />
      <span :title="classifierProblem.detail">
        Questions : {{ CLASSIFY_ERROR_LABEL[classifierProblem.code] }}
      </span>
    </p>
  </section>
</template>

<style scoped>
.audio-strip {
  padding: 6px 10px 7px 8px;
  border-bottom: 1px solid var(--line);
  background: rgb(0 0 0 / 0.18);
}
.audio-sources {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
  gap: 4px 12px;
}
.audio-source {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}
.icon-btn.mute {
  flex: none;
  width: 24px;
  height: 24px;
  color: var(--text);
}
.icon-btn.mute[aria-pressed="true"] {
  color: var(--warn);
}
.meter {
  position: relative;
  flex: 1;
  min-width: 40px;
  height: 5px;
  overflow: hidden;
  border-radius: 3px;
  background: rgb(255 255 255 / 0.08);
}
.meter-fill {
  position: absolute;
  inset: 0 auto 0 0;
  border-radius: 3px;
  background: var(--ok);
  transition: width 90ms linear;
}
.meter-fill.clipping {
  background: var(--danger);
}
.state {
  flex: none;
  color: var(--text-2);
  font-size: 11px;
  white-space: nowrap;
}
.is-capturing:not(.silent) .state {
  color: var(--ok);
}
.is-muted .state,
.is-paused .state,
.is-recovering .state {
  color: var(--warn);
}
.is-error .state {
  color: var(--danger);
}
.audio-note {
  margin: 2px 0;
  color: var(--text-3);
  font-size: 11.5px;
}
.audio-error {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 5px 0 0;
  color: #ffb4b0;
  font-size: 11.5px;
}
.audio-error span {
  flex: 1;
  min-width: 0;
}
.audio-error.warn {
  color: var(--warn);
}
.link {
  flex: none;
  border: 0;
  padding: 0;
  background: none;
  color: var(--accent);
  font-size: 11.5px;
  font-weight: 600;
  text-decoration: underline;
}
</style>
