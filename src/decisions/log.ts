/**
 * The TS reducer over the decision log: `fold(events) → (state, done, undone)`, with the
 * preconditions of design/file-engine.md §5.1. Pure, no I/O, no DOM, so the golden fixtures shared
 * with the Rust `fold` (docs/testing.md §3.3) can run it in Node.
 *
 * Locked and applied groups (§5.4) are not modelled yet: they need plans, which arrive with the
 * review and apply screens (M4).
 */
import {
  type DecisionEvent,
  type EventBody,
  type GroupState,
  stateFor,
  type Verdict,
} from "./events";

/** What a `decided` or user `cleared` event did, so undo and redo can revert and replay it. */
interface Entry {
  readonly groupId: string;
  readonly before: GroupState;
  readonly after: GroupState;
}

export interface Folded {
  /** Groups that were ever touched; a missing group is undecided. */
  readonly state: ReadonlyMap<string, GroupState>;
  /** Seqs of `decided` and user `cleared` events that can be undone, oldest first. */
  readonly done: readonly number[];
  /** Seqs of undone events that can be redone, oldest first; the top is the last element. */
  readonly undone: readonly number[];
  readonly entries: ReadonlyMap<number, Entry>;
  readonly lastSeq: number;
}

export const EMPTY: Folded = {
  state: new Map(),
  done: [],
  undone: [],
  entries: new Map(),
  lastSeq: 0,
};

export const stateOf = (folded: Folded, groupId: string): GroupState =>
  folded.state.get(groupId) ?? "undecided";

export type StepResult = { ok: true; folded: Folded } | { ok: false; reason: string };

/** Applies one event, or says which precondition it breaks (the log is then corrupt). */
export function step(folded: Folded, event: DecisionEvent): StepResult {
  if (event.seq <= folded.lastSeq) {
    return refuse(`seq ${event.seq} is not after ${folded.lastSeq}`);
  }
  const { body } = event;
  if (event.cause === "reconcile" && body.type !== "cleared") {
    return refuse(`seq ${event.seq}: only cleared can have cause reconcile`);
  }
  switch (body.type) {
    case "decided": {
      const before = stateOf(folded, body.groupId);
      if (before !== "undecided") {
        return refuse(`seq ${event.seq}: group ${body.groupId} is already ${before}`);
      }
      const after = stateFor(body.verdict);
      return record(folded, event.seq, { groupId: body.groupId, before, after });
    }
    case "cleared": {
      const before = stateOf(folded, body.groupId);
      if (before === "undecided") {
        return refuse(`seq ${event.seq}: group ${body.groupId} is already undecided`);
      }
      if (event.cause === "reconcile") {
        // ADR 0024: the group becomes undecided and leaves both stacks; nothing is pushed.
        const touches = (seq: number) => folded.entries.get(seq)?.groupId === body.groupId;
        return ok({
          ...folded,
          state: withState(folded.state, body.groupId, "undecided"),
          done: folded.done.filter((seq) => !touches(seq)),
          undone: folded.undone.filter((seq) => !touches(seq)),
          lastSeq: event.seq,
        });
      }
      return record(folded, event.seq, { groupId: body.groupId, before, after: "undecided" });
    }
    case "undo": {
      const top = folded.done.at(-1);
      const entry = top === undefined ? undefined : folded.entries.get(top);
      if (top !== body.targetSeq || entry === undefined) {
        return refuse(`seq ${event.seq}: undo target ${body.targetSeq} is not the top of done`);
      }
      if (stateOf(folded, entry.groupId) !== entry.after) {
        return refuse(`seq ${event.seq}: group ${entry.groupId} changed since ${top}`);
      }
      return ok({
        ...folded,
        state: withState(folded.state, entry.groupId, entry.before),
        done: folded.done.slice(0, -1),
        undone: [...folded.undone, top],
        lastSeq: event.seq,
      });
    }
    case "redo": {
      const top = folded.undone.at(-1);
      const entry = top === undefined ? undefined : folded.entries.get(top);
      if (top !== body.targetSeq || entry === undefined) {
        return refuse(`seq ${event.seq}: redo target ${body.targetSeq} is not the top of undone`);
      }
      if (stateOf(folded, entry.groupId) !== entry.before) {
        return refuse(`seq ${event.seq}: group ${entry.groupId} changed since ${top}`);
      }
      return ok({
        ...folded,
        state: withState(folded.state, entry.groupId, entry.after),
        done: [...folded.done, top],
        undone: folded.undone.slice(0, -1),
        lastSeq: event.seq,
      });
    }
  }
}

export interface FoldResult {
  readonly folded: Folded;
  /** Set when the log breaks a precondition: folding stopped before that event. */
  readonly corrupt?: { readonly seq: number; readonly reason: string };
}

export function fold(events: Iterable<DecisionEvent>): FoldResult {
  let folded = EMPTY;
  for (const event of events) {
    const result = step(folded, event);
    if (!result.ok) {
      return { folded, corrupt: { seq: event.seq, reason: result.reason } };
    }
    folded = result.folded;
  }
  return { folded };
}

/** What the user asked for; `propose` turns it into an event body, or says why nothing is appended. */
export type Intent =
  | { readonly type: "decide"; readonly groupId: string; readonly verdict: Verdict }
  | { readonly type: "clear"; readonly groupId: string }
  | { readonly type: "undo" }
  | { readonly type: "redo" };

export type Refusal =
  | "already-decided"
  | "already-undecided"
  | "nothing-to-undo"
  | "nothing-to-redo";

export type Proposal = { ok: true; body: EventBody } | { ok: false; refusal: Refusal };

export function propose(folded: Folded, intent: Intent): Proposal {
  switch (intent.type) {
    case "decide":
      return stateOf(folded, intent.groupId) === "undecided"
        ? { ok: true, body: { type: "decided", groupId: intent.groupId, verdict: intent.verdict } }
        : { ok: false, refusal: "already-decided" };
    case "clear":
      return stateOf(folded, intent.groupId) === "undecided"
        ? { ok: false, refusal: "already-undecided" }
        : { ok: true, body: { type: "cleared", groupId: intent.groupId } };
    case "undo": {
      const top = folded.done.at(-1);
      return top === undefined
        ? { ok: false, refusal: "nothing-to-undo" }
        : { ok: true, body: { type: "undo", targetSeq: top } };
    }
    case "redo": {
      const top = folded.undone.at(-1);
      return top === undefined
        ? { ok: false, refusal: "nothing-to-redo" }
        : { ok: true, body: { type: "redo", targetSeq: top } };
    }
  }
}

/** The group an event acts on: its own for decided and cleared, its target's for undo and redo. */
export function groupOf(folded: Folded, body: EventBody): string | undefined {
  switch (body.type) {
    case "decided":
    case "cleared":
      return body.groupId;
    case "undo":
    case "redo":
      return folded.entries.get(body.targetSeq)?.groupId;
  }
}

/**
 * The golden fixtures' `expected.json` form: decided groups only (an undecided group is left out,
 * whether it was never touched or cleared), and both stacks oldest first. Proposed by the agent and
 * awaiting Margaux (SPEC §10), until the Rust side writes the first non-empty fixture.
 */
export interface Expected {
  state: Record<string, "kept" | "rejected">;
  done: number[];
  undone: number[];
}

export function toExpected(folded: Folded): Expected {
  const state: Record<string, "kept" | "rejected"> = {};
  for (const [groupId, value] of folded.state) {
    if (value !== "undecided") {
      state[groupId] = value;
    }
  }
  return { state, done: [...folded.done], undone: [...folded.undone] };
}

const ok = (folded: Folded): StepResult => ({ ok: true, folded });
const refuse = (reason: string): StepResult => ({ ok: false, reason });

function withState(
  state: ReadonlyMap<string, GroupState>,
  groupId: string,
  value: GroupState,
): Map<string, GroupState> {
  const next = new Map(state);
  next.set(groupId, value);
  return next;
}

function record(folded: Folded, seq: number, entry: Entry): StepResult {
  const entries = new Map(folded.entries);
  entries.set(seq, entry);
  return ok({
    state: withState(folded.state, entry.groupId, entry.after),
    done: [...folded.done, seq],
    undone: [],
    entries,
    lastSeq: seq,
  });
}
