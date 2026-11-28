import type { KeepMode } from "./types";

/**
 * Which destination and which move or copy a new session starts with (SPEC §2, "Keep"; US-21;
 * decided by Margaux 28 Nov 2026). Pure, so the rule is tested once and the demo and, later, the
 * Rust side follow the same table:
 *
 * - a folder culled before: the destination remembered for it ("same as last time");
 * - a new folder: the last destination used anywhere ("same as last time");
 * - a first run: none, the user chooses on the review screen;
 * - move or copy: the last choice, on any folder; none the first time.
 *
 * A remembered destination whose volume is missing is still proposed, marked not available, so the
 * review screen can say so and make the user choose.
 */

export interface DestinationMemory {
  /** Destination id per source id. */
  readonly bySource: Readonly<Record<string, string>>;
  /** The destination chosen or used last, on any source. */
  readonly lastUsed: string | null;
  /** Move or copy as last chosen, on any source. */
  readonly keepMode: KeepMode | null;
}

export const EMPTY_MEMORY: DestinationMemory = { bySource: {}, lastUsed: null, keepMode: null };

export function proposedDestinationId(
  memory: DestinationMemory,
  sourceId: string,
): { id: string; reason: "thisSource" | "lastUsed" } | null {
  // Own keys only: an id such as `constructor` or `__proto__` must not find an inherited value.
  const own = Object.hasOwn(memory.bySource, sourceId) ? memory.bySource[sourceId] : undefined;
  if (own !== undefined) {
    return { id: own, reason: "thisSource" };
  }
  if (memory.lastUsed !== null) {
    return { id: memory.lastUsed, reason: "lastUsed" };
  }
  return null;
}

export function remember(
  memory: DestinationMemory,
  sourceId: string,
  destinationId: string,
): DestinationMemory {
  return {
    ...memory,
    bySource: { ...memory.bySource, [sourceId]: destinationId },
    lastUsed: destinationId,
  };
}

export function rememberKeepMode(memory: DestinationMemory, keepMode: KeepMode): DestinationMemory {
  return { ...memory, keepMode };
}

/** Reads a stored memory back, treating anything malformed as a first run. */
export function parseMemory(text: string | null): DestinationMemory {
  if (text === null) {
    return EMPTY_MEMORY;
  }
  try {
    const value: unknown = JSON.parse(text);
    if (typeof value !== "object" || value === null) {
      return EMPTY_MEMORY;
    }
    const { bySource, lastUsed, keepMode } = value as Record<string, unknown>;
    const isText = (entry: [string, unknown]): entry is [string, string] =>
      typeof entry[1] === "string";
    const map = typeof bySource === "object" && bySource !== null ? bySource : {};
    const entries = Object.entries(map).filter(isText);
    return {
      bySource: Object.fromEntries(entries),
      lastUsed: typeof lastUsed === "string" ? lastUsed : null,
      keepMode: keepMode === "move" || keepMode === "copy" ? keepMode : null,
    };
  } catch {
    return EMPTY_MEMORY;
  }
}
