import { onBeforeUnmount, onMounted } from "vue";
import { inTauri, overlayWindow, type ShortcutAction } from "@/platform/overlayWindow";

export type OverlayAction = Exclude<ShortcutAction, "toggle-overlay">;
type Handlers = Record<OverlayAction, () => void> & { escape: () => void };

const LOCAL_BINDINGS: Array<{ action: OverlayAction; matches: (e: KeyboardEvent) => boolean }> = [
  { action: "toggle-capture", matches: (e) => e.code === "KeyP" },
  { action: "submit-manual", matches: (e) => e.key === "Enter" },
  { action: "enlarge-active", matches: (e) => e.key === "ArrowUp" },
];

/**
 * In the desktop app, shortcuts are global and registered natively (src-tauri/src/shortcuts.rs);
 * the Rust side forwards the actions the UI handles. In a plain browser (dev server) the same
 * Ctrl+Shift combinations are handled locally so the overlay can be exercised without Tauri.
 * Escape is always local: it must never be captured system-wide.
 */
export function useShortcuts(handlers: Handlers): void {
  let unlisten: (() => void) | null = null;
  let disposed = false;

  const onKeydown = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      handlers.escape();
      return;
    }
    if (inTauri || !event.ctrlKey || !event.shiftKey) return;
    const binding = LOCAL_BINDINGS.find((b) => b.matches(event));
    if (binding) {
      event.preventDefault();
      handlers[binding.action]();
    }
  };

  onMounted(async () => {
    window.addEventListener("keydown", onKeydown);
    const stop = await overlayWindow.onShortcut((action) => {
      if (action !== "toggle-overlay") handlers[action]();
    });
    if (disposed) stop();
    else unlisten = stop;
  });

  onBeforeUnmount(() => {
    disposed = true;
    window.removeEventListener("keydown", onKeydown);
    unlisten?.();
  });
}
