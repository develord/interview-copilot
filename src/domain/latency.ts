/**
 * End-to-end latency of a spoken question, from the arrival times of its pipeline events in
 * the webview (the IPC adds a few milliseconds). Three stages:
 *
 * - **transcription**: end of speech → final transcript, as measured by the STT session;
 * - **detection**: final transcript → card (waiting for a cut turn, then the classifier);
 * - **answer**: card → first word on screen (close to 0 when the early answer was kept).
 */
export interface LatencyMarks {
  /** End of speech → final transcript, from the `stt.status` that follows the final. */
  transcriptionMs: number | null;
  /** Arrival of the question's last final transcript; `null` for typed questions. */
  finalAt: number | null;
  detectedAt: number;
  /** Duration of the classifier call, when Claude decided. */
  classifyMs: number | null;
  firstWordAt: number | null;
}

export interface LatencyBreakdown {
  transcriptionMs: number | null;
  detectionMs: number | null;
  classifyMs: number | null;
  answerMs: number;
  /** End of speech → first word (the stages that are known). */
  totalMs: number;
}

export function breakdown(marks: LatencyMarks): LatencyBreakdown | null {
  if (marks.firstWordAt === null) return null;
  const answerMs = Math.max(0, marks.firstWordAt - marks.detectedAt);
  const detectionMs = marks.finalAt === null ? null : Math.max(0, marks.detectedAt - marks.finalAt);
  return {
    transcriptionMs: marks.transcriptionMs,
    detectionMs,
    classifyMs: marks.classifyMs,
    answerMs,
    totalMs: (marks.transcriptionMs ?? 0) + (detectionMs ?? 0) + answerMs,
  };
}

export function median(values: readonly number[]): number | null {
  if (values.length === 0) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 1 ? sorted[mid]! : (sorted[mid - 1]! + sorted[mid]!) / 2;
}

/** "1,2 s" (French decimal comma). */
export function formatSeconds(ms: number): string {
  return `${(ms / 1000).toFixed(1).replace(".", ",")} s`;
}

/** One line for a tooltip: every known stage. */
export function describeLatency(b: LatencyBreakdown): string {
  const parts: string[] = [];
  if (b.transcriptionMs !== null) parts.push(`transcription ${formatSeconds(b.transcriptionMs)}`);
  if (b.detectionMs !== null) {
    const claude = b.classifyMs !== null ? ` dont Claude ${formatSeconds(b.classifyMs)}` : "";
    parts.push(`détection ${formatSeconds(b.detectionMs)}${claude}`);
  }
  parts.push(`réponse ${formatSeconds(b.answerMs)}`);
  return `Premier mot ${formatSeconds(b.totalMs)} après la fin de la question : ${parts.join(" · ")}`;
}
