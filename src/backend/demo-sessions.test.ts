import { describe, expect, it } from "vitest";
import type { DecisionEvent } from "../decisions/events";
import {
  appliedDay,
  decidedCount,
  parseSessions,
  renderSessions,
  resumable,
  type StoredSession,
  storedStatus,
} from "./demo-sessions";

const decided = (seq: number, group: number, verdict: "keep" | "reject"): DecisionEvent => ({
  seq,
  ts: 1000 + seq,
  cause: "user",
  type: "decided",
  group_id: group,
  verdict,
});

const session = (events: DecisionEvent[], appliedOn: string | null = null): StoredSession => ({
  events,
  rawFollows: false,
  applied: appliedOn === null ? [] : ["MGF_7001"],
  appliedOn,
});

describe("demo sessions (US-37)", () => {
  it("round-trips a session through its stored text, the log in the core's text form", () => {
    const events = [
      decided(1, 1, "keep"),
      decided(2, 2, "reject"),
      { seq: 3, ts: 1003, cause: "user", type: "undo", target_seq: 2 } as DecisionEvent,
    ];
    const text = renderSessions(new Map([["src-2026", session(events)]]));
    const back = parseSessions(text).get("src-2026");
    expect(back?.events).toEqual(events);
    expect(back?.rawFollows).toBe(false);
    expect(back?.appliedOn).toBeNull();
  });

  it("counts the photos decided once the log is folded, undos included", () => {
    const events = [
      decided(1, 1, "keep"),
      decided(2, 2, "reject"),
      { seq: 3, ts: 1003, cause: "user", type: "undo", target_seq: 2 } as DecisionEvent,
    ];
    expect(decidedCount(events)).toBe(1);
  });

  it("treats malformed or corrupt entries as no session, keeping the others", () => {
    expect(parseSessions(null).size).toBe(0);
    expect(parseSessions("not json").size).toBe(0);
    expect(parseSessions("[1]").size).toBe(0);
    const good = JSON.parse(renderSessions(new Map([["a", session([decided(1, 1, "keep")])]])));
    // Decided twice on the same photo does not fold: the log is corrupt.
    const corrupt = JSON.parse(
      renderSessions(new Map([["b", session([decided(1, 1, "keep"), decided(2, 1, "keep")])]])),
    );
    const text = JSON.stringify({
      ...good,
      ...corrupt,
      c: { log: "garbage\n", rawFollows: true, applied: [], appliedOn: null },
      d: { log: "", rawFollows: "yes", applied: [] },
    });
    expect([...parseSessions(text).keys()]).toEqual(["a"]);
  });

  it("resumes a folder with decisions and no apply since; an applied one starts fresh", () => {
    expect(resumable(undefined)).toBe(false);
    expect(resumable(session([]))).toBe(false);
    expect(resumable(session([decided(1, 1, "keep")]))).toBe(true);
    expect(resumable(session([decided(1, 1, "keep")], "28 Nov"))).toBe(false);
  });

  it("gives the recent folder its status: in progress with the count, or applied", () => {
    expect(storedStatus(undefined)).toBeNull();
    expect(storedStatus(session([]))).toBeNull();
    expect(storedStatus(session([decided(1, 1, "keep"), decided(2, 4, "reject")]))).toEqual({
      kind: "inProgress",
      decided: 2,
    });
    expect(storedStatus(session([decided(1, 1, "keep")], "28 Nov"))).toEqual({
      kind: "applied",
      on: "28 Nov",
    });
  });

  it("writes the day of an apply as the recent folders show it", () => {
    expect(appliedDay(new Date(2026, 10, 28, 15))).toBe("28 Nov");
    expect(appliedDay(new Date(2027, 0, 3))).toBe("3 Jan");
  });
});
