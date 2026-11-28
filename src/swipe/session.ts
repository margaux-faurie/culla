/**
 * State of the swipe screen: the decision log and its fold (decisions/log.ts), plus where the user
 * is, the filter and the feedback to show. Pure: `now` comes in with each action, and the events to
 * mirror to the backend are read off `log` by the caller (commit first, then autosave, ADR 0005).
 */
import type { DecisionEvent, GroupId, GroupState, Verdict } from "../decisions/events";
import {
  EMPTY,
  type Folded,
  fold,
  groupOf,
  type Intent,
  nextSeq,
  propose,
  type Refusal,
  stateOf,
  step,
} from "../decisions/log";

export type Filter = "all" | "undecided" | "kept" | "rejected";

export const FILTERS: readonly Filter[] = ["all", "undecided", "kept", "rejected"];

export type Feedback =
  /** ←/→, the buttons or a drag on a decided photo: shake and "Press U to undo" (ADR 0006). */
  | { readonly kind: "already-decided"; readonly photoId: string; readonly state: GroupState }
  | { readonly kind: Exclude<Refusal, "already-decided"> }
  | { readonly kind: "no-undecided" }
  /** A session resumed from its stored log (US-37): how far it had got. */
  | { readonly kind: "resumed"; readonly decided: number; readonly total: number };

export interface SwipeState {
  /**
   * Opaque photo (group) ids in display order. The photo at index `i` is decision group `i + 1`
   * (`groupAt`), until the backend lists groups with their own ids (proposed, SPEC §10).
   */
  readonly photos: readonly string[];
  readonly log: readonly DecisionEvent[];
  readonly folded: Folded;
  /** Position in `photos`; `photos.length` is the end-of-list card. */
  readonly index: number;
  readonly filter: Filter;
  readonly feedback: Feedback | null;
  /** The card leaving after a decision, for the exit animation; never blocks input (ADR 0015). */
  readonly exit: { readonly photoId: string; readonly verdict: Verdict } | null;
  /** Bumped on every action that shows feedback or an exit, so the UI can restart its animation. */
  readonly nonce: number;
  /** For the polite live region (NFR-A11Y-09). */
  readonly announcement: string;
}

export type SwipeAction =
  | { readonly type: "decide"; readonly verdict: Verdict; readonly now: number }
  | { readonly type: "clear"; readonly now: number }
  | { readonly type: "undo"; readonly now: number }
  | { readonly type: "redo"; readonly now: number }
  | { readonly type: "next" }
  | { readonly type: "prev" }
  | { readonly type: "nextUndecided" }
  | { readonly type: "cycleFilter" }
  | { readonly type: "dismissFeedback" }
  /**
   * Back from the review on one photo (a grid thumbnail, US-34): no decision, and the filter goes
   * to "all" so the photo is always reachable (proposed, SPEC §10).
   */
  | { readonly type: "show"; readonly photoId: string }
  /** "Sort them now" on the review: the first undecided photo, with the filter on "all". */
  | { readonly type: "firstUndecided" };

export function initialState(photos: readonly string[]): SwipeState {
  return {
    photos,
    log: [],
    folded: EMPTY,
    index: 0,
    filter: "all",
    feedback: null,
    exit: null,
    nonce: 0,
    announcement: "",
  };
}

/**
 * A session resumed from its stored log (US-37): every decision and both undo stacks as they were,
 * the list in its stored order, on the first undecided photo (proposed, SPEC §10), or the end card
 * when none is left. The events are already in the backend, so nothing is saved again. A log that
 * does not fold keeps the decisions before the bad event (the backend refuses such a log first).
 */
export function resumedState(
  photos: readonly string[],
  events: readonly DecisionEvent[],
): SwipeState {
  const { folded, corrupt } = fold(events);
  const log = corrupt === undefined ? events : events.filter((e) => e.seq < corrupt.seq);
  const start: SwipeState = { ...initialState(photos), log, folded };
  if (log.length === 0) {
    return start;
  }
  const first = photos.findIndex((_, i) => stateAt(start, i) === "undecided");
  const index = first < 0 ? photos.length : first;
  const tally = counts(start);
  const resumed: Feedback = {
    kind: "resumed",
    decided: tally.kept + tally.rejected,
    total: tally.total,
  };
  return { ...start, index, feedback: resumed, nonce: 1, announcement: announce(resumed) };
}

export const currentPhoto = (s: SwipeState): string | undefined => s.photos[s.index];

/** The decision group of the photo at `index`. */
export const groupAt = (index: number): GroupId => index + 1;

/** The photo index of a decision group, or -1 if it is not on screen. */
const indexOfGroup = (s: SwipeState, group: GroupId): number =>
  Number.isSafeInteger(group) && group >= 1 && group <= s.photos.length ? group - 1 : -1;

/** A photo's decision state by its index in `photos`. */
export const stateAt = (s: SwipeState, index: number): GroupState =>
  stateOf(s.folded, groupAt(index));

export const atEnd = (s: SwipeState): boolean => s.index >= s.photos.length;

export interface Counts {
  readonly total: number;
  readonly kept: number;
  readonly rejected: number;
  readonly undecided: number;
}

export function counts(s: SwipeState): Counts {
  let kept = 0;
  let rejected = 0;
  for (let i = 0; i < s.photos.length; i += 1) {
    const state = stateAt(s, i);
    if (state === "kept") kept += 1;
    else if (state === "rejected") rejected += 1;
  }
  const total = s.photos.length;
  return { total, kept, rejected, undecided: total - kept - rejected };
}

const matches = (filter: Filter, state: GroupState): boolean =>
  filter === "all" || filter === state;

export function swipeReducer(s: SwipeState, action: SwipeAction): SwipeState {
  switch (action.type) {
    case "decide": {
      const photoId = currentPhoto(s);
      if (photoId === undefined) {
        return s;
      }
      const intent: Intent = { type: "decide", group: groupAt(s.index), verdict: action.verdict };
      const result = commit(s, intent, action.now);
      if (!result.ok) {
        return feedback(s, {
          kind: "already-decided",
          photoId,
          state: stateAt(s, s.index),
        });
      }
      const index = nextIndex(result.state, s.index);
      return {
        ...result.state,
        index,
        exit: { photoId, verdict: action.verdict },
        feedback: null,
        nonce: s.nonce + 1,
        announcement: `${action.verdict === "keep" ? "Kept" : "Rejected"}, ${of(s, s.index)}`,
      };
    }
    case "clear": {
      const photoId = currentPhoto(s);
      if (photoId === undefined) {
        return s;
      }
      const result = commit(s, { type: "clear", group: groupAt(s.index) }, action.now);
      if (!result.ok) {
        return feedback(s, { kind: result.refusal as Exclude<Refusal, "already-decided"> });
      }
      return {
        ...result.state,
        feedback: null,
        exit: null,
        announcement: `Undecided, ${of(s, s.index)}`,
      };
    }
    case "undo":
    case "redo": {
      const result = commit(s, { type: action.type }, action.now);
      if (!result.ok) {
        return feedback(s, { kind: result.refusal as Exclude<Refusal, "already-decided"> });
      }
      const group = result.group;
      const index = group === undefined ? -1 : indexOfGroup(s, group);
      const target = index < 0 ? s.index : index;
      const state = group === undefined ? "undecided" : stateOf(result.state.folded, group);
      return {
        ...result.state,
        index: target,
        feedback: null,
        exit: null,
        announcement: `${action.type === "undo" ? "Undone" : "Redone"}, ${of(s, target)}: ${state}`,
      };
    }
    case "next":
      return moved(s, nextIndex(s, s.index));
    case "prev":
      return moved(s, prevIndex(s, s.index));
    case "nextUndecided": {
      const index = nextUndecided(s);
      if (index === null) {
        return feedback(s, { kind: "no-undecided" });
      }
      return moved(s, index);
    }
    case "cycleFilter": {
      const at = FILTERS.indexOf(s.filter);
      const filter = FILTERS[(at + 1) % FILTERS.length] ?? "all";
      return { ...s, filter, announcement: `Showing ${filter === "all" ? "all photos" : filter}` };
    }
    case "dismissFeedback":
      return s.feedback === null ? s : { ...s, feedback: null };
    case "show": {
      const index = s.photos.indexOf(action.photoId);
      return index < 0 ? s : shown(s, index);
    }
    case "firstUndecided": {
      const index = s.photos.findIndex((_, i) => stateAt(s, i) === "undecided");
      return index < 0 ? s : shown(s, index);
    }
  }
}

type Committed =
  | { ok: true; state: SwipeState; group: GroupId | undefined }
  | { ok: false; refusal: Refusal };

function commit(s: SwipeState, intent: Intent, now: number): Committed {
  const proposal = propose(s.folded, intent);
  if (!proposal.ok) {
    return { ok: false, refusal: proposal.refusal };
  }
  const event: DecisionEvent = { seq: nextSeq(s.folded), ts: now, cause: "user", ...proposal.body };
  const result = step(s.folded, event);
  if (!result.ok) {
    // `propose` checked the same preconditions, so this would be a bug in the reducer.
    throw new Error(`decision log refused its own event: ${result.reason}`);
  }
  return {
    ok: true,
    state: { ...s, log: [...s.log, event], folded: result.folded },
    group: groupOf(s.folded, proposal.body),
  };
}

function feedback(s: SwipeState, value: Feedback): SwipeState {
  return { ...s, feedback: value, exit: null, nonce: s.nonce + 1, announcement: announce(value) };
}

/** On one photo with the filter on "all", coming back from the review. */
function shown(s: SwipeState, index: number): SwipeState {
  return {
    ...s,
    index,
    filter: "all",
    feedback: null,
    exit: null,
    announcement: `Photo ${of(s, index)}`,
  };
}

function moved(s: SwipeState, index: number): SwipeState {
  if (index === s.index) {
    return s;
  }
  return {
    ...s,
    index,
    feedback: null,
    exit: null,
    announcement: index >= s.photos.length ? "End of the list" : `Photo ${of(s, index)}`,
  };
}

/** The next photo after `from` that matches the filter, or the end card. */
function nextIndex(s: SwipeState, from: number): number {
  for (let i = from + 1; i < s.photos.length; i += 1) {
    if (matches(s.filter, stateAt(s, i))) {
      return i;
    }
  }
  return s.photos.length;
}

/** The previous photo before `from` that matches the filter, or `from` if there is none. */
function prevIndex(s: SwipeState, from: number): number {
  for (let i = Math.min(from, s.photos.length) - 1; i >= 0; i -= 1) {
    if (matches(s.filter, stateAt(s, i))) {
      return i;
    }
  }
  return from;
}

/** The next undecided photo after the current one, wrapping round; from the end card, the first. */
function nextUndecided(s: SwipeState): number | null {
  const n = s.photos.length;
  for (let k = 1; k <= n; k += 1) {
    const i = (Math.min(s.index, n - 1) + k) % n;
    if (stateAt(s, i) === "undecided" && (i !== s.index || atEnd(s))) {
      return i;
    }
  }
  return null;
}

/** "3 of 24", for announcements. */
const of = (s: SwipeState, index: number): string => `${index + 1} of ${s.photos.length}`;

function announce(value: Feedback): string {
  switch (value.kind) {
    case "already-decided":
      return `Already ${value.state}. Press U to undo`;
    case "already-undecided":
      return "This photo has no decision to undo";
    case "nothing-to-undo":
      return "Nothing to undo";
    case "nothing-to-redo":
      return "Nothing to redo";
    case "no-undecided":
      return "No undecided photos left";
    case "resumed":
      return `Sorting resumed: ${value.decided} of ${value.total} photos decided`;
  }
}
