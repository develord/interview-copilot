import { nextTick, onBeforeUnmount, onMounted, ref, watch, type Ref } from "vue";
import { isPinnedToEdge, type ScrollEdge } from "./scrollPolicy";

/**
 * Keeps a scrollable card list stable while answers stream in:
 *  - pinned at the "new cards" edge → follow new content (including streaming growth);
 *  - reading older cards → never move; count unseen cards instead and offer a jump button.
 * Growth of cards above the viewport is compensated by the browser's native scroll anchoring
 * (`overflow-anchor`, supported by WebView2/Chromium).
 */
export function useStickyScroll(
  scroller: Readonly<Ref<HTMLElement | null>>,
  content: Readonly<Ref<HTMLElement | null>>,
  edge: Readonly<Ref<ScrollEdge>>,
  itemCount: Readonly<Ref<number>>,
) {
  const pinned = ref(true);
  const unseen = ref(0);
  let observer: ResizeObserver | null = null;

  function scrollToEdge(): void {
    const el = scroller.value;
    if (el) el.scrollTop = edge.value === "top" ? 0 : el.scrollHeight;
  }

  function onScroll(): void {
    const el = scroller.value;
    if (!el) return;
    pinned.value = isPinnedToEdge(el, edge.value);
    if (pinned.value) unseen.value = 0;
  }

  function jumpToEdge(): void {
    pinned.value = true;
    unseen.value = 0;
    scrollToEdge();
  }

  onMounted(() => {
    scroller.value?.addEventListener("scroll", onScroll, { passive: true });
    if (typeof ResizeObserver !== "undefined" && content.value) {
      observer = new ResizeObserver(() => {
        if (pinned.value) scrollToEdge();
      });
      observer.observe(content.value);
    }
    scrollToEdge();
  });

  onBeforeUnmount(() => {
    scroller.value?.removeEventListener("scroll", onScroll);
    observer?.disconnect();
  });

  watch(itemCount, (count, previous) => {
    if (count < previous) unseen.value = 0;
    else if (!pinned.value) unseen.value += count - previous;
    if (pinned.value) void nextTick(scrollToEdge);
  });

  watch(edge, () => void nextTick(jumpToEdge));

  return { pinned, unseen, jumpToEdge };
}
