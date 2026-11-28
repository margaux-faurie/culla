/**
 * The apply screen's words and state from the backend's `ApplyStatus` (US-36, US-38). Pure, no
 * DOM, so Vitest covers it. Paths and volume labels come from the backend as display strings and
 * are printed as they are: this module counts and words, it never builds a path.
 *
 * Copy follows the design canvas (Apply board) where it has it; any other wording is proposed by
 * the agent (stories.md M4, SPEC §10).
 */
import type {
  ApplyProgress,
  ApplyReport,
  ApplyState,
  CurrentFile,
  FailReason,
  ReportFile,
  ReportGroup,
} from "../backend/types";
import { count, duration, formatBytes } from "../review/model";

/**
 * The stop button's label: the open question of US-36. SPEC §4.2 and file-engine §9.5 stop after
 * the current file, which the merged executor does; the canvas says "Stop after this photo", which
 * reads as if the whole group finished first. Proposed: follow the engine. Switch it here.
 */
export const STOP_LABEL = "Stop after this file";
/** The words shown at once when the stop is asked for. */
export const STOPPING_TEXT =
  "Stopping after this file. It finishes first, copy, check and all, then nothing new starts.";

/**
 * Whether an apply found interrupted at launch rolls forward on its own once its volumes are
 * present, or waits for "Continue the apply" (US-38, open: file-engine §10.1, architecture §6.5).
 * Proposed: on its own, with pause and stop available. Switch it here.
 */
export const ROLL_FORWARD_ON_ITS_OWN = true;

/** How often the screen asks the backend where the apply stands (ms). */
export const POLL_MS = 250;
/** Progress is announced at most every this many photos or this many ms (US-36, proposed). */
export const ANNOUNCE_PHOTOS = 10;
export const ANNOUNCE_MS = 5000;

const capital = (s: string): string => s.charAt(0).toUpperCase() + s.slice(1);

/** The heading while the apply runs: the canvas's "Moving your photos", or what it does. */
export function runningTitle(progress: ApplyProgress): string {
  switch (progress.mode) {
    case "move":
      return "Moving your photos";
    case "copy":
      return "Copying your photos";
    case "mixed":
      return "Moving and copying your photos";
  }
}

/** `61 of 138 photos done. You can keep using your computer.` (canvas). */
export const doneLine = (p: ApplyProgress): string =>
  `${p.photosDone.toLocaleString("en-GB")} of ${count(p.photos, "photo")} done. You can keep using your computer.`;

/** `About 2 minutes left`, `Under a minute left`, `A few seconds left`. */
export const timeLeft = (p: ApplyProgress): string =>
  p.secondsLeft <= 0 ? "A few seconds left" : `${capital(duration(p.secondsLeft))} left`;

/** `1.0 of 2.1 GB`: the part done in the unit of the whole. */
export function bytesLine(p: ApplyProgress): string {
  const total = formatBytes(p.bytes);
  const unit = total.split(" ")[1] ?? "B";
  const scale: Readonly<Record<string, number>> = {
    B: 1,
    KB: 1e3,
    MB: 1e6,
    GB: 1e9,
    TB: 1e12,
  };
  const div = scale[unit] ?? 1;
  const done = p.bytesDone / div;
  const shown = div === 1 ? String(Math.round(done)) : done.toFixed(div === 1e3 ? 0 : 1);
  return `${shown} of ${total}`;
}

/** The progress bar's value, 0 to 100, by bytes (each file's verification reads included). */
export const percent = (p: ApplyProgress): number =>
  p.bytes === 0
    ? p.photos === 0
      ? 100
      : Math.round((p.photosDone / p.photos) * 100)
    : Math.min(100, Math.round((p.bytesDone / p.bytes) * 100));

/** The words for a pause or a stop asked for and not reached yet: shown at once (NFR-PERF-13). */
export function haltText(state: ApplyState): string | null {
  if (state.kind !== "running" || state.halt === null) return null;
  return state.halt === "pause"
    ? "Pausing after this file. It finishes first, so nothing is left half-done."
    : STOPPING_TEXT;
}

/** Why the apply is paused, and what to do. */
export function pausedText(state: ApplyState): string | null {
  if (state.kind !== "paused") return null;
  const volume = state.volume ?? "The drive";
  switch (state.reason) {
    case "user":
      return "Paused. Nothing is half-done. Continue when you are ready.";
    case "volumeLost":
      return state.back
        ? `${volume} is back. Continue when you are ready: the apply carries on from where it stopped.`
        : `${volume} is not connected. Connect it again: nothing is lost, and the apply carries on from where it stopped.`;
    case "diskFull":
      return `${volume} is full. Free some space on it, then continue. The file that was being copied starts again.`;
    case "writeProtected":
      return `${volume} is write-protected. If it is a memory card, slide the lock switch on its side up, away from Lock, then continue.`;
  }
}

/**
 * Why Stop is unavailable while a pause's volume is missing: a stop is a run, which first looks
 * at the cut file, and it cannot while the volume is away (SPEC §10, proposed).
 */
export const stopBlockedText = (volume: string | null): string =>
  `Connect ${volume ?? "the drive"} first: Culla can stop only once it can see the file it was working on.`;

/** Said when a pause, stop or continue did not reach the engine: the button works again. */
export function askFailedText(what: "pause" | "cancel" | "continue"): string {
  const verb = what === "pause" ? "pause" : what === "cancel" ? "stop" : "carry on";
  return `Culla could not ask the apply to ${verb} just now. Try again.`;
}

/** Continue is possible: a pause whose volume is present. */
export const canContinue = (state: ApplyState): boolean => state.kind === "paused" && state.back;

/** The launch notice (US-38, proposed wording). */
export function interruptedText(rollsOnItsOwn: boolean): string {
  const base =
    "An apply was interrupted. Each photo is either in its original place or checked at its new one. Culla will tell you about anything that needs attention.";
  return rollsOnItsOwn ? `${base} Culla is finishing it.` : base;
}

/** While a volume of the interrupted apply is missing (FE §10.1, proposed wording). */
export const missingText = (volume: string): string =>
  `Connect ${volume} to finish it. Culla touches nothing until it is back.`;

/** `MGF_7502 · JPEG, NEF and 1 edit file` (canvas). */
export function photoLine(c: CurrentFile): string {
  const parts: string[] = [];
  if (c.jpeg > 0) parts.push(c.jpeg === 1 ? "JPEG" : `${c.jpeg} JPEG`);
  if (c.nef > 0) parts.push(c.nef === 1 ? "NEF" : `${c.nef} NEF`);
  if (c.edit > 0) parts.push(count(c.edit, "edit file"));
  const files =
    parts.length <= 1 ? (parts[0] ?? "") : `${parts.slice(0, -1).join(", ")} and ${parts.at(-1)}`;
  return `${c.photoName} · ${files}`;
}

/** `File 2 of 3: MGF_7502.NEF`, or the name alone for a photo of one file. */
export const fileLine = (c: CurrentFile): string =>
  c.count === 1 ? c.fileName : `File ${c.index} of ${c.count}: ${c.fileName}`;

export type StepState = "done" | "current" | "next";

export interface StepLine {
  readonly text: string;
  readonly state: StepState;
}

/**
 * The current file's steps in words (canvas, file-engine §9.3): a rename is one step; a copy is
 * copied, read back and checked; a move to another drive then lets the original go, once every
 * file of the photo is checked.
 */
export function stepLines(c: CurrentFile): readonly StepLine[] {
  if (c.kind === "rename") {
    return [{ text: `Moving to ${c.to}, on the same drive`, state: "current" }];
  }
  const order = ["copy", "check", "source"] as const;
  const at: Record<CurrentFile["step"], (typeof order)[number]> = {
    renaming: "copy",
    retrying: "copy",
    copying: "copy",
    verifying: "check",
    reverifying: "check",
    removingSource: "source",
  };
  const now = order.indexOf(at[c.step]);
  const stateAt = (i: number): StepState => (i < now ? "done" : i === now ? "current" : "next");
  const copy =
    c.step === "retrying"
      ? `Another app has ${c.fileName} open. Trying again`
      : now === 0
        ? `Copying to ${c.to}`
        : `Copied to ${c.to}`;
  const check =
    c.step === "reverifying"
      ? "Checking the copy again, from scratch, before the original goes"
      : "Reading the copy back and checking it matches";
  const source =
    c.kind === "xmove"
      ? now === 2
        ? `The original leaves ${c.from}`
        : `Then the original leaves ${c.from}`
      : c.cameraProtected
        ? `The original stays in ${c.from}: the camera protected it`
        : `The original stays in ${c.from}`;
  return [
    { text: copy, state: stateAt(0) },
    { text: check, state: stateAt(1) },
    { text: source, state: c.kind === "xmove" ? stateAt(2) : "next" },
  ];
}

export interface Announced {
  readonly at: number;
  readonly done: number;
}

/**
 * Progress for the live region, at most every 10 photos or 5 s, and only when it changed
 * (US-36, proposed; NFR-A11Y-09). Null when nothing is to be said.
 */
export function progressAnnouncement(
  last: Announced,
  now: number,
  p: ApplyProgress,
): string | null {
  if (p.photosDone === last.done) return null;
  if (p.photosDone - last.done < ANNOUNCE_PHOTOS && now - last.at < ANNOUNCE_MS) return null;
  return `${p.photosDone.toLocaleString("en-GB")} of ${count(p.photos, "photo")} done.`;
}

// --- The report (US-36, file-engine §9.6) ---

/** What became of one photo, in the order the report shows them. */
export type Outcome =
  /** `lost` or `fs_inconsistent`: run chkdsk first (§10.2). */
  | "alarm"
  /** A copy did not match when read back: it points at the reader or the card (§9.4). */
  | "mismatch"
  | "failed"
  /** A move to another drive with files at both places (§9.5). */
  | "copiedSourceKept"
  /** Some files moved, the others still where they were (§9.5). */
  | "split"
  | "notStarted"
  | "done";

export const OUTCOME_ORDER: readonly Outcome[] = [
  "alarm",
  "mismatch",
  "failed",
  "copiedSourceKept",
  "split",
  "notStarted",
  "done",
];

const isAlarm = (f: ReportFile): boolean =>
  f.state === "failed" && (f.reason === "lost" || f.reason === "fsInconsistent");

/** A file is where the plan sends it (a rename held at committed counts, §9.3 step 4). */
const placed = (f: ReportFile): boolean =>
  f.state === "done" || f.state === "committed" || f.state === "sourceRemoved";

export function outcomeOf(g: ReportGroup): Outcome {
  const files = g.files;
  if (files.some(isAlarm)) return "alarm";
  if (files.some((f) => f.reason === "verifyMismatch")) return "mismatch";
  if (files.some((f) => f.state === "copiedSourceKept")) return "copiedSourceKept";
  if (files.every(placed)) return "done";
  if (files.some(placed)) return "split";
  if (files.every((f) => f.state === "pending")) return "notStarted";
  return "failed";
}

/** The reason a group failed: its first own reason, not `group_failed`; `stopped` after a stop. */
export function groupReason(
  g: ReportGroup,
  end: ApplyReport["end"],
): FailReason | "stopped" | null {
  const own = g.files.find((f) => f.state === "failed" && f.reason !== "groupFailed");
  if (own?.reason !== undefined && own.reason !== null) return own.reason;
  if (g.files.some((f) => f.state === "failed"))
    return end === "cancelled" ? "stopped" : "groupFailed";
  return null;
}

export function reasonText(reason: FailReason | "stopped", code: number | null = null): string {
  switch (reason) {
    case "locked":
      return "another app kept it open (Lightroom, Explorer or an antivirus), so Culla skipped it after trying again";
    case "conflict":
      return "a file with its name appeared where it was going. Nothing was replaced";
    case "planStale":
      return "the drives changed since the plan was made";
    case "accessDenied":
      return "Windows refused access to it";
    case "verifyMismatch":
      return "the copy did not match the original when read back. The original is kept and the copy removed. The card or the card reader may be faulty: try another reader before retrying";
    case "shortCopy":
      return "the copy came out shorter than the original. The original is kept";
    case "sourceChanged":
      return "it changed since the plan was made, so Culla left it alone";
    case "sourceMissing":
      return "it was no longer there";
    case "lost":
      return "it is in neither place Culla expected. Nothing was touched";
    case "fsInconsistent":
      return "the drive shows it in two places at once. Nothing was touched";
    case "groupFailed":
      return "another file of this photo failed, so none of its originals went";
    case "stopped":
      return "the apply stopped before it";
    case "io":
      return code === null ? "a drive error stopped it" : `a drive error stopped it (code ${code})`;
  }
}

/** What happened to one file, in words. */
export function fileText(f: ReportFile, g: ReportGroup, end: ApplyReport["end"]): string {
  switch (f.state) {
    case "done":
    case "committed":
    case "sourceRemoved":
      if (g.kind === "copy") {
        return g.cameraProtected
          ? `${f.fileName}: copied to ${f.to}, original kept (protected on the camera)`
          : `${f.fileName}: copied to ${f.to}`;
      }
      return g.kind === "xmove"
        ? `${f.fileName}: moved to ${f.to}, checked`
        : `${f.fileName}: moved to ${f.to}`;
    case "copiedSourceKept":
      return `${f.fileName}: copied, source kept. Checked at ${f.to}, and the original is still in place`;
    case "pending":
      return `${f.fileName}: not started, still at ${f.from}`;
    case "failed": {
      const reason =
        f.reason === "groupFailed" && groupReason(g, end) === "stopped"
          ? "stopped"
          : (f.reason ?? "groupFailed");
      const still = isAlarm(f) ? "" : ` Still at ${f.from}.`;
      return `${f.fileName}: ${reasonText(reason, f.code)}.${still}`;
    }
    // A report should not hold a file mid-step (the next run looks at it first, §10.2), but every
    // journal state has words, so none is ever shown raw.
    case "renaming":
      return `${f.fileName}: the apply stopped while it was being moved. Culla checks where it is before it does anything else with it`;
    case "copying":
      return `${f.fileName}: the apply stopped while it was being copied. The original is still at ${f.from}`;
    case "verifying":
      return `${f.fileName}: copied, but the check did not finish. The original is still at ${f.from}`;
  }
}

/** The count's label in the report's summary. */
export function outcomeLabel(outcome: Outcome, n: number): string {
  const photos = count(n, "photo");
  switch (outcome) {
    case "alarm":
      return `${photos} not where expected`;
    case "mismatch":
      return n === 1 ? "1 copy did not match" : `${n.toLocaleString("en-GB")} copies did not match`;
    case "failed":
      return `${photos} not applied`;
    case "copiedSourceKept":
      return `${photos} copied, source kept`;
    case "split":
      return `${photos} split`;
    case "notStarted":
      return `${photos} not started`;
    case "done":
      return `${photos} done`;
  }
}

/** Each section's heading. */
export function outcomeTitle(outcome: Outcome): string {
  switch (outcome) {
    case "alarm":
      return "Run chkdsk first";
    case "mismatch":
      return "A copy did not match";
    case "failed":
      return "Not applied";
    case "copiedSourceKept":
      return "Copied, source kept";
    case "split":
      return "Split between folders";
    case "notStarted":
      return "Not started";
    case "done":
      return "Done";
  }
}

/** What a section says before its photos. */
export function outcomeNote(outcome: Outcome, report: ApplyReport): string {
  switch (outcome) {
    case "alarm":
      return `A file is not where Culla's journal says it should be. Run chkdsk on ${chkdskVolumes(report)} before you use it for anything else, then check these photos.`;
    case "mismatch":
      return "Reading the copy back gave different bytes from the original. That points at the card reader or the card. The originals are kept.";
    case "failed":
      return "These photos stay where they were.";
    case "copiedSourceKept":
      return "Each file is at both places: the copy is checked, and the original is kept. Finish moves checks each copy again, then lets the original go.";
    case "split":
      return "Some files of these photos are at their new place and the others still where they were: the apply stopped, or another of their files failed, part-way through them.";
    case "notStarted":
      return report.end === "cancelled"
        ? "The apply stopped before these photos. They stay where they are."
        : "These photos stay where they are.";
    case "done":
      return "These photos are at their new place.";
  }
}

const chkdskVolumes = (report: ApplyReport): string =>
  report.destinationVolume === null || report.destinationVolume === report.sourceVolume
    ? report.sourceVolume
    : `${report.sourceVolume} and ${report.destinationVolume}`;

export interface ReportSection {
  readonly outcome: Outcome;
  readonly groups: readonly ReportGroup[];
}

/** The report's groups by outcome, alarms first (FE §10.2), each section in plan order. */
export function reportSections(report: ApplyReport): readonly ReportSection[] {
  return OUTCOME_ORDER.map((outcome) => ({
    outcome,
    groups: report.groups.filter((g) => outcomeOf(g) === outcome),
  })).filter((s) => s.groups.length > 0);
}

/** `12 done · 1 copied, source kept · 2 held back`, in the report's order. */
export function summaryParts(report: ApplyReport): readonly string[] {
  const parts = reportSections(report).map((s) => outcomeLabel(s.outcome, s.groups.length));
  if (report.held.length > 0) parts.push(`${count(report.held.length, "photo")} held back`);
  return parts;
}

const PROBLEMS: ReadonlySet<Outcome> = new Set([
  "alarm",
  "mismatch",
  "failed",
  "copiedSourceKept",
  "split",
]);

export function reportTitle(report: ApplyReport): string {
  if (report.end === "cancelled") return "Apply stopped";
  return report.groups.some((g) => PROBLEMS.has(outcomeOf(g)))
    ? "Apply finished, with photos to check"
    : "Apply finished";
}

export function reportLead(report: ApplyReport): string {
  const done = report.groups.filter((g) => outcomeOf(g) === "done").length;
  const of = `${done.toLocaleString("en-GB")} of ${count(report.groups.length, "photo")} done. Nothing was deleted.`;
  return report.end === "cancelled" ? `Stopped after the current file. ${of}` : of;
}

export interface ReportActions {
  /** Failed groups no file of which reached committed (§9.6). */
  readonly retry: number;
  /** Groups with "copied, source kept" files. */
  readonly finish: number;
  /** Groups never started: a new plan takes them (§9.6). */
  readonly remaining: number;
}

export function reportActions(report: ApplyReport): ReportActions {
  return {
    retry: report.groups.filter((g) => g.retryable).length,
    finish: report.groups.filter((g) => g.files.some((f) => f.state === "copiedSourceKept")).length,
    remaining: report.groups.filter((g) => outcomeOf(g) === "notStarted").length,
  };
}
