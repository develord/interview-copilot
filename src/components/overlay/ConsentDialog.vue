<script setup lang="ts">
import { ref } from "vue";
import Icon from "@/components/icons/Icon";

/**
 * Shown before live capture of the other party's voice. French law (art. 226-1 Code pénal)
 * punishes capturing or transmitting private words without their author's consent; consent
 * is presumed only when done openly. This is product guidance, not legal advice.
 */
defineProps<{ transcribedBy: string | null; analyzedBy: string | null }>();
defineEmits<{ confirm: []; cancel: [] }>();

const informed = ref(false);
const SCRIPT =
  "Pour m'aider à prendre des notes, j'utilise un assistant qui transcrit notre échange en direct. Rien n'est enregistré. Êtes-vous d'accord ?";
</script>

<template>
  <section class="consent" role="dialog" aria-modal="true" aria-labelledby="consent-title">
    <header class="consent-head">
      <h2 id="consent-title">Avant de démarrer</h2>
      <button type="button" class="icon-btn" title="Annuler (Échap)" @click="$emit('cancel')">
        <Icon name="close" />
      </button>
    </header>
    <p>
      La voix de votre interlocuteur va être captée<template v-if="transcribedBy">
        et transmise à <strong>{{ transcribedBy }}</strong> (serveurs UE) pour la
        transcription</template
      ><template v-if="analyzedBy"
        >, puis le texte transcrit analysé par <strong>{{ analyzedBy }}</strong> pour repérer les
        questions</template
      >. En France, capter ou transmettre des paroles privées sans le consentement de leur auteur
      est sanctionné (art. 226-1 du Code pénal). Informez votre interlocuteur et obtenez son accord.
    </p>
    <p class="label">Texte à lire ou à envoyer</p>
    <blockquote>{{ SCRIPT }}</blockquote>
    <label class="check">
      <input v-model="informed" type="checkbox" />
      <span>Mon interlocuteur est informé et d'accord</span>
    </label>
    <div class="actions">
      <button type="button" class="btn" @click="$emit('cancel')">Annuler</button>
      <button type="button" class="btn primary" :disabled="!informed" @click="$emit('confirm')">
        Démarrer la capture
      </button>
    </div>
    <p class="hint">
      Sans accord, désactivez « Son du système » dans les réglages : seul votre micro sera utilisé.
    </p>
  </section>
</template>

<style scoped>
.consent {
  position: absolute;
  inset: 42px 0 0;
  z-index: 25;
  overflow-y: auto;
  padding: 12px 16px 18px;
  background: rgb(var(--bg-rgb) / 0.985);
  font-size: 13px;
}
.consent-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
h2 {
  margin: 0;
  font-size: 15px;
}
p {
  margin: 8px 0;
  color: var(--text-2);
  line-height: 1.5;
}
.label {
  margin-bottom: 4px;
  color: var(--text-3);
  font-size: 11.5px;
  font-weight: 600;
}
blockquote {
  margin: 0;
  padding: 8px 10px;
  border-left: 3px solid var(--accent);
  border-radius: 4px;
  background: var(--raise);
  font-style: italic;
}
.check {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 12px 0;
  font-weight: 600;
}
input[type="checkbox"] {
  accent-color: var(--accent);
}
.actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
.btn:disabled {
  opacity: 0.45;
  cursor: default;
}
.hint {
  color: var(--text-3);
  font-size: 11.5px;
}
</style>
