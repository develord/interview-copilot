<script setup lang="ts">
import { computed, ref } from "vue";
import { parseRichText } from "@/domain/richText";
import Icon from "@/components/icons/Icon";

/** Renders answer text as plain text nodes (never HTML): paragraphs, inline code, code blocks. */
const props = defineProps<{ text: string }>();
const blocks = computed(() => parseRichText(props.text));
const copiedIndex = ref<number | null>(null);

async function copy(code: string, index: number) {
  try {
    await navigator.clipboard.writeText(code);
    copiedIndex.value = index;
    setTimeout(() => {
      if (copiedIndex.value === index) copiedIndex.value = null;
    }, 1500);
  } catch {
    // Clipboard can be unavailable without a user gesture; copying is a convenience only.
  }
}
</script>

<template>
  <template v-for="(block, i) in blocks" :key="i">
    <p v-if="block.kind === 'text'" class="rt-p">
      <template v-for="(part, j) in block.parts" :key="j">
        <code v-if="part.code" class="rt-inline">{{ part.text }}</code>
        <template v-else>{{ part.text }}</template>
      </template>
    </p>
    <div v-else class="rt-code">
      <div class="rt-code-head">
        <span>{{ block.lang || "code" }}</span>
        <button
          v-if="block.closed"
          type="button"
          class="rt-copy"
          :title="copiedIndex === i ? 'Copié' : 'Copier le code'"
          @click="copy(block.code, i)"
        >
          <Icon name="copy" :size="12" />
          {{ copiedIndex === i ? "Copié" : "Copier" }}
        </button>
      </div>
      <pre><code>{{ block.code }}</code></pre>
    </div>
  </template>
</template>

<style scoped>
.rt-p {
  margin: 0 0 6px;
  white-space: pre-wrap;
}
.rt-p:last-child {
  margin-bottom: 0;
}
.rt-inline {
  padding: 1px 5px;
  border-radius: 5px;
  background: var(--code-bg);
  font-family: var(--font-mono);
  font-size: 0.88em;
}
.rt-code {
  margin: 8px 0;
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  background: var(--code-bg);
  overflow: hidden;
}
.rt-code-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 4px 8px 4px 10px;
  border-bottom: 1px solid var(--line);
  color: var(--text-3);
  font-size: 11px;
}
.rt-copy {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  border: 0;
  border-radius: 5px;
  padding: 2px 6px;
  background: transparent;
  color: var(--text-2);
  font-size: 11px;
}
.rt-copy:hover {
  background: var(--raise-hover);
}
pre {
  margin: 0;
  padding: 8px 10px;
  /* Long lines scroll inside the block; they never widen the card or the window. */
  overflow-x: auto;
  font: 12px/1.55 var(--font-mono);
  tab-size: 2;
}
</style>
