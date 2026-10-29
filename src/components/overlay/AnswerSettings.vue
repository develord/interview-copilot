<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import {
  ANSWER_MODEL_LABEL,
  formatClock,
  PROFILE_ERROR_LABEL,
  PROFILE_FIELD_LABEL,
} from "@/components/cards/labels";
import { ANSWER_MODELS } from "@/domain/answer";
import {
  asProfileError,
  emptyProfile,
  MAX_PDF_BYTES,
  PROFILE_FIELDS,
  readAsBase64,
  type Profile,
} from "@/domain/profile";
import { profileBackend } from "@/platform/answerBackend";
import { useOverlaySettings } from "@/stores/overlaySettings";

/**
 * Answer model and the verified profile. The CV is read by Claude once to prefill a draft;
 * nothing from it is used until the user has reviewed and saved the profile.
 */
const settings = useOverlaySettings();

const profile = ref<Profile | null>(null);
const draft = ref<Profile | null>(null);
const busy = ref<"extracting" | "saving" | null>(null);
const over = ref(false);
const confirmDelete = ref(false);
const message = ref<{ ok: boolean; text: string } | null>(null);
const model = computed(() => settings.answers.model);

onMounted(async () => {
  try {
    profile.value = await profileBackend.get();
  } catch (error) {
    message.value = { ok: false, text: PROFILE_ERROR_LABEL[asProfileError(error).code] };
  }
});

async function importFile(file: File | undefined) {
  over.value = false;
  if (!file) return;
  if (file.type !== "application/pdf" && !file.name.toLowerCase().endsWith(".pdf")) {
    message.value = { ok: false, text: "Ce fichier n'est pas un PDF." };
    return;
  }
  if (file.size > MAX_PDF_BYTES) {
    message.value = { ok: false, text: "Le PDF dépasse 10 Mo." };
    return;
  }
  busy.value = "extracting";
  message.value = null;
  try {
    const extracted = await profileBackend.extractPdf(await readAsBase64(file));
    // The targeted role is never in a CV: keep what the user already wrote.
    draft.value = { ...extracted, targetRole: profile.value?.targetRole ?? "" };
  } catch (error) {
    const e = asProfileError(error);
    message.value = { ok: false, text: `${PROFILE_ERROR_LABEL[e.code]}.` };
  } finally {
    busy.value = null;
  }
}

function onDrop(event: DragEvent) {
  void importFile(event.dataTransfer?.files[0]);
}

function onPick(event: Event) {
  const input = event.target as HTMLInputElement;
  void importFile(input.files?.[0]);
  input.value = "";
}

async function save() {
  if (!draft.value) return;
  busy.value = "saving";
  try {
    profile.value = await profileBackend.save(draft.value);
    draft.value = null;
    message.value = { ok: true, text: "Profil validé : les réponses s'appuient dessus." };
  } catch (error) {
    message.value = { ok: false, text: asProfileError(error).detail };
  } finally {
    busy.value = null;
  }
}

async function remove() {
  if (!confirmDelete.value) {
    confirmDelete.value = true;
    return;
  }
  confirmDelete.value = false;
  try {
    await profileBackend.clear();
    profile.value = null;
    message.value = { ok: true, text: "Profil supprimé." };
  } catch (error) {
    message.value = { ok: false, text: asProfileError(error).detail };
  }
}
</script>

<template>
  <fieldset class="group">
    <span class="label-inline">Réponses</span>
    <div class="segmented" role="radiogroup" aria-label="Modèle des réponses">
      <button
        v-for="kind in ANSWER_MODELS"
        :key="kind"
        type="button"
        role="radio"
        :aria-checked="model === kind"
        :class="{ on: model === kind }"
        @click="settings.answers.model = kind"
      >
        {{ ANSWER_MODEL_LABEL[kind].name }}
      </button>
    </div>
    <p class="hint">{{ ANSWER_MODEL_LABEL[model].hint }}</p>
    <label class="row check">
      <input v-model="settings.answers.earlyStart" type="checkbox" />
      <span>Réponse anticipée : commencer à écrire pendant la détection</span>
    </label>
    <p class="hint">
      Environ 1 s gagnée par question. Coûte un peu plus : une réponse est aussi commencée pour les
      phrases qui ne sont pas des questions (elle est arrêtée, jamais affichée).
    </p>
  </fieldset>

  <fieldset class="group" :disabled="busy !== null">
    <span class="label-inline">Profil vérifié</span>
    <p class="hint">
      Seule source de faits personnels dans les réponses (expériences, projets, chiffres). Ton CV
      est envoyé une fois à Claude (Anthropic) pour préremplir le profil ; tu relis tout avant de
      valider. Le profil reste sur ce PC.
    </p>

    <template v-if="draft">
      <p class="hint warn">Relis et corrige : rien n'est utilisé avant ta validation.</p>
      <label v-for="field in PROFILE_FIELDS" :key="field" class="stack">
        <span class="field-label">{{ PROFILE_FIELD_LABEL[field].label }}</span>
        <textarea
          v-model="draft[field]"
          class="field"
          :rows="PROFILE_FIELD_LABEL[field].rows"
          maxlength="8000"
          spellcheck="false"
        />
      </label>
      <div class="row actions">
        <button type="button" class="btn small" @click="draft = null">Annuler</button>
        <button type="button" class="btn small primary" @click="save">Valider le profil</button>
      </div>
    </template>

    <template v-else>
      <div v-if="profile" class="row saved">
        <span class="ok"
          >{{ profile.name || "Profil"
          }}<template v-if="profile.headline"> — {{ profile.headline }}</template></span
        >
        <span class="hint">validé à {{ formatClock(profile.verifiedAt ?? 0) }}</span>
      </div>
      <div v-if="profile" class="row actions">
        <button type="button" class="btn small" @click="draft = { ...profile }">Modifier</button>
        <button type="button" class="btn small" @click="remove">
          {{ confirmDelete ? "Confirmer la suppression" : "Supprimer" }}
        </button>
      </div>

      <div
        class="drop"
        :class="{ over }"
        @dragenter.prevent="over = true"
        @dragover.prevent="over = true"
        @dragleave="over = false"
        @drop.prevent="onDrop"
      >
        <template v-if="busy === 'extracting'">Lecture du CV par Claude… (20 à 60 s)</template>
        <template v-else>
          {{ profile ? "Glisse un autre CV (PDF) ici" : "Glisse ton CV (PDF) ici" }} ou
          <label class="link"
            >choisis un fichier<input type="file" accept="application/pdf,.pdf" @change="onPick"
          /></label>
        </template>
      </div>
      <button v-if="!profile" type="button" class="link small" @click="draft = emptyProfile()">
        ou remplir le profil à la main
      </button>
    </template>

    <p v-if="message" class="hint" :class="message.ok ? 'ok' : 'error'" role="status">
      {{ message.text }}
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
input[type="checkbox"] {
  accent-color: var(--accent);
}
.actions {
  justify-content: flex-end;
}
.saved {
  justify-content: space-between;
}
.label-inline {
  color: var(--text-2);
  font-size: 12px;
  font-weight: 600;
}
.stack {
  display: grid;
  gap: 3px;
  margin-top: 8px;
}
.field-label {
  color: var(--text-2);
  font-size: 11.5px;
}
.field {
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
  grid-template-columns: repeat(3, 1fr);
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
.drop {
  margin-top: 8px;
  padding: 14px 10px;
  border: 1px dashed var(--line-strong);
  border-radius: 8px;
  color: var(--text-2);
  font-size: 12px;
  text-align: center;
}
.drop.over {
  border-color: var(--accent);
  background: rgb(110 168 254 / 0.08);
}
.drop input[type="file"] {
  display: none;
}
.link {
  border: 0;
  padding: 0;
  background: none;
  color: var(--accent);
  font-size: 12px;
  font-weight: 600;
  text-decoration: underline;
  cursor: pointer;
}
.link.small {
  margin-top: 6px;
  font-size: 11.5px;
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
.warn {
  color: var(--warn);
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
