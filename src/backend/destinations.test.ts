import { describe, expect, it } from "vitest";
import {
  EMPTY_MEMORY,
  parseMemory,
  proposedDestinationId,
  remember,
  rememberKeepMode,
} from "./destinations";

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

  it("never finds a destination on an inherited key", () => {
    expect(proposedDestinationId(EMPTY_MEMORY, "constructor")).toBeNull();
    expect(proposedDestinationId(EMPTY_MEMORY, "__proto__")).toBeNull();
    const memory = remember(EMPTY_MEMORY, "__proto__", "d1");
    expect(proposedDestinationId(memory, "__proto__")).toEqual({ id: "d1", reason: "thisSource" });
    expect(proposedDestinationId(memory, "toString")).toEqual({ id: "d1", reason: "lastUsed" });
  });

  it("reads back what it stored and treats anything else as a first run", () => {
    const memory = remember(EMPTY_MEMORY, "s1", "d1");
    expect(parseMemory(JSON.stringify(memory))).toEqual(memory);
    expect(parseMemory(null)).toEqual(EMPTY_MEMORY);
    expect(parseMemory("not json")).toEqual(EMPTY_MEMORY);
    expect(parseMemory("[1]")).toEqual(EMPTY_MEMORY);
    expect(parseMemory('{"bySource":{"s1":4,"s2":"d2"},"lastUsed":7,"keepMode":"delete"}')).toEqual(
      { bySource: { s2: "d2" }, lastUsed: null, keepMode: null },
    );
    // A memory stored before move or copy was remembered reads as no last choice.
    expect(parseMemory('{"bySource":{},"lastUsed":"d1"}')).toEqual({
      bySource: {},
      lastUsed: "d1",
      keepMode: null,
    });
  });

  it("remembers move or copy as the last choice, apart from the destinations", () => {
    expect(EMPTY_MEMORY.keepMode).toBeNull();
    const memory = rememberKeepMode(remember(EMPTY_MEMORY, "s1", "d1"), "copy");
    expect(memory.keepMode).toBe("copy");
    expect(remember(memory, "s2", "d2").keepMode).toBe("copy");
    expect(proposedDestinationId(memory, "s1")).toEqual({ id: "d1", reason: "thisSource" });
    expect(parseMemory(JSON.stringify(memory))).toEqual(memory);
  });
});
