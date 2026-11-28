/**
 * The demo backend's apply log: the same text as `culla_core::applylog::render` (design/
 * file-engine.md §9.8), so the demo's "Open log" shows what the app writes under
 * `%LOCALAPPDATA%\Culla\logs`. In the app the core renders it and the adapter writes it; the
 * webview never sees the file or its path. Pure: the same inputs give the same text.
 *
 * Escaping follows `culla_core::text` (threat model T6b): control, bidi, zero-width and other
 * invisible characters are written `\u{..}`; in a path component a quote, a `\` and a `{` are
 * escaped too, and in a display string a `{`.
 */

/** A journal state as the core writes it (`OpState::as_str`). */
export type LogState =
  | "pending"
  | "renaming"
  | "copying"
  | "verifying"
  | "committed"
  | "source_removed"
  | "done"
  | "copied_source_kept"
  | "failed";

/** The order of §2.1, used by the summary. */
export const LOG_STATES: readonly LogState[] = [
  "pending",
  "renaming",
  "copying",
  "verifying",
  "committed",
  "source_removed",
  "done",
  "copied_source_kept",
  "failed",
];

/** `FailReason::as_str`. */
export type LogReason =
  | "locked"
  | "conflict"
  | "plan_stale"
  | "access_denied"
  | "verify_mismatch"
  | "short_copy"
  | "source_changed"
  | "source_missing"
  | "lost"
  | "fs_inconsistent"
  | "group_failed"
  | "io";

export interface LogHeader {
  readonly version: string;
  readonly plan: number;
  readonly purpose: "apply" | "redo";
  readonly run: "first" | "resume";
  readonly started: string;
  readonly source: string;
  readonly destination: string | null;
}

export interface LogOp {
  readonly id: number;
  readonly group: number;
  readonly action: "keep" | "reject";
  readonly kind: "rename" | "copy" | "xmove" | "finish";
  /** Components from the source root. */
  readonly src: readonly string[];
  /** Components from the target root. */
  readonly dst: readonly string[];
  readonly root: "destination" | "rejected";
  readonly state: LogState;
  readonly reason: LogReason | null;
  /** The Win32 code of an `io` failure. */
  readonly code: number | null;
  readonly attempts: number;
  /** H1 as 64 lowercase hex digits, once the journal has it. */
  readonly hash: string | null;
}

export type LogPause = "user" | "volumeLost" | "writeProtected" | "diskFull";

export type LogEnd =
  | { readonly kind: "finished" }
  | { readonly kind: "paused"; readonly reason: LogPause }
  | { readonly kind: "cancelled" }
  | { readonly kind: "stopped" }
  | { readonly kind: "internalError" };

export interface LogFooter {
  readonly end: LogEnd;
  readonly ended: string;
}

/** `culla_core::text::needs_escape`. */
export function needsEscape(cp: number): boolean {
  return (
    cp <= 0x1f ||
    (cp >= 0x7f && cp <= 0x9f) ||
    cp === 0xad ||
    cp === 0x61c ||
    cp === 0x180e ||
    (cp >= 0x200b && cp <= 0x200f) ||
    (cp >= 0x2028 && cp <= 0x202e) ||
    (cp >= 0x2060 && cp <= 0x206f) ||
    cp === 0xfeff ||
    (cp >= 0xfff9 && cp <= 0xfffb) ||
    (cp >= 0xe0000 && cp <= 0xe007f)
  );
}

/** `culla_core::text::escaped`: each selected code point written `\u{hex}`. */
export function escaped(s: string, also: (ch: string) => boolean): string {
  let out = "";
  for (const ch of s) {
    const cp = ch.codePointAt(0) ?? 0;
    out += needsEscape(cp) || also(ch) ? `\\u{${cp.toString(16)}}` : ch;
  }
  return out;
}

const display = (s: string): string => escaped(s, (ch) => ch === "{");

const quoted = (components: readonly string[]): string =>
  `"${components.map((c) => escaped(c, (ch) => ch === '"' || ch === "\\" || ch === "{")).join("\\")}"`;

const isAlarm = (op: LogOp): boolean =>
  op.state === "failed" && (op.reason === "lost" || op.reason === "fs_inconsistent");

function stateText(op: LogOp): string {
  if (op.state !== "failed") return op.state;
  if (op.reason === null) return "failed";
  if (op.reason === "io") return `failed (io ${op.code ?? 0})`;
  return `failed (${op.reason})`;
}

function line(op: LogOp): string {
  let s = `op ${op.id}  group ${op.group}  ${op.action}  ${op.kind}  source ${quoted(op.src)} → ${op.root} ${quoted(op.dst)}  ${stateText(op)}`;
  if (op.attempts > 1) s += `  attempts ${op.attempts}`;
  if (op.hash !== null) s += `  blake3 ${op.hash}`;
  return s;
}

const PAUSE_TEXT: Readonly<Record<LogPause, string>> = {
  user: "by the user",
  volumeLost: "a volume went away",
  writeProtected: "the volume is write-protected",
  diskFull: "the destination is full",
};

function endText(end: LogEnd): string {
  switch (end.kind) {
    case "finished":
      return "finished";
    case "paused":
      return `paused (${PAUSE_TEXT[end.reason]})`;
    case "cancelled":
      return "cancelled";
    case "stopped":
      return "stopped: the journal could not be written; resumes on the next launch";
    case "internalError":
      return "stopped by an internal error in Culla; report it with this log";
  }
}

/** The log of one run of a plan, ending with a newline; lines end with `\n` only. */
export function renderLog(header: LogHeader, ops: readonly LogOp[], footer: LogFooter): string {
  const o: string[] = [];
  o.push("Culla apply log");
  o.push(`version      ${display(header.version)}`);
  o.push(
    `plan         ${header.plan} (${header.purpose}, ${header.run === "first" ? "first run" : "resume"})`,
  );
  o.push(`started      ${display(header.started)}`);
  o.push(`source       ${display(header.source)}`);
  o.push(`destination  ${header.destination === null ? "none" : display(header.destination)}`);
  o.push("rejected     the _rejected folder in the source");

  const alarms = ops.filter(isAlarm);
  if (alarms.length > 0) {
    o.push("");
    o.push(
      "alarms: a file is not where the journal says. Run chkdsk on the volume before anything else.",
    );
    for (const op of alarms) o.push(`  ${line(op)}`);
  }

  o.push("");
  if (ops.length === 0) {
    o.push("operations: none");
  } else {
    o.push("operations, in plan order");
    for (const op of ops) o.push(`  ${line(op)}`);
  }

  o.push("");
  o.push("summary");
  o.push(`  operations          ${ops.length}`);
  for (const state of LOG_STATES) {
    const n = ops.filter((op) => op.state === state).length;
    if (n > 0) o.push(`  ${state.padEnd(20)}${n}`);
  }
  if (alarms.length > 0) o.push(`  ${"alarms".padEnd(20)}${alarms.length}`);
  o.push(`end          ${endText(footer.end)}`);
  o.push(`ended        ${display(footer.ended)}`);
  return `${o.join("\n")}\n`;
}

/** `2026-11-28 09:14:03 +01:00`, in the PC's time zone, as the adapter formats the log's times. */
export function logTime(date: Date): string {
  const pad = (n: number, width = 2) => String(n).padStart(width, "0");
  const offset = -date.getTimezoneOffset();
  const sign = offset >= 0 ? "+" : "-";
  const abs = Math.abs(offset);
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())} ${sign}${pad(Math.floor(abs / 60))}:${pad(abs % 60)}`;
}
