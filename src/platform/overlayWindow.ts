import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  availableMonitors,
  currentMonitor,
  getCurrentWindow,
  LogicalSize,
  PhysicalPosition,
  type Monitor,
} from "@tauri-apps/api/window";
import {
  computePresetPosition,
  nextMonitorIndex,
  type Rect,
  type WindowPreset,
} from "./windowGeometry";

/**
 * Thin wrapper over the native window. Every call is a no-op outside Tauri (plain browser dev
 * server, unit tests) and never throws: OS limitations are logged, not surfaced as crashes.
 */

export const inTauri = isTauri();

export type ShortcutAction =
  "toggle-overlay" | "toggle-capture" | "submit-manual" | "enlarge-active";

/** Mirrors `ShortcutBinding` in src-tauri/src/shortcuts.rs. */
export interface ShortcutBinding {
  action: ShortcutAction;
  preferred: string;
  accelerator: string | null;
  error: string | null;
}

const SHORTCUT_EVENT = "overlay:shortcut";
const MARGIN_LOGICAL_PX = 16;

async function guard<T>(fallback: T, run: () => Promise<T>): Promise<T> {
  if (!inTauri) return fallback;
  try {
    return await run();
  } catch (error) {
    console.warn("[overlay-window]", error);
    return fallback;
  }
}

const workArea = (m: Monitor): Rect => ({
  x: m.workArea.position.x,
  y: m.workArea.position.y,
  width: m.workArea.size.width,
  height: m.workArea.size.height,
});

async function placeOn(monitor: Monitor, preset: WindowPreset): Promise<void> {
  const win = getCurrentWindow();
  const size = await win.outerSize();
  const margin = Math.round(MARGIN_LOGICAL_PX * monitor.scaleFactor);
  const { x, y } = computePresetPosition(preset, workArea(monitor), size, margin);
  await win.setPosition(new PhysicalPosition(x, y));
}

export const overlayWindow = {
  setAlwaysOnTop: (value: boolean) =>
    guard(undefined, () => getCurrentWindow().setAlwaysOnTop(value)),

  minimize: () => guard(undefined, () => getCurrentWindow().minimize()),

  close: () => guard(undefined, () => getCurrentWindow().close()),

  startResize: () => guard(undefined, () => getCurrentWindow().startResizeDragging("SouthEast")),

  logicalSize: () =>
    guard(null, async () => {
      const win = getCurrentWindow();
      const [size, scale] = await Promise.all([win.innerSize(), win.scaleFactor()]);
      return size.toLogical(scale);
    }),

  setLogicalSize: (width: number, height: number) =>
    guard(undefined, () => getCurrentWindow().setSize(new LogicalSize(width, height))),

  moveToPreset: (preset: WindowPreset) =>
    guard(undefined, async () => {
      const monitor = await currentMonitor();
      if (monitor) await placeOn(monitor, preset);
    }),

  /** Moves the overlay to the same corner of the next monitor. Returns false if there is none. */
  moveToNextMonitor: (preset: WindowPreset) =>
    guard(false, async () => {
      const monitors = await availableMonitors();
      if (monitors.length < 2) return false;
      const current = await currentMonitor();
      const rect = (m: Monitor): Rect => ({ ...m.position, ...m.size });
      const target = monitors[nextMonitorIndex(monitors.map(rect), current ? rect(current) : null)];
      if (!target) return false;
      await placeOn(target, preset);
      return true;
    }),

  onShortcut: (handler: (action: ShortcutAction) => void) =>
    guard<() => void>(
      () => {},
      () => listen<ShortcutAction>(SHORTCUT_EVENT, (event) => handler(event.payload)),
    ),

  shortcutStatus: () => guard<ShortcutBinding[]>([], () => invoke("shortcut_status")),
};
