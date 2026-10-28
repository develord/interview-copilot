export type ScrollEdge = "top" | "bottom";

export interface ScrollMetrics {
  scrollTop: number;
  scrollHeight: number;
  clientHeight: number;
}

/** Pixels between the viewport and the edge where new cards appear. */
export function distanceFromEdge(metrics: ScrollMetrics, edge: ScrollEdge): number {
  return edge === "top"
    ? metrics.scrollTop
    : metrics.scrollHeight - metrics.clientHeight - metrics.scrollTop;
}

/**
 * A list is "pinned" when the reader is at the edge where new cards appear. Only a pinned list
 * follows new content; otherwise the reader's position is preserved.
 */
export function isPinnedToEdge(metrics: ScrollMetrics, edge: ScrollEdge, threshold = 32): boolean {
  return distanceFromEdge(metrics, edge) <= threshold;
}
