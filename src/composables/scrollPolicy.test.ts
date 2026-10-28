import { describe, expect, it } from "vitest";
import { distanceFromEdge, isPinnedToEdge } from "./scrollPolicy";

describe("scroll policy", () => {
  const atBottom = { scrollTop: 600, scrollHeight: 1000, clientHeight: 400 };
  const readingOlder = { scrollTop: 150, scrollHeight: 1000, clientHeight: 400 };
  const atTop = { scrollTop: 0, scrollHeight: 1000, clientHeight: 400 };

  it("measures the distance to the edge where new cards appear", () => {
    expect(distanceFromEdge(atBottom, "bottom")).toBe(0);
    expect(distanceFromEdge(readingOlder, "bottom")).toBe(450);
    expect(distanceFromEdge(readingOlder, "top")).toBe(150);
  });

  it("follows new content only when the reader is at that edge", () => {
    expect(isPinnedToEdge(atBottom, "bottom")).toBe(true);
    expect(isPinnedToEdge(readingOlder, "bottom")).toBe(false);
    expect(isPinnedToEdge(atTop, "top")).toBe(true);
    expect(isPinnedToEdge(readingOlder, "top")).toBe(false);
  });

  it("tolerates small offsets within the threshold", () => {
    expect(isPinnedToEdge({ ...atBottom, scrollTop: 575 }, "bottom")).toBe(true);
    expect(isPinnedToEdge({ ...atBottom, scrollTop: 560 }, "bottom")).toBe(false);
  });
});
