/**
 * The text form of the decision log and of its fold, a port of the core's
 * `crates/core/src/decisions/codec.rs` (the golden fixtures in `testdata/event-log/`, INV-15).
 * Same rules, same check order, same output byte for byte:
 *
 * - A log is JSON Lines: one event per line, every line ending with `\n`, the empty log being the
 *   empty text. `seq`, `ts`, `cause` and `type` are always present; `group_id` with `decided` and
 *   `cleared`, `verdict` with `decided`, `target_seq` with `undo` and `redo`, and no other key.
 *   Numbers are integers within ±(2^53 − 1); strings never need escapes, so a backslash is
 *   refused. Keys may come in any order, with JSON whitespace; `renderLog` writes them in the
 *   order above with no spaces.
 * - A fold is the `.expected.json` object written by `renderFold` (two-space indent, final
 *   newline): decided groups only, in numeric order; `done` and `undone` bottom first; `corrupt`
 *   only when the fold stopped at a bad event.
 *
 * Offsets in `syntax` errors count UTF-16 code units where the core counts bytes; they are equal
 * on ASCII lines, which every valid line is.
 */
import type { Cause, CorruptReason, DecisionEvent, GroupState, Verdict } from "./events";

export const MAX_SAFE_INTEGER = Number.MAX_SAFE_INTEGER;

export function renderEvent(e: DecisionEvent): string {
  const head = `{"seq":${e.seq},"ts":${e.ts},"cause":"${e.cause}","type":"${e.type}"`;
  switch (e.type) {
    case "decided":
      return `${head},"group_id":${e.group_id},"verdict":"${e.verdict}"}`;
    case "cleared":
      return `${head},"group_id":${e.group_id}}`;
    case "undo":
    case "redo":
      return `${head},"target_seq":${e.target_seq}}`;
  }
}

export function renderLog(events: Iterable<DecisionEvent>): string {
  let s = "";
  for (const e of events) {
    s += `${renderEvent(e)}\n`;
  }
  return s;
}

/** The parts of a fold the text form shows. */
export interface FoldText {
  /** Decided groups only; any order (rendered in numeric order). */
  readonly state: Iterable<readonly [number, GroupState]>;
  readonly done: readonly number[];
  readonly undone: readonly number[];
  readonly corrupt?: { readonly seq: number; readonly reason: CorruptReason } | undefined;
}

export function renderFold(f: FoldText): string {
  const decided = [...f.state].filter(([, st]) => st !== "undecided").sort(([a], [b]) => a - b);
  let s = '{\n  "state": {';
  s += decided.map(([g, st]) => `\n    "${g}": "${st}"`).join(",");
  s += decided.length === 0 ? "},\n" : "\n  },\n";
  s += `  "done": [${f.done.join(", ")}],\n`;
  s += `  "undone": [${f.undone.join(", ")}]`;
  if (f.corrupt !== undefined) {
    s += `,\n  "corrupt": {"seq": ${f.corrupt.seq}, "reason": "${f.corrupt.reason}"}`;
  }
  return `${s}\n}\n`;
}

/** Why a line is not an event (`ParseError` in the core). */
export type ParseError =
  | { readonly kind: "syntax"; readonly at: number; readonly expected: string }
  | { readonly kind: "unknown_key"; readonly key: string }
  | { readonly kind: "duplicate_key"; readonly key: string }
  | { readonly kind: "missing_key"; readonly key: string }
  | { readonly kind: "bad_value"; readonly key: string }
  | { readonly kind: "no_final_newline" };

export function describeParseError(e: ParseError): string {
  switch (e.kind) {
    case "syntax":
      return `at byte ${e.at}: expected ${e.expected}`;
    case "unknown_key":
      return `unexpected key ${JSON.stringify(e.key)}`;
    case "duplicate_key":
      return `key ${JSON.stringify(e.key)} given twice`;
    case "missing_key":
      return `missing key ${JSON.stringify(e.key)}`;
    case "bad_value":
      return `bad value for ${JSON.stringify(e.key)}`;
    case "no_final_newline":
      return "does not end with a newline";
  }
}

/** A line of a log that is not an event; `line` is 1-based. */
export class LogError extends Error {
  readonly line: number;
  readonly error: ParseError;

  constructor(line: number, error: ParseError) {
    super(`line ${line}: ${describeParseError(error)}`);
    this.name = "LogError";
    this.line = line;
    this.error = error;
  }
}

class Refused extends Error {
  readonly error: ParseError;

  constructor(error: ParseError) {
    super(describeParseError(error));
    this.error = error;
  }
}

const fail = (error: ParseError): never => {
  throw new Refused(error);
};

/** Parses a whole log, throwing `LogError`. It does not fold: a parsed log may still be corrupt. */
export function parseLog(text: string): DecisionEvent[] {
  if (text === "") {
    return [];
  }
  if (!text.endsWith("\n")) {
    throw new LogError(text.split("\n").length, { kind: "no_final_newline" });
  }
  return text
    .slice(0, -1)
    .split("\n")
    .map((line, i) => {
      const result = tryParseEvent(line);
      if (!result.ok) {
        throw new LogError(i + 1, result.error);
      }
      return result.event;
    });
}

export type ParsedEvent =
  | { readonly ok: true; readonly event: DecisionEvent }
  | { readonly ok: false; readonly error: ParseError };

/** Parses one event line (no newline). */
export function tryParseEvent(line: string): ParsedEvent {
  try {
    return { ok: true, event: parseFields(flatObject(line)) };
  } catch (e) {
    if (e instanceof Refused) {
      return { ok: false, error: e.error };
    }
    throw e;
  }
}

type Scalar = { readonly int: bigint } | { readonly str: string };

const KEYS = ["seq", "ts", "cause", "type", "group_id", "verdict", "target_seq"] as const;
type Key = (typeof KEYS)[number];

function parseFields(fields: readonly (readonly [string, Scalar])[]): DecisionEvent {
  const values = new Map<Key, Scalar>();
  for (const [k, v] of fields) {
    const key = KEYS.find((known) => known === k);
    if (key === undefined) {
      fail({ kind: "unknown_key", key: k });
    } else if (values.has(key)) {
      fail({ kind: "duplicate_key", key: k });
    } else {
      values.set(key, v);
    }
  }
  const int = (key: Key): number => {
    const v = values.get(key);
    if (v === undefined) {
      return fail({ kind: "missing_key", key });
    }
    if ("int" in v) {
      const abs = v.int < 0n ? -v.int : v.int;
      if (abs <= BigInt(MAX_SAFE_INTEGER)) {
        return Number(v.int);
      }
    }
    return fail({ kind: "bad_value", key });
  };
  const string = (key: Key): string => {
    const v = values.get(key);
    if (v === undefined) {
      return fail({ kind: "missing_key", key });
    }
    return "str" in v ? v.str : fail({ kind: "bad_value", key });
  };
  const unexpected = (key: Key) => {
    if (values.has(key)) {
      fail({ kind: "unknown_key", key });
    }
  };
  const seq = int("seq");
  const ts = int("ts");
  const causeText = string("cause");
  const cause: Cause =
    causeText === "user" || causeText === "reconcile"
      ? causeText
      : fail({ kind: "bad_value", key: "cause" });
  const type = string("type");
  switch (type) {
    case "decided": {
      unexpected("target_seq");
      const verdictText = string("verdict");
      const verdict: Verdict =
        verdictText === "keep" || verdictText === "reject"
          ? verdictText
          : fail({ kind: "bad_value", key: "verdict" });
      return { seq, ts, cause, type, group_id: int("group_id"), verdict };
    }
    case "cleared":
      unexpected("verdict");
      unexpected("target_seq");
      return { seq, ts, cause, type, group_id: int("group_id") };
    case "undo":
    case "redo": {
      unexpected("group_id");
      unexpected("verdict");
      return { seq, ts, cause, type, target_seq: int("target_seq") };
    }
    default:
      return fail({ kind: "bad_value", key: "type" });
  }
}

const I64_MIN = -(2n ** 63n);
const I64_MAX = 2n ** 63n - 1n;

/** A JSON object whose values are all strings without escapes or integers. */
function flatObject(text: string): [string, Scalar][] {
  let i = 0;
  const syntax = (at: number, expected: string): never => fail({ kind: "syntax", at, expected });
  const ws = () => {
    while (i < text.length && " \t\r\n".includes(text.charAt(i))) {
      i += 1;
    }
  };
  const string = (): string => {
    if (text[i] !== '"') {
      return syntax(i, "a string");
    }
    const start = i + 1;
    let j = start;
    for (;;) {
      const c = text[j];
      if (c === undefined) {
        return syntax(j, "the end of the string");
      }
      if (c === '"') {
        break;
      }
      if (c === "\\") {
        return syntax(j, "a string without escapes");
      }
      if (c.charCodeAt(0) < 0x20) {
        return syntax(j, "no control character");
      }
      j += 1;
    }
    i = j + 1;
    return text.slice(start, j);
  };
  const isDigit = (c: string | undefined) => c !== undefined && c >= "0" && c <= "9";
  const number = (): bigint => {
    const start = i;
    let j = i;
    if (text[j] === "-") {
      j += 1;
    }
    const digits = j;
    while (isDigit(text[j])) {
      j += 1;
    }
    if (j === digits || (text[digits] === "0" && j > digits + 1)) {
      return syntax(start, "an integer");
    }
    const after = text[j];
    if (after === "." || after === "e" || after === "E") {
      return syntax(j, "an integer");
    }
    i = j;
    const n = BigInt(text.slice(start, j));
    return n < I64_MIN || n > I64_MAX ? syntax(start, "an integer in range") : n;
  };

  ws();
  if (text[i] !== "{") {
    syntax(i, "'{'");
  }
  i += 1;
  const fields: [string, Scalar][] = [];
  ws();
  if (text[i] === "}") {
    i += 1;
  } else {
    for (;;) {
      ws();
      const key = string();
      ws();
      if (text[i] !== ":") {
        syntax(i, "':'");
      }
      i += 1;
      ws();
      const value: Scalar = text[i] === '"' ? { str: string() } : { int: number() };
      fields.push([key, value]);
      ws();
      if (text[i] === ",") {
        i += 1;
      } else if (text[i] === "}") {
        i += 1;
        break;
      } else {
        syntax(i, "',' or '}'");
      }
    }
  }
  ws();
  if (i !== text.length) {
    syntax(i, "the end of the line");
  }
  return fields;
}
