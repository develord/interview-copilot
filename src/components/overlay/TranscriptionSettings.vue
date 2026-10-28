<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { PROVIDER_LABEL, STT_ERROR_LABEL } from "@/components/cards/labels";
import { asSttError, PROVIDER_KINDS, type KeyStatus } from "@/domain/stt";
import { sttBackend } from "@/platform/sttBackend";
import { useOverlaySettings } from "@/stores/overlaySettings";
import { useSessionStore } from "@/stores/session";

const settings = useOverlaySettings();
const store = useSessionStore();

const keys = ref<KeyStatus>({ assemblyai: false, soniox: false });
const draft = ref("");
const busy = ref(false);
const message = ref<{ ok: boolean; text: string } | null>(null);
const locked = computed(() => store.status !== "idle");
const provider = computed(() => settings.transcription.provider);

const HINT: Record<(typeof PROVIDER_KINDS)[number], string> = {
  assemblyai: "Recommandé : meilleure précision mesurée en français mêlé d'anglais.",
  soniox: "Moins cher, n'enregistre rien et ne s'entraîne pas sur l'audio.",
};

async function refreshKeys() {
  try {
    keys.value = await sttBackend.keyStatus();
  } catch (error) {
    message.value = { ok: false, text: STT_ERROR_LABEL[asSttError(error).code] };
  }
}

async function saveKey() {
  busy.value = true;
  try {
    await sttBackend.setKey(provider.value, draft.value);
    draft.value = "";
    message.value = {
      ok: true,
      text: "Clé enregistrée dans le Gestionnaire d'identification Windows.",
    };
    await refreshKeys();
  } catch (error) {
    const e = asSttError(error);
    message.value = {
      ok: false,
      text: e.code === "unauthorized" ? "Ce texte ne ressemble pas à une clé API." : e.detail,
    };
  } finally {
    busy.value = false;
  }
}

async function removeKey() {
  busy.value = true;
  try {
    await sttBackend.clearKey(provider.value);
    message.value = { ok: true, text: "Clé supprimée." };
    await refreshKeys();
  } finally {
    busy.value = false;
  }
}

onMounted(refreshKeys);
</script>

<template>
  <fieldset class="group" :disabled="locked">
    <label class="row check">
      <input v-model="settings.transcription.enabled" type="checkbox" />
      <span class="label-inline">Transcrire en direct</span>
    </label>
    <p class="hint">
      L'audio capté est envoyé au service choisi (serveurs dans l'UE). L'application n'enregistre ni
      l'audio ni la clé en clair.
    </p>

    <div class="segmented" role="radiogroup" aria-label="Service de transcription">
      <button
        v-for="kind in PROVIDER_KINDS"
        :key="kind"
        type="button"
        role="radio"
        :aria-checked="provider === kind"
        :class="{ on: provider === kind }"
        @click="settings.transcription.provider = kind"
      >
        {{ PROVIDER_LABEL[kind] }}
      </button>
    </div>
    <p class="hint">{{ HINT[provider] }}</p>

    <div v-if="keys[provider]" class="row key">
      <span class="ok">Clé {{ PROVIDER_LABEL[provider] }} enregistrée</span>
      <button type="button" class="btn small" :disabled="busy" @click="removeKey">Supprimer</button>
    </div>
    <form v-else class="row key" @submit.prevent="saveKey">
      <input
        v-model="draft"
        class="field"
        type="password"
        autocomplete="off"
        spellcheck="false"
        maxlength="512"
        :placeholder="`Clé API ${PROVIDER_LABEL[provider]}`"
        :aria-label="`Clé API ${PROVIDER_LABEL[provider]}`"
      />
      <button type="submit" class="btn small" :disabled="busy || !draft.trim()">Enregistrer</button>
    </form>
    <p v-if="message" class="hint" :class="message.ok ? 'ok' : 'error'" role="status">
      {{ message.text }}
    </p>

    <label class="stack">
      <span class="label-inline">Termes de l'offre</span>
      <textarea
        v-model="settings.transcription.extraTerms"
        class="field"
        rows="2"
        maxlength="4000"
        placeholder="Spring Boot, Kafka, Datadog…"
      />
    </label>
    <label class="row check">
      <input v-model="settings.transcription.builtinVocabulary" type="checkbox" />
      <span>Vocabulaire technique intégré (~100 termes)</span>
    </label>
    <label class="row check">
      <input v-model="settings.transcription.fastEndpoint" type="checkbox" />
      <span>Clôturer une question après 0,7 s de silence</span>
    </label>
  </fieldset>
</template>

<style scoped>
.group {
  margin: 0;
  padding: 10px 0;
  border: 0;
  border-bottom: 1px solid var(--line);
}
.row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 6px;
  font-size: 12.5px;
}
.row.check {
  justify-content: flex-start;
}
.key {
  justify-content: space-between;
}
.label-inline {
  color: var(--text-2);
  font-size: 12px;
  font-weight: 600;
}
.stack {
  display: grid;
  gap: 4px;
  margin-top: 10px;
}
input[type="checkbox"] {
  accent-color: var(--accent);
}
.field {
  flex: 1;
  min-width: 0;
  padding: 6px 8px;
  border: 1px solid var(--line-strong);
  border-radius: 7px;
  background: rgb(0 0 0 / 0.3);
  color: var(--text);
  font: inherit;
  font-size: 12.5px;
  resize: vertical;
}
.segmented {
  display: grid;
  grid-template-columns: 1fr 1fr;
  margin-top: 8px;
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
.btn.small {
  height: 26px;
  padding: 0 9px;
  font-size: 11.5px;
}
.hint {
  margin: 4px 0 0;
  color: var(--text-3);
  font-size: 11.5px;
}
.ok {
  color: var(--ok);
}
.error {
  color: var(--danger);
}
fieldset:disabled {
  opacity: 0.6;
}
</style>
