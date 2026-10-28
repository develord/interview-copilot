export const WINDOW_PRESETS = ["top-left", "top-right", "bottom-left", "bottom-right"] as const;
export type WindowPreset = (typeof WINDOW_PRESETS)[number];

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Top-left position that places a window in a corner of a monitor's work area (physical px). */
export function computePresetPosition(
  preset: WindowPreset,
  area: Rect,
  size: { width: number; height: number },
  margin: number,
): { x: number; y: number } {
  const left = area.x + margin;
  const top = area.y + margin;
  const right = area.x + area.width - size.width - margin;
  const bottom = area.y + area.height - size.height - margin;

  const x = preset.endsWith("left") ? left : Math.max(left, right);
  const y = preset.startsWith("top") ? top : Math.max(top, bottom);
  return { x: Math.round(x), y: Math.round(y) };
}

/** Index of the monitor after `current` (wrapping), or 0 when the current one is unknown. */
export function nextMonitorIndex(monitors: Rect[], current: Rect | null): number {
  if (monitors.length === 0) return -1;
  const index = current ? monitors.findIndex((m) => m.x === current.x && m.y === current.y) : -1;
  return (index + 1) % monitors.length;
}
