/**
 * Decision events, as in ADR 0005, ADR 0024 and design/file-engine.md §2.1 and §5.1.
 *
 * The Rust types are the source of this shape and will be exported to TS (ADR 0019); until the
 * core's `decisions` module and its golden fixtures land (M2, US-10), this file is the TS side's
 * own copy. Its JSON form (camelCase, `body.type` as the tag, string group ids) is proposed by the
 * agent and awaiting Margaux (SPEC §10): the golden test reads fixtures in this form.
 */

export type Verdict = "keep" | "reject";

export type GroupState = "undecided" | "kept" | "rejected";

/** `reconcile` is only ever set on `cleared`, by the core when a file joins a decided group. */
export type Cause = "user" | "reconcile";

export type EventBody =
  | { readonly type: "decided"; readonly groupId: string; readonly verdict: Verdict }
  | { readonly type: "cleared"; readonly groupId: string }
  | { readonly type: "undo"; readonly targetSeq: number }
  | { readonly type: "redo"; readonly targetSeq: number };

export interface DecisionEvent {
  readonly seq: number;
  /** Unix milliseconds (UTC). */
  readonly ts: number;
  readonly cause: Cause;
  readonly body: EventBody;
}

export function stateFor(verdict: Verdict): GroupState {
  return verdict === "keep" ? "kept" : "rejected";
}

/** Reads one event from untrusted JSON (a fixture line or a backend reply). Throws on any other shape. */
export function parseEvent(raw: unknown): DecisionEvent {
  if (!isObject(raw)) {
    throw new TypeError("event is not an object");
  }
  const { seq, ts, cause, body } = raw;
  if (!isSeq(seq)) {
    throw new TypeError("event seq is not a positive integer");
  }
  if (typeof ts !== "number" || !Number.isFinite(ts)) {
    throw new TypeError(`event ${seq}: ts is not a number`);
  }
  if (cause !== "user" && cause !== "reconcile") {
    throw new TypeError(`event ${seq}: unknown cause`);
  }
  return { seq, ts, cause, body: parseBody(seq, body) };
}

function parseBody(seq: number, raw: unknown): EventBody {
  if (!isObject(raw)) {
    throw new TypeError(`event ${seq}: body is not an object`);
  }
  switch (raw.type) {
    case "decided": {
      const { groupId, verdict } = raw;
      if (typeof groupId !== "string" || (verdict !== "keep" && verdict !== "reject")) {
        throw new TypeError(`event ${seq}: bad decided body`);
      }
      return { type: "decided", groupId, verdict };
    }
    case "cleared": {
      const { groupId } = raw;
      if (typeof groupId !== "string") {
        throw new TypeError(`event ${seq}: bad cleared body`);
      }
      return { type: "cleared", groupId };
    }
    case "undo":
    case "redo": {
      const { targetSeq } = raw;
      if (!isSeq(targetSeq)) {
        throw new TypeError(`event ${seq}: bad ${raw.type} body`);
      }
      return { type: raw.type, targetSeq };
    }
    default:
      throw new TypeError(`event ${seq}: unknown body type`);
  }
}

const isObject = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

const isSeq = (value: unknown): value is number =>
  typeof value === "number" && Number.isSafeInteger(value) && value > 0;
