<script setup lang="ts">
import { computed, ref, useTemplateRef } from "vue";
import Icon from "@/components/icons/Icon";
import {
  CLASSIFIER_STATE_LABEL,
  IGNORE_REASON_LABEL,
  INTENT_META,
  PROVIDER_LABEL,
  STT_STATE_LABEL,
} from "@/components/cards/labels";
import type { UtteranceReport } from "@/domain/intent";
import { formatSeconds } from "@/domain/latency";
import { useStickyScroll } from "@/composables/useStickyScroll";
import { useOverlaySettings } from "@/stores/overlaySettings";
import { useSessionStore } from "@/stores/session";

/**
 * Diagnostics view of the live transcript (Phase 3). One line per segment id: partial updates
 * rewrite the same line in place, finals are shown normally, partials dimmed.
 */
const store = useSessionStore();
const settings = useOverlaySettings();
const open = ref(true);

const lines = computed(() => store.transcriptLines.slice(-60));
const visible = computed(
  () =>
    settings.sessionMode === "live" &&
    (settings.transcription.enabled || store.transcriptLines.length > 0),
);
const summary = computed(() => {
  const statuses = Object.values(store.sttStatus);
  if (statuses.length === 0) return settings.transcription.enabled ? "prête" : "désactivée";
  const first = statuses[0]!;
  const latency = statuses
    .map((s) => s.lastFinalLatencyMs)
    .filter((ms): ms is number => ms !== null)
    .at(0);
  const state = STT_STATE_LABEL[first.state];
  const classifier = store.classifierStatus;
  const detection = classifier
    ? ` · questions : ${CLASSIFIER_STATE_LABEL[classifier.state]}${
        classifier.state === "ready" && classifier.lastLatencyMs !== null
          ? ` ≈${classifier.lastLatencyMs} ms`
          : ""
      }`
    : "";
  const answered =
    store.medianLatencyMs !== null ? ` · 1er mot ≈${formatSeconds(store.medianLatencyMs)}` : "";
  return `${PROVIDER_LABEL[first.provider]} · ${state}${latency !== undefined ? ` · ≈${latency} ms` : ""}${detection}${answered}`;
});

/** Short tag for the decision taken on the utterance that ends with this line. */
function tag(report: UtteranceReport): { text: string; kind: string; title: string } {
  const how =
    report.stage === "llm"
      ? `Claude${report.latencyMs !== null ? `, ${report.latencyMs} ms` : ""}`
      : report.stage === "rules"
        ? "règles"
        : "règles (Claude indisponible)";
  if (report.decision === "question") {
    const intent = report.intent ? INTENT_META[report.intent].label : "Question";
    return { text: `Question · ${intent}`, kind: "question", title: how };
  }
  if (report.decision === "revision") {
    return { text: "Précision de la question", kind: "revision", title: how };
  }
  const reason = report.reason ? IGNORE_REASON_LABEL[report.reason] : "ignoré";
  return { text: `Ignoré · ${reason}`, kind: "ignored", title: how };
}

const scroller = useTemplateRef<HTMLElement>("scroller");
const content = useTemplateRef<HTMLElement>("content");
useStickyScroll(
  scroller,
  content,
  computed(() => "bottom" as const),
  computed(() => lines.value.length),
);
</script>

<template>
  <section v-if="visible" class="transcript" aria-label="Transcription en direct">
    <button type="button" class="transcript-head" :aria-expanded="open" @click="open = !open">
      <span class="eyebrow">Transcription</span>
      <span class="summary">{{ summary }}</span>
      <span class="spacer" />
      <Icon :name="open ? 'arrow-down' : 'arrow-up'" :size="12" />
    </button>
    <div v-show="open" ref="scroller" class="transcript-body" role="log" aria-live="off">
      <div ref="content">
        <p v-if="lines.length === 0" class="empty">
          {{
            settings.transcription.enabled
              ? "Les phrases transcrites apparaîtront ici."
              : "Activez la transcription dans Réglages › Audio."
          }}
        </p>
        <p
          v-for="line in lines"
          :key="line.id"
          class="line"
          :class="{ partial: !line.isFinal }"
          :data-source="line.sourceId"
        >
          <span class="who">{{ line.sourceId === "system" ? "Interlocuteur" : "Vous" }}</span>
          <span class="text"
            >{{ line.text }}
            <span
              v-if="store.utteranceBySegment[line.id]"
              class="tag"
              :class="`tag-${tag(store.utteranceBySegment[line.id]!).kind}`"
              :title="tag(store.utteranceBySegment[line.id]!).title"
              >{{ tag(store.utteranceBySegment[line.id]!).text }}</span
            ></span
          >
        </p>
      </div>
    </div>
  </section>
</template>

<style scoped>
.transcript {
  border-top: 1px solid var(--line);
  background: rgb(0 0 0 / 0.16);
}
.transcript-head {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  height: 28px;
  padding: 0 12px;
  border: 0;
  background: transparent;
  color: var(--text-2);
}
.summary {
  overflow: hidden;
  color: var(--text-3);
  font-size: 11px;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.spacer {
  flex: 1;
}
.transcript-body {
  max-height: 110px;
  overflow-y: auto;
  padding: 0 12px 8px;
  overflow-anchor: auto;
}
.empty {
  margin: 2px 0;
  color: var(--text-3);
  font-size: 11.5px;
}
.line {
  display: flex;
  gap: 8px;
  margin: 2px 0;
  font-size: 12.5px;
  line-height: 1.4;
  overflow-wrap: anywhere;
}
.who {
  flex: none;
  width: 80px;
  color: var(--tone-teal);
  font-size: 11px;
  font-weight: 600;
}
.line[data-source="microphone"] .who {
  color: var(--tone-amber);
}
.line.partial .text {
  color: var(--text-3);
  font-style: italic;
}
.tag {
  display: inline-block;
  margin-left: 6px;
  padding: 0 6px;
  border-radius: 999px;
  font-size: 10.5px;
  font-style: normal;
  font-weight: 600;
  line-height: 16px;
  white-space: nowrap;
  vertical-align: 1px;
}
.tag-question {
  background: rgb(110 168 254 / 0.18);
  color: var(--accent);
}
.tag-revision {
  background: rgb(255 196 87 / 0.16);
  color: var(--warn);
}
.tag-ignored {
  background: rgb(255 255 255 / 0.06);
  color: var(--text-3);
}
</style>
