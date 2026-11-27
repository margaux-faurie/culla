import { describe, expect, it } from "vitest";
import { stateOf } from "../decisions/log";
import {
  atEnd,
  counts,
  currentPhoto,
  initialState,
  type SwipeAction,
  type SwipeState,
  swipeReducer,
} from "./session";

const photos = ["p1", "p2", "p3", "p4", "p5"];

function run(actions: SwipeAction[], start: SwipeState = initialState(photos)): SwipeState {
  return actions.reduce(swipeReducer, start);
}

const keep: SwipeAction = { type: "decide", verdict: "keep", now: 1 };
const reject: SwipeAction = { type: "decide", verdict: "reject", now: 1 };

describe("swipe session", () => {
  it("keeps or rejects the current photo, then shows the next one (US-22)", () => {
    const s = run([keep, reject]);
    expect(stateOf(s.folded, "p1")).toBe("kept");
    expect(stateOf(s.folded, "p2")).toBe("rejected");
    expect(currentPhoto(s)).toBe("p3");
    expect(s.log.map((e) => e.seq)).toEqual([1, 2]);
    expect(s.exit).toEqual({ photoId: "p2", verdict: "reject" });
    expect(s.announcement).toBe("Rejected, 2 of 5");
  });

  it("records five fast decisions in order (US-24)", () => {
    const s = run([keep, keep, reject, keep, reject]);
    expect(s.log.map((e) => (e.body.type === "decided" ? e.body.groupId : ""))).toEqual(photos);
    expect(atEnd(s)).toBe(true);
    expect(counts(s)).toEqual({ total: 5, kept: 3, rejected: 2, undecided: 0 });
  });

  it("does nothing on a decided photo but explains it (ADR 0006, US-26)", () => {
    const s = run([keep, { type: "prev" }, reject]);
    expect(currentPhoto(s)).toBe("p1");
    expect(stateOf(s.folded, "p1")).toBe("kept");
    expect(s.log).toHaveLength(1);
    expect(s.feedback).toEqual({ kind: "already-decided", photoId: "p1", state: "kept" });
    const again = swipeReducer(s, keep);
    expect(again.nonce).toBe(s.nonce + 1);
  });

  it("U makes the current photo undecided; then the arrows work again (US-27)", () => {
    const s = run([keep, { type: "prev" }, { type: "clear", now: 2 }, reject]);
    expect(stateOf(s.folded, "p1")).toBe("rejected");
    expect(s.log.map((e) => e.body.type)).toEqual(["decided", "cleared", "decided"]);
  });

  it("U on an undecided photo appends nothing", () => {
    const s = run([{ type: "clear", now: 1 }]);
    expect(s.log).toHaveLength(0);
    expect(s.feedback).toEqual({ kind: "already-undecided" });
  });

  it("Ctrl+Z undoes the last decision and jumps to it; Ctrl+Shift+Z redoes it (US-27)", () => {
    const decided = run([keep, keep, keep]);
    expect(currentPhoto(decided)).toBe("p4");
    const undone = swipeReducer(decided, { type: "undo", now: 2 });
    expect(currentPhoto(undone)).toBe("p3");
    expect(stateOf(undone.folded, "p3")).toBe("undecided");
    const further = swipeReducer(undone, { type: "undo", now: 3 });
    expect(currentPhoto(further)).toBe("p2");
    const redone = swipeReducer(further, { type: "redo", now: 4 });
    expect(currentPhoto(redone)).toBe("p2");
    expect(stateOf(redone.folded, "p2")).toBe("kept");
    expect(redone.log.map((e) => e.body.type)).toEqual([
      "decided",
      "decided",
      "decided",
      "undo",
      "undo",
      "redo",
    ]);
  });

  it("says when there is nothing to undo or redo", () => {
    expect(run([{ type: "undo", now: 1 }]).feedback).toEqual({ kind: "nothing-to-undo" });
    expect(run([keep, { type: "redo", now: 1 }]).feedback).toEqual({ kind: "nothing-to-redo" });
  });

  it("Shift+arrows browse without deciding (US-28)", () => {
    const s = run([{ type: "next" }, { type: "next" }, { type: "prev" }]);
    expect(currentPhoto(s)).toBe("p2");
    expect(s.log).toHaveLength(0);
    expect(run([{ type: "prev" }]).index).toBe(0);
  });

  it("N jumps to the next undecided photo, wrapping round", () => {
    const s = run([{ type: "next" }, keep, keep, { type: "prev" }, { type: "prev" }]);
    // p2 and p3 kept; on p2.
    expect(currentPhoto(s)).toBe("p2");
    expect(currentPhoto(swipeReducer(s, { type: "nextUndecided" }))).toBe("p4");
    const atLast = run([{ type: "next" }, { type: "next" }, { type: "next" }], s);
    expect(currentPhoto(atLast)).toBe("p5");
    expect(currentPhoto(swipeReducer(atLast, { type: "nextUndecided" }))).toBe("p1");
  });

  it("N with nothing undecided says so", () => {
    const s = run([keep, keep, keep, keep, keep, { type: "nextUndecided" }]);
    expect(s.feedback).toEqual({ kind: "no-undecided" });
  });

  it("F cycles all, undecided, kept, rejected, and browsing follows the filter (US-28)", () => {
    const filters = [1, 2, 3, 4].map(
      (n) => run(Array.from({ length: n }, () => ({ type: "cycleFilter" }) as const)).filter,
    );
    expect(filters).toEqual(["undecided", "kept", "rejected", "all"]);
    // p1 kept, p2 rejected, p3 kept; back to the start, then show kept only.
    const s = run([keep, reject, keep, { type: "prev" }, { type: "prev" }, { type: "prev" }]);
    const kept = run([{ type: "cycleFilter" }, { type: "cycleFilter" }], s);
    expect(kept.filter).toBe("kept");
    expect(currentPhoto(swipeReducer(kept, { type: "next" }))).toBe("p3");
  });

  it("the end card follows the last photo; N from there finds the first undecided (US-29)", () => {
    const s = run([{ type: "next" }, keep, keep, keep, keep]);
    expect(atEnd(s)).toBe(true);
    expect(currentPhoto(swipeReducer(s, { type: "nextUndecided" }))).toBe("p1");
    // Decisions on the end card do nothing.
    expect(swipeReducer(s, keep)).toBe(s);
  });

  it("an undo from the end card jumps back to the photo", () => {
    const s = run([keep, keep, keep, keep, keep, { type: "undo", now: 2 }]);
    expect(currentPhoto(s)).toBe("p5");
  });

  it("dismisses feedback", () => {
    const s = run([{ type: "undo", now: 1 }, { type: "dismissFeedback" }]);
    expect(s.feedback).toBeNull();
  });
});
