/**
 * Decision events, as in ADR 0005, ADR 0024 and design/file-engine.md §2.1 and §5.1.
 *
 * The shape is the core's text form (`crates/core/src/decisions/codec.rs`): one flat object per
 * event whose keys are the `decision_event` columns, snake_case, so a TS event and a line of a
 * golden fixture (`testdata/event-log/`) are the same JSON. The Rust types stay the source; this
 * copy goes once they are exported to TS (ADR 0019). `codec.ts` reads and writes the text form.
 */

/** A group's id: the `photo_group.id` row id of the session database. */
export type GroupId = number;

/** An event's sequence number: strictly increasing within a log, gaps allowed. */
export type Seq = number;

export type Verdict = "keep" | "reject";

export type GroupState = "undecided" | "kept" | "rejected";

/** `reconcile` is only ever set on `cleared`, by the core when a file joins a decided group. */
export type Cause = "user" | "reconcile";

/** What an event says; flat, like the rest of the event. */
export type EventBody =
  | { readonly type: "decided"; readonly group_id: GroupId; readonly verdict: Verdict }
  | { readonly type: "cleared"; readonly group_id: GroupId }
  | { readonly type: "undo"; readonly target_seq: Seq }
  | { readonly type: "redo"; readonly target_seq: Seq };

export interface EventHead {
  readonly seq: Seq;
  /** Unix milliseconds (UTC). Informational: the fold ignores it. */
  readonly ts: number;
  readonly cause: Cause;
}

export type DecisionEvent = EventHead & EventBody;

/** Why the fold stopped at an event (`CorruptReason::as_str` in the core). */
export type CorruptReason =
  | "seq_not_increasing"
  | "decided_on_decided"
  | "cleared_on_undecided"
  | "reconcile_not_cleared"
  | "undo_not_top"
  | "redo_not_top";

export function stateFor(verdict: Verdict): GroupState {
  return verdict === "keep" ? "kept" : "rejected";
}

/** The event's body alone, for comparing what a command proposes. */
export function bodyOf(event: DecisionEvent): EventBody {
  switch (event.type) {
    case "decided":
      return { type: "decided", group_id: event.group_id, verdict: event.verdict };
    case "cleared":
      return { type: "cleared", group_id: event.group_id };
    case "undo":
    case "redo":
      return { type: event.type, target_seq: event.target_seq };
  }
}
