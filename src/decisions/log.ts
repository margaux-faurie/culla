/**
 * The TS reducer over the decision log: `fold(events) → (state, done, undone)`, a port of the
 * core's `fold` (`crates/core/src/decisions/mod.rs`, design/file-engine.md §5.1). Pure, no I/O, no
 * DOM, so the golden fixtures shared with the Rust `fold` (docs/testing.md §3.3) run it in Node
 * and must give the same text byte for byte (`tests/golden.test.ts`).
 *
 * The checks run in the core's order: the seq first (strictly increasing, gaps allowed, `ts`
 * ignored), then the cause (reconcile only on `cleared`), then the body. Locks (groups in an open
 * plan or applied, §5.4) are not visible to the fold, as in the core; the commands will check them
 * once plans reach the UI (M4).
 */
import {
  type CorruptReason,
  type DecisionEvent,
  type EventBody,
  type GroupId,
  type GroupState,
  type Seq,
  stateFor,
  type Verdict,
} from "./events";

/** A user `decided` or `cleared` that is on one of the stacks: what undo and redo need. */
interface Step {
  readonly group: GroupId;
  readonly before: GroupState;
  readonly after: GroupState;
}

export interface Folded {
  /** Decided groups only: undecided is the absence of an entry. */
  readonly state: ReadonlyMap<GroupId, GroupState>;
  /** The undo stack, bottom first: the next one to undo is the last element. */
  readonly done: readonly Seq[];
  /** The redo stack, bottom first: the next one to redo is the last element. */
  readonly undone: readonly Seq[];
  /** One entry per seq in `done` or `undone`, and no other. */
  readonly steps: ReadonlyMap<Seq, Step>;
  /** The seq of the last event folded; undefined for an empty log. */
  readonly lastSeq: Seq | undefined;
}

export const EMPTY: Folded = {
  state: new Map(),
  done: [],
  undone: [],
  steps: new Map(),
  lastSeq: undefined,
};

export const stateOf = (folded: Folded, group: GroupId): GroupState =>
  folded.state.get(group) ?? "undecided";

/** The seq the store gives the next event (the core's store does the same from the last one). */
export const nextSeq = (folded: Folded): Seq => (folded.lastSeq ?? 0) + 1;

export type StepResult =
  | { readonly ok: true; readonly folded: Folded }
  | { readonly ok: false; readonly reason: CorruptReason };

/** Folds one more event (`Decisions::apply`), or says which precondition it breaks. */
export function step(folded: Folded, e: DecisionEvent): StepResult {
  if (folded.lastSeq !== undefined && e.seq <= folded.lastSeq) {
    return refuse("seq_not_increasing");
  }
  if (e.cause === "reconcile" && e.type !== "cleared") {
    return refuse("reconcile_not_cleared");
  }
  switch (e.type) {
    case "decided": {
      const before = stateOf(folded, e.group_id);
      if (before !== "undecided") {
        return refuse("decided_on_decided");
      }
      return pushUserStep(folded, e.seq, { group: e.group_id, before, after: stateFor(e.verdict) });
    }
    case "cleared": {
      const before = stateOf(folded, e.group_id);
      if (before === "undecided") {
        return refuse("cleared_on_undecided");
      }
      if (e.cause === "user") {
        return pushUserStep(folded, e.seq, { group: e.group_id, before, after: "undecided" });
      }
      // ADR 0024: the group becomes undecided and its events leave both stacks; nothing is pushed.
      const steps = new Map(folded.steps);
      const keep = (seq: Seq) => {
        const drop = folded.steps.get(seq)?.group === e.group_id;
        if (drop) {
          steps.delete(seq);
        }
        return !drop;
      };
      return ok({
        state: withState(folded.state, e.group_id, "undecided"),
        done: folded.done.filter(keep),
        undone: folded.undone.filter(keep),
        steps,
        lastSeq: e.seq,
      });
    }
    case "undo": {
      const s = folded.done.at(-1) === e.target_seq ? folded.steps.get(e.target_seq) : undefined;
      if (s === undefined) {
        return refuse("undo_not_top");
      }
      return ok({
        ...folded,
        state: withState(folded.state, s.group, s.before),
        done: folded.done.slice(0, -1),
        undone: [...folded.undone, e.target_seq],
        lastSeq: e.seq,
      });
    }
    case "redo": {
      const s = folded.undone.at(-1) === e.target_seq ? folded.steps.get(e.target_seq) : undefined;
      if (s === undefined) {
        return refuse("redo_not_top");
      }
      return ok({
        ...folded,
        state: withState(folded.state, s.group, s.after),
        done: [...folded.done, e.target_seq],
        undone: folded.undone.slice(0, -1),
        lastSeq: e.seq,
      });
    }
  }
}

export interface FoldResult {
  /** The fold of every event, or of the events before the bad one. */
  readonly folded: Folded;
  /** Set when the log breaks a precondition: folding stopped at that event. */
  readonly corrupt?: { readonly seq: Seq; readonly reason: CorruptReason };
}

export function fold(events: Iterable<DecisionEvent>): FoldResult {
  let folded = EMPTY;
  for (const e of events) {
    const result = step(folded, e);
    if (!result.ok) {
      return { folded, corrupt: { seq: e.seq, reason: result.reason } };
    }
    folded = result.folded;
  }
  return { folded };
}

/** What the user asked for; `propose` turns it into an event body or says why nothing is added. */
export type Intent =
  | { readonly type: "decide"; readonly group: GroupId; readonly verdict: Verdict }
  | { readonly type: "clear"; readonly group: GroupId }
  | { readonly type: "undo" }
  | { readonly type: "redo" };

export type Refusal =
  | "already-decided"
  | "already-undecided"
  | "nothing-to-undo"
  | "nothing-to-redo";

export type Proposal = { ok: true; body: EventBody } | { ok: false; refusal: Refusal };

/** The command checks of the core (`Decisions::decide`, `clear`, `undo`, `redo`), without locks. */
export function propose(folded: Folded, intent: Intent): Proposal {
  switch (intent.type) {
    case "decide":
      return stateOf(folded, intent.group) === "undecided"
        ? { ok: true, body: { type: "decided", group_id: intent.group, verdict: intent.verdict } }
        : { ok: false, refusal: "already-decided" };
    case "clear":
      return stateOf(folded, intent.group) === "undecided"
        ? { ok: false, refusal: "already-undecided" }
        : { ok: true, body: { type: "cleared", group_id: intent.group } };
    case "undo": {
      const top = folded.done.at(-1);
      return top === undefined
        ? { ok: false, refusal: "nothing-to-undo" }
        : { ok: true, body: { type: "undo", target_seq: top } };
    }
    case "redo": {
      const top = folded.undone.at(-1);
      return top === undefined
        ? { ok: false, refusal: "nothing-to-redo" }
        : { ok: true, body: { type: "redo", target_seq: top } };
    }
  }
}

/** The group an event acts on: its own for decided and cleared, its target's for undo and redo. */
export function groupOf(folded: Folded, body: EventBody): GroupId | undefined {
  switch (body.type) {
    case "decided":
    case "cleared":
      return body.group_id;
    case "undo":
    case "redo":
      return folded.steps.get(body.target_seq)?.group;
  }
}

/** The parsed form of a golden `.expected.json`. */
export interface Expected {
  state: Record<string, "kept" | "rejected">;
  done: Seq[];
  undone: Seq[];
  corrupt?: { seq: Seq; reason: CorruptReason };
}

export function toExpected(result: FoldResult): Expected {
  const state: Record<string, "kept" | "rejected"> = {};
  for (const [group, value] of result.folded.state) {
    if (value !== "undecided") {
      state[String(group)] = value;
    }
  }
  const { done, undone } = result.folded;
  const expected: Expected = { state, done: [...done], undone: [...undone] };
  if (result.corrupt !== undefined) {
    expected.corrupt = { ...result.corrupt };
  }
  return expected;
}

const ok = (folded: Folded): StepResult => ({ ok: true, folded });
const refuse = (reason: CorruptReason): StepResult => ({ ok: false, reason });

function withState(
  state: ReadonlyMap<GroupId, GroupState>,
  group: GroupId,
  value: GroupState,
): Map<GroupId, GroupState> {
  const next = new Map(state);
  if (value === "undecided") {
    next.delete(group);
  } else {
    next.set(group, value);
  }
  return next;
}

/** A new user `decided` or `cleared`: push on `done`, empty `undone` (and drop its steps). */
function pushUserStep(folded: Folded, seq: Seq, s: Step): StepResult {
  const steps = new Map(folded.steps);
  for (const undone of folded.undone) {
    steps.delete(undone);
  }
  steps.set(seq, s);
  return ok({
    state: withState(folded.state, s.group, s.after),
    done: [...folded.done, seq],
    undone: [],
    steps,
    lastSeq: seq,
  });
}
