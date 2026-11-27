import { describe, expect, it } from "vitest";
import { dragOutcome } from "./drag";

describe("drag commit rule (US-25)", () => {
  it("commits past the offset threshold, right to keep and left to reject", () => {
    expect(dragOutcome(300, 0, 1000)).toBe("keep");
    expect(dragOutcome(-300, 0, 1000)).toBe("reject");
  });

  it("uses at least 120 px on a narrow card", () => {
    expect(dragOutcome(110, 0, 300)).toBeNull();
    expect(dragOutcome(120, 0, 300)).toBe("keep");
  });

  it("commits on a flick in the drag's direction", () => {
    expect(dragOutcome(40, 900, 1000)).toBe("keep");
    expect(dragOutcome(-40, -900, 1000)).toBe("reject");
  });

  it("springs back below both thresholds, on a flick against the drag, or on a tiny flick", () => {
    expect(dragOutcome(100, 100, 1000)).toBeNull();
    expect(dragOutcome(100, -900, 1000)).toBeNull();
    expect(dragOutcome(10, 2000, 1000)).toBeNull();
  });
});
