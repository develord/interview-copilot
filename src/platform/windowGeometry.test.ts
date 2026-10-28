import { describe, expect, it } from "vitest";
import { computePresetPosition, nextMonitorIndex } from "./windowGeometry";

const area = { x: 0, y: 0, width: 1920, height: 1040 };
const size = { width: 440, height: 680 };

describe("computePresetPosition", () => {
  it("places the window in each corner of the work area", () => {
    expect(computePresetPosition("top-left", area, size, 16)).toEqual({ x: 16, y: 16 });
    expect(computePresetPosition("top-right", area, size, 16)).toEqual({ x: 1464, y: 16 });
    expect(computePresetPosition("bottom-left", area, size, 16)).toEqual({ x: 16, y: 344 });
    expect(computePresetPosition("bottom-right", area, size, 16)).toEqual({ x: 1464, y: 344 });
  });

  it("respects a secondary monitor's offset (including negative coordinates)", () => {
    const left = { x: -1280, y: 100, width: 1280, height: 984 };
    expect(computePresetPosition("top-right", left, size, 10)).toEqual({ x: -450, y: 110 });
  });

  it("never pushes the window past the top-left of a too-small area", () => {
    const tiny = { x: 0, y: 0, width: 300, height: 300 };
    expect(computePresetPosition("bottom-right", tiny, size, 8)).toEqual({ x: 8, y: 8 });
  });
});

describe("nextMonitorIndex", () => {
  const monitors = [
    { x: 0, y: 0, width: 1920, height: 1080 },
    { x: 1920, y: 0, width: 2560, height: 1440 },
  ];

  it("cycles to the next monitor and wraps around", () => {
    expect(nextMonitorIndex(monitors, monitors[0]!)).toBe(1);
    expect(nextMonitorIndex(monitors, monitors[1]!)).toBe(0);
  });

  it("falls back to the first monitor when the current one is unknown", () => {
    expect(nextMonitorIndex(monitors, null)).toBe(0);
    expect(nextMonitorIndex([], null)).toBe(-1);
  });
});
