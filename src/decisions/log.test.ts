import { describe, expect, it } from "vitest";
import { type DecisionEvent, type EventBody, parseEvent } from "./events";
import { EMPTY, type Folded, fold, propose, stateOf, step, toExpected } from "./log";

const user = (seq: number, body: EventBody): DecisionEvent => ({
  seq,
  ts: seq,
  cause: "user",
  body,
});
const keep = (seq: number, groupId: string) =>
  user(seq, { type: "decided", groupId, verdict: "keep" });
const reject = (seq: number, groupId: string) =>
  user(seq, { type: "decided", groupId, verdict: "reject" });
const clear = (seq: number, groupId: string) => user(seq, { type: "cleared", groupId });
const undo = (seq: number, targetSeq: number) => user(seq, { type: "undo", targetSeq });
const redo = (seq: number, targetSeq: number) => user(seq, { type: "redo", targetSeq });

function folded(events: DecisionEvent[]): Folded {
  const result = fold(events);
  expect(result.corrupt).toBeUndefined();
  return result.folded;
}

describe("fold", () => {
  it("gives an empty state and empty stacks for an empty log", () => {
    expect(toExpected(folded([]))).toEqual({ state: {}, done: [], undone: [] });
  });

  it("records decisions and pushes them on done", () => {
    const f = folded([keep(1, "a"), reject(2, "b")]);
    expect(toExpected(f)).toEqual({
      state: { a: "kept", b: "rejected" },
      done: [1, 2],
      undone: [],
    });
  });

  it("undo reverts the top of done and moves it to undone; redo replays it", () => {
    const undone = folded([keep(1, "a"), reject(2, "b"), undo(3, 2)]);
    expect(toExpected(undone)).toEqual({ state: { a: "kept" }, done: [1], undone: [2] });
    const redone = folded([keep(1, "a"), reject(2, "b"), undo(3, 2), redo(4, 2)]);
    expect(toExpected(redone)).toEqual({
      state: { a: "kept", b: "rejected" },
      done: [1, 2],
      undone: [],
    });
  });

  it("undo and redo walk the stacks in order", () => {
    const f = folded([keep(1, "a"), keep(2, "b"), undo(3, 2), undo(4, 1), redo(5, 1)]);
    expect(toExpected(f)).toEqual({ state: { a: "kept" }, done: [1], undone: [2] });
  });

  it("U (cleared) is undoable and undo restores the earlier verdict", () => {
    const cleared = folded([reject(1, "a"), clear(2, "a")]);
    expect(stateOf(cleared, "a")).toBe("undecided");
    expect(cleared.done).toEqual([1, 2]);
    const restored = folded([reject(1, "a"), clear(2, "a"), undo(3, 2)]);
    expect(stateOf(restored, "a")).toBe("rejected");
    expect(restored.undone).toEqual([2]);
  });

  it("a new decision or cleared empties the redo stack", () => {
    expect(folded([keep(1, "a"), undo(2, 1), keep(3, "b")]).undone).toEqual([]);
    expect(folded([keep(1, "a"), keep(2, "b"), undo(3, 2), clear(4, "a")]).undone).toEqual([]);
  });

  it("a reconcile cleared leaves both stacks and cannot be undone (ADR 0024)", () => {
    const reconcile = (seq: number, groupId: string): DecisionEvent => ({
      seq,
      ts: seq,
      cause: "reconcile",
      body: { type: "cleared", groupId },
    });
    const after = folded([keep(1, "a"), keep(2, "b"), keep(3, "c"), reconcile(4, "a")]);
    expect(toExpected(after)).toEqual({
      state: { b: "kept", c: "kept" },
      done: [2, 3],
      undone: [],
    });

    const both = folded([keep(1, "a"), clear(2, "a"), undo(3, 2)]);
    expect([both.done, both.undone]).toEqual([[1], [2]]);
    const dropped = folded([keep(1, "a"), clear(2, "a"), undo(3, 2), reconcile(4, "a")]);
    expect(toExpected(dropped)).toEqual({ state: {}, done: [], undone: [] });
    expect(propose(dropped, { type: "undo" })).toEqual({ ok: false, refusal: "nothing-to-undo" });
  });

  it("stops at an event that breaks a precondition and reports it", () => {
    const cases: DecisionEvent[][] = [
      [keep(1, "a"), keep(2, "a")],
      [clear(1, "a")],
      [keep(1, "a"), undo(2, 7)],
      [keep(1, "a"), redo(2, 1)],
      [keep(2, "a"), keep(1, "b")],
      [keep(1, "a"), { ...keep(2, "b"), cause: "reconcile" }],
    ];
    for (const events of cases) {
      const result = fold(events);
      expect(result.corrupt?.seq).toBe(events.at(-1)?.seq);
      expect(result.folded.lastSeq).toBe(events.length === 1 ? 0 : (events[0]?.seq ?? 0));
    }
  });

  it("any prefix of a valid log folds without error", () => {
    const events = [
      keep(1, "a"),
      reject(2, "b"),
      undo(3, 2),
      clear(4, "a"),
      undo(5, 4),
      redo(6, 4),
    ];
    for (let n = 0; n <= events.length; n += 1) {
      expect(fold(events.slice(0, n)).corrupt).toBeUndefined();
    }
  });
});

describe("propose", () => {
  it("refuses to decide a decided group (undo first, ADR 0006)", () => {
    const f = folded([keep(1, "a")]);
    expect(propose(f, { type: "decide", groupId: "a", verdict: "reject" })).toEqual({
      ok: false,
      refusal: "already-decided",
    });
  });

  it("refuses U on an undecided group, and undo or redo on an empty stack", () => {
    expect(propose(EMPTY, { type: "clear", groupId: "a" })).toEqual({
      ok: false,
      refusal: "already-undecided",
    });
    expect(propose(EMPTY, { type: "undo" })).toEqual({ ok: false, refusal: "nothing-to-undo" });
    expect(propose(EMPTY, { type: "redo" })).toEqual({ ok: false, refusal: "nothing-to-redo" });
  });

  it("proposes undo of the top of done and redo of the top of undone", () => {
    const f = folded([keep(1, "a"), keep(2, "b"), undo(3, 2)]);
    expect(propose(f, { type: "undo" })).toEqual({ ok: true, body: undo(9, 1).body });
    expect(propose(f, { type: "redo" })).toEqual({ ok: true, body: redo(9, 2).body });
  });

  it("every accepted proposal steps cleanly", () => {
    let f = EMPTY;
    const intents = [
      { type: "decide", groupId: "a", verdict: "keep" },
      { type: "clear", groupId: "a" },
      { type: "undo" },
      { type: "redo" },
      { type: "decide", groupId: "a", verdict: "reject" },
    ] as const;
    for (const intent of intents) {
      const proposal = propose(f, intent);
      expect(proposal.ok).toBe(true);
      if (proposal.ok) {
        const result = step(f, user(f.lastSeq + 1, proposal.body));
        expect(result.ok).toBe(true);
        if (result.ok) {
          f = result.folded;
        }
      }
    }
    expect(toExpected(f)).toEqual({ state: { a: "rejected" }, done: [1, 2, 5], undone: [] });
  });
});

describe("parseEvent", () => {
  it("reads every event type", () => {
    for (const event of [keep(3, "g"), reject(4, "g"), clear(5, "g"), undo(6, 5), redo(7, 5)]) {
      const raw: unknown = JSON.parse(JSON.stringify(event));
      expect(parseEvent(raw)).toEqual(event);
    }
  });

  it("rejects other shapes", () => {
    const bad: unknown[] = [
      null,
      [],
      { seq: 0, ts: 1, cause: "user", body: { type: "cleared", groupId: "g" } },
      { seq: 1, ts: "1", cause: "user", body: { type: "cleared", groupId: "g" } },
      { seq: 1, ts: 1, cause: "agent", body: { type: "cleared", groupId: "g" } },
      { seq: 1, ts: 1, cause: "user", body: { type: "decided", groupId: "g", verdict: "maybe" } },
      { seq: 1, ts: 1, cause: "user", body: { type: "undo", targetSeq: -1 } },
      { seq: 1, ts: 1, cause: "user", body: { type: "rename" } },
    ];
    for (const value of bad) {
      expect(() => parseEvent(value)).toThrow();
    }
  });
});
