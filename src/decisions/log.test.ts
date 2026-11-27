import { describe, expect, it } from "vitest";
import { parseLog, renderEvent, renderFold, tryParseEvent } from "./codec";
import { bodyOf, type Cause, type DecisionEvent, type EventBody } from "./events";
import { EMPTY, type Folded, fold, nextSeq, propose, stateOf, step, toExpected } from "./log";

const ev = (seq: number, body: EventBody, cause: Cause = "user"): DecisionEvent => ({
  seq,
  ts: seq,
  cause,
  ...body,
});
const keep = (seq: number, group_id: number) =>
  ev(seq, { type: "decided", group_id, verdict: "keep" });
const reject = (seq: number, group_id: number) =>
  ev(seq, { type: "decided", group_id, verdict: "reject" });
const clear = (seq: number, group_id: number) => ev(seq, { type: "cleared", group_id });
const reconcile = (seq: number, group_id: number) =>
  ev(seq, { type: "cleared", group_id }, "reconcile");
const undo = (seq: number, target_seq: number) => ev(seq, { type: "undo", target_seq });
const redo = (seq: number, target_seq: number) => ev(seq, { type: "redo", target_seq });

function folded(events: DecisionEvent[]): Folded {
  const result = fold(events);
  expect(result.corrupt).toBeUndefined();
  return result.folded;
}
const expected = (events: DecisionEvent[]) => toExpected(fold(events));

describe("fold", () => {
  it("gives an empty state and empty stacks for an empty log", () => {
    expect(expected([])).toEqual({ state: {}, done: [], undone: [] });
    expect(EMPTY.lastSeq).toBeUndefined();
  });

  it("records decisions and pushes them on done", () => {
    expect(expected([keep(1, 1), reject(2, 2)])).toEqual({
      state: { "1": "kept", "2": "rejected" },
      done: [1, 2],
      undone: [],
    });
  });

  it("undo reverts the top of done and moves it to undone; redo replays it", () => {
    expect(expected([keep(1, 1), reject(2, 2), undo(3, 2)])).toEqual({
      state: { "1": "kept" },
      done: [1],
      undone: [2],
    });
    expect(expected([keep(1, 1), reject(2, 2), undo(3, 2), redo(4, 2)])).toEqual({
      state: { "1": "kept", "2": "rejected" },
      done: [1, 2],
      undone: [],
    });
  });

  it("U (cleared) is undoable and undo restores the earlier verdict", () => {
    const cleared = folded([reject(1, 1), clear(2, 1)]);
    expect(stateOf(cleared, 1)).toBe("undecided");
    expect(cleared.state.has(1)).toBe(false);
    expect(cleared.done).toEqual([1, 2]);
    const restored = folded([reject(1, 1), clear(2, 1), undo(3, 2)]);
    expect(stateOf(restored, 1)).toBe("rejected");
    expect(restored.undone).toEqual([2]);
  });

  it("a new decision or user cleared empties the redo stack", () => {
    expect(folded([keep(1, 1), undo(2, 1), keep(3, 2)]).undone).toEqual([]);
    expect(folded([keep(1, 1), keep(2, 2), undo(3, 2), clear(4, 1)]).undone).toEqual([]);
  });

  it("a reconcile cleared leaves both stacks and cannot be undone (ADR 0024)", () => {
    expect(expected([keep(1, 1), keep(2, 2), keep(3, 3), reconcile(4, 1)])).toEqual({
      state: { "2": "kept", "3": "kept" },
      done: [2, 3],
      undone: [],
    });
    const dropped = folded([keep(1, 1), clear(2, 1), undo(3, 2), reconcile(4, 1)]);
    expect(toExpected({ folded: dropped })).toEqual({ state: {}, done: [], undone: [] });
    expect(dropped.steps.size).toBe(0);
    expect(propose(dropped, { type: "undo" })).toEqual({ ok: false, refusal: "nothing-to-undo" });
  });

  it("allows seq gaps and any first seq, and ignores ts", () => {
    const events = [
      { ...keep(-5, 1), ts: 99 },
      { ...undo(40, -5), ts: 0 },
    ];
    expect(expected(events)).toEqual({
      state: {},
      done: [],
      undone: [-5],
    });
  });

  it("stops at the first bad event with the core's reason, keeping the fold before it", () => {
    const cases: [DecisionEvent[], string][] = [
      [[keep(1, 1), keep(2, 1)], "decided_on_decided"],
      [[clear(1, 1)], "cleared_on_undecided"],
      [[reconcile(1, 1)], "cleared_on_undecided"],
      [[keep(1, 1), undo(2, 7)], "undo_not_top"],
      [[undo(1, 1)], "undo_not_top"],
      [[keep(1, 1), redo(2, 1)], "redo_not_top"],
      [[keep(2, 1), keep(1, 2)], "seq_not_increasing"],
      [[keep(1, 1), keep(1, 2)], "seq_not_increasing"],
      [[keep(1, 1), { ...keep(2, 2), cause: "reconcile" }], "reconcile_not_cleared"],
      [[keep(1, 1), { ...undo(2, 1), cause: "reconcile" }], "reconcile_not_cleared"],
    ];
    for (const [events, reason] of cases) {
      const result = fold(events);
      expect(result.corrupt).toEqual({ seq: events.at(-1)?.seq, reason });
      expect(result.folded.lastSeq).toBe(events.length === 1 ? undefined : events[0]?.seq);
    }
  });

  it("checks the seq first, then the cause, then the body", () => {
    // A repeated seq that is also decided-on-decided and reconcile-not-cleared.
    const both = ev(1, { type: "decided", group_id: 1, verdict: "keep" }, "reconcile");
    expect(fold([keep(1, 1), both]).corrupt?.reason).toBe("seq_not_increasing");
    expect(fold([keep(1, 1), { ...both, seq: 2 }]).corrupt?.reason).toBe("reconcile_not_cleared");
  });
});

describe("propose", () => {
  it("refuses to decide a decided group (undo first, ADR 0006)", () => {
    expect(propose(folded([keep(1, 1)]), { type: "decide", group: 1, verdict: "reject" })).toEqual({
      ok: false,
      refusal: "already-decided",
    });
  });

  it("refuses U on an undecided group, and undo or redo on an empty stack", () => {
    expect(propose(EMPTY, { type: "clear", group: 1 })).toEqual({
      ok: false,
      refusal: "already-undecided",
    });
    expect(propose(EMPTY, { type: "undo" })).toEqual({ ok: false, refusal: "nothing-to-undo" });
    expect(propose(EMPTY, { type: "redo" })).toEqual({ ok: false, refusal: "nothing-to-redo" });
  });

  it("proposes undo of the top of done and redo of the top of undone", () => {
    const f = folded([keep(1, 1), keep(2, 2), undo(3, 2)]);
    expect(propose(f, { type: "undo" })).toEqual({ ok: true, body: bodyOf(undo(9, 1)) });
    expect(propose(f, { type: "redo" })).toEqual({ ok: true, body: bodyOf(redo(9, 2)) });
  });

  it("every accepted proposal steps cleanly", () => {
    let f = EMPTY;
    const intents = [
      { type: "decide", group: 1, verdict: "keep" },
      { type: "clear", group: 1 },
      { type: "undo" },
      { type: "redo" },
      { type: "decide", group: 1, verdict: "reject" },
    ] as const;
    for (const intent of intents) {
      const proposal = propose(f, intent);
      expect(proposal.ok).toBe(true);
      if (proposal.ok) {
        const result = step(f, ev(nextSeq(f), proposal.body));
        expect(result.ok).toBe(true);
        if (result.ok) {
          f = result.folded;
        }
      }
    }
    expect(toExpected({ folded: f })).toEqual({
      state: { "1": "rejected" },
      done: [1, 2, 5],
      undone: [],
    });
  });
});

describe("codec", () => {
  it("renders and parses every event type in the core's form", () => {
    expect(renderEvent(keep(1, 7))).toBe(
      '{"seq":1,"ts":1,"cause":"user","type":"decided","group_id":7,"verdict":"keep"}',
    );
    expect(renderEvent(reconcile(4, 7))).toBe(
      '{"seq":4,"ts":4,"cause":"reconcile","type":"cleared","group_id":7}',
    );
    expect(renderEvent(undo(3, 2))).toBe(
      '{"seq":3,"ts":3,"cause":"user","type":"undo","target_seq":2}',
    );
    for (const e of [keep(3, 9), reject(4, 9), clear(5, 9), undo(6, 5), redo(7, 5)]) {
      expect(tryParseEvent(renderEvent(e))).toEqual({ ok: true, event: e });
    }
  });

  it("accepts any key order and JSON whitespace", () => {
    const line =
      '{ "verdict" : "reject","group_id":-3,"type":"decided","cause":"user",\t"seq":2 ,"ts":0 }';
    expect(tryParseEvent(line)).toEqual({ ok: true, event: { ...reject(2, -3), ts: 0 } });
  });

  it("refuses what the core refuses, with the same error", () => {
    const head = '"seq":1,"ts":1,"cause":"user"';
    const cleared = (rest: string) => `{${head},"type":"cleared"${rest}}`;
    const syntax = (at: number, expected: string) => ({ kind: "syntax", at, expected });
    const unknown = (key: string) => ({ kind: "unknown_key", key });
    const bad = (key: string) => ({ kind: "bad_value", key });
    const cases: [string, unknown][] = [
      ["", syntax(0, "'{'")],
      [cleared(',"group_id":1,"extra":1'), unknown("extra")],
      [`{${head},"seq":2,"type":"cleared","group_id":1}`, { kind: "duplicate_key", key: "seq" }],
      [cleared(""), { kind: "missing_key", key: "group_id" }],
      [cleared(',"group_id":"1"'), bad("group_id")],
      [cleared(',"group_id":9007199254740992'), bad("group_id")],
      [cleared(',"group_id":-9007199254740991'), null],
      [cleared(',"group_id":1.0'), syntax(60, "an integer")],
      [cleared(',"group_id":01'), syntax(59, "an integer")],
      [cleared(',"group_id":99999999999999999999'), syntax(59, "an integer in range")],
      [`{${head},"type":"cle\\u0061red","group_id":1}`, syntax(42, "a string without escapes")],
      [cleared(',"group_id":1,"verdict":"keep"'), unknown("verdict")],
      [`{${head},"type":"undo","group_id":1,"target_seq":1}`, unknown("group_id")],
      [`{${head},"type":"decided","group_id":1,"verdict":"maybe"}`, bad("verdict")],
      ['{"seq":1,"ts":1,"cause":"agent","type":"cleared","group_id":1}', bad("cause")],
      [`{${head},"type":"rename"}`, bad("type")],
      [`${cleared(',"group_id":1')}x`, syntax(61, "the end of the line")],
    ];
    for (const [line, error] of cases) {
      const result = tryParseEvent(line);
      expect(result.ok ? null : result.error).toEqual(error);
    }
  });

  it("needs a final newline and refuses an empty line", () => {
    expect(parseLog("")).toEqual([]);
    expect(() => parseLog(renderEvent(keep(1, 1)))).toThrow();
    expect(() => parseLog(`${renderEvent(keep(1, 1))}\n\n`)).toThrow();
  });

  it("renders a fold like the core", () => {
    const result = fold([keep(1, 9), reject(2, 10), keep(3, 2), undo(4, 3), keep(5, 9)]);
    expect(renderFold({ ...result.folded, corrupt: result.corrupt })).toBe(
      [
        "{",
        '  "state": {',
        '    "9": "kept",',
        '    "10": "rejected"',
        "  },",
        '  "done": [1, 2],',
        '  "undone": [3],',
        '  "corrupt": {"seq": 5, "reason": "decided_on_decided"}',
        "}",
        "",
      ].join("\n"),
    );
    expect(renderFold(EMPTY)).toBe('{\n  "state": {},\n  "done": [],\n  "undone": []\n}\n');
  });
});
