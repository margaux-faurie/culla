import { parseLog, renderLog } from "../decisions/codec";
import type { DecisionEvent } from "../decisions/events";
import { fold } from "../decisions/log";
import type { SourceStatus } from "./types";

/**
 * Sessions the web demo keeps between visits, so a folder sorted part-way can be resumed after a
 * reload (US-37, SPEC §4.2). Demo only: the app keeps sessions in SQLite under
 * `%LOCALAPPDATA%\Culla\sessions` (US-11). Pure, so the rules are tested without a browser.
 *
 * Each source keeps its decision log in the core's text form (`decisions/codec.ts`), "RAW follows
 * JPEG" as the session had it, the names of the photos applied by a closed plan, and the day of
 * that apply while no decision followed it.
 */

/** Where the demo keeps its sessions between visits. Demo only, display data only. */
export const SESSIONS_KEY = "culla-demo:sessions";

export interface StoredSession {
  /** The decision log, in order. */
  readonly events: readonly DecisionEvent[];
  readonly rawFollows: boolean;
  /** Photos a closed plan applied: a later plan holds them back as applied (INV-04). */
  readonly applied: readonly string[];
  /** The day the last plan was applied (`28 Nov`), until a new decision is made. */
  readonly appliedOn: string | null;
}

/** Reads the stored sessions back. Anything malformed counts as no session for that source. */
export function parseSessions(text: string | null): Map<string, StoredSession> {
  const sessions = new Map<string, StoredSession>();
  if (text === null) {
    return sessions;
  }
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    return sessions;
  }
  if (typeof value !== "object" || value === null) {
    return sessions;
  }
  for (const [sourceId, entry] of Object.entries(value)) {
    const stored = parseEntry(entry);
    if (stored !== null) {
      sessions.set(sourceId, stored);
    }
  }
  return sessions;
}

function parseEntry(entry: unknown): StoredSession | null {
  if (typeof entry !== "object" || entry === null) {
    return null;
  }
  const { log, rawFollows, applied, appliedOn } = entry as Record<string, unknown>;
  if (typeof log !== "string" || typeof rawFollows !== "boolean" || !Array.isArray(applied)) {
    return null;
  }
  let events: DecisionEvent[];
  try {
    events = parseLog(log);
  } catch {
    return null;
  }
  // A log that does not fold is corrupt; the demo starts that folder fresh (the app's handling of
  // a corrupt log is SPEC §10, decision log (d)).
  if (fold(events).corrupt !== undefined) {
    return null;
  }
  return {
    events,
    rawFollows,
    applied: applied.filter((name): name is string => typeof name === "string"),
    appliedOn: typeof appliedOn === "string" ? appliedOn : null,
  };
}

export function renderSessions(sessions: ReadonlyMap<string, StoredSession>): string {
  const out: Record<string, unknown> = {};
  for (const [sourceId, s] of sessions) {
    out[sourceId] = {
      log: renderLog(s.events),
      rawFollows: s.rawFollows,
      applied: s.applied,
      appliedOn: s.appliedOn,
    };
  }
  return JSON.stringify(out);
}

/** Photos with a decision once the log is folded. */
export const decidedCount = (events: readonly DecisionEvent[]): number =>
  fold(events).folded.state.size;

/**
 * Whether opening the folder resumes its session (proposed, SPEC §10 "Resume"): a session with
 * decision events and no apply since its last one. After an apply, the folder starts fresh, as
 * the demo's applied folder always has.
 */
export const resumable = (stored: StoredSession | undefined): stored is StoredSession =>
  stored !== undefined && stored.events.length > 0 && stored.appliedOn === null;

/** The recent folder's status from its stored session, or null to keep the one it has. */
export function storedStatus(stored: StoredSession | undefined): SourceStatus | null {
  if (stored === undefined) {
    return null;
  }
  if (stored.appliedOn !== null) {
    return { kind: "applied", on: stored.appliedOn };
  }
  if (stored.events.length > 0) {
    return { kind: "inProgress", decided: decidedCount(stored.events) };
  }
  return null;
}

/** `28 Nov`, as the recent folders show the day of an apply. */
export function appliedDay(date: Date): string {
  const months = [
    "Jan",
    "Feb",
    "Mar",
    "Apr",
    "May",
    "Jun",
    "Jul",
    "Aug",
    "Sep",
    "Oct",
    "Nov",
    "Dec",
  ];
  return `${date.getDate()} ${months[date.getMonth()] ?? ""}`;
}
