import { describe, expect, it } from "vitest";
import { EMPTY_MEMORY, parseMemory, proposedDestinationId, remember } from "./destinations";

describe("destination memory (SPEC §2, US-21)", () => {
  it("proposes nothing on a first run", () => {
    expect(proposedDestinationId(EMPTY_MEMORY, "s1")).toBeNull();
  });

  it("proposes the last destination used anywhere for a new folder", () => {
    const memory = remember(EMPTY_MEMORY, "s1", "d1");
    expect(proposedDestinationId(memory, "s2")).toEqual({ id: "d1", reason: "lastUsed" });
  });

  it("restores a folder's own destination when it comes back", () => {
    const memory = remember(remember(EMPTY_MEMORY, "s1", "d1"), "s2", "d2");
    expect(proposedDestinationId(memory, "s1")).toEqual({ id: "d1", reason: "thisSource" });
    expect(proposedDestinationId(memory, "s3")).toEqual({ id: "d2", reason: "lastUsed" });
  });

  it("reads back what it stored and treats anything else as a first run", () => {
    const memory = remember(EMPTY_MEMORY, "s1", "d1");
    expect(parseMemory(JSON.stringify(memory))).toEqual(memory);
    expect(parseMemory(null)).toEqual(EMPTY_MEMORY);
    expect(parseMemory("not json")).toEqual(EMPTY_MEMORY);
    expect(parseMemory("[1]")).toEqual({ bySource: {}, lastUsed: null });
    expect(parseMemory('{"bySource":{"s1":4,"s2":"d2"},"lastUsed":7}')).toEqual({
      bySource: { s2: "d2" },
      lastUsed: null,
    });
  });
});
