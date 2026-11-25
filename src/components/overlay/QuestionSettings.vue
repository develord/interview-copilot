<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { CLASSIFY_ERROR_LABEL, CLAUDE_MODEL_LABEL } from "@/components/cards/labels";
import { asClassifyError, CLAUDE_MODELS } from "@/domain/intent";
import { llmBackend } from "@/platform/llmBackend";
import { useOverlaySettings } from "@/stores/overlaySettings";
import { useSessionStore } from "@/stores/session";

const settings = useOverlaySettings();
const store = useSessionStore();

const hasKey = ref(false);
const draft = ref("");
const busy = ref(false);
const message = ref<{ ok: boolean; text: string } | null>(null);
const locked = computed(() => store.status !== "idle");
const model = computed(() => settings.questions.model);

async function refreshKey() {
  try {
    hasKey.value = (await llmBackend.keyStatus()).anthropic;
  } catch (error) {
    message.value = { ok: false, text: CLASSIFY_ERROR_LABEL[asClassifyError(error).code] };
  }
}

async function saveKey() {
  busy.value = true;
  try {
    await llmBackend.setKey(draft.value);
    draft.value = "";
    message.value = {
      ok: true,
      text: "Clé enregistrée dans le Gestionnaire d'identification Windows.",
    };
    await refreshKey();
  } catch (error) {
    const e = asClassifyError(error);
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
    await llmBackend.clearKey();
    message.value = { ok: true, text: "Clé supprimée." };
    await refreshKey();
  } finally {
    busy.value = false;
  }
}

onMounted(refreshKey);
</script>

<template>
  <fieldset class="group" :disabled="locked">
    <label class="row check">
      <input v-model="settings.questions.enabled" type="checkbox" />
      <span class="label-inline">Détecter les questions</span>
    </label>
    <p v-if="!settings.transcription.enabled" class="hint">Nécessite la transcription en direct.</p>
    <p class="hint">
      Les phrases de l'interlocuteur et le contexte récent sont envoyés à Claude (Anthropic) pour
      repérer les questions. Sans clé, seules les questions explicites et les salutations sont
      détectées.
    </p>

    <div class="segmented" role="radiogroup" aria-label="Modèle Claude">
      <button
        v-for="kind in CLAUDE_MODELS"
        :key="kind"
        type="button"
        role="radio"
        :aria-checked="model === kind"
        :class="{ on: model === kind }"
        @click="settings.questions.model = kind"
      >
        {{ CLAUDE_MODEL_LABEL[kind].name }}
      </button>
    </div>
    <p class="hint">{{ CLAUDE_MODEL_LABEL[model].hint }}</p>

    <div v-if="hasKey" class="row key">
      <span class="ok">Clé Claude enregistrée</span>
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
        placeholder="Clé API Claude"
        aria-label="Clé API Claude"
      />
      <button type="submit" class="btn small" :disabled="busy || !draft.trim()">Enregistrer</button>
    </form>
    <p v-if="message" class="hint" :class="message.ok ? 'ok' : 'error'" role="status">
      {{ message.text }}
    </p>

    <label class="row check">
      <input v-model="settings.questions.technicalOnly" type="checkbox" />
      <span>Questions techniques uniquement</span>
    </label>
    <p class="hint">
      Salutations, parcours et questions comportementales sont ignorés : ni carte ni réponse.
    </p>

    <label class="row check">
      <input v-model="settings.questions.includeMicrophone" type="checkbox" />
      <span>Détecter aussi les questions dites au micro (pour tester seul)</span>
    </label>
    <p class="hint">
      Lire une réponse à voix haute, ou parler à partir d'elle, n'est pas une question.
    </p>
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
