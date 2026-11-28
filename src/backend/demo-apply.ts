/**
 * The demo backend's apply: a simulated run of a confirmed plan through the journal states of
 * design/file-engine.md §9.2, following the merged executor (`culla_core::journal::run`):
 *
 * - groups in plan order, rejects first (§9.5); one file at a time;
 * - a pause or a stop is asked for at once and taken between groups and before each operation, so
 *   the current file's whole operation (copy, verify, commit) finishes first;
 * - a stop part-way through a cross-volume group fails the group: its committed files end
 *   `copied_source_kept` (both files kept), the others `failed (group_failed)`; a rename group can
 *   stay split, some files moved and the others where they were (§9.5);
 * - a failed file fails its group before any of its sources goes; a cross-volume group's sources
 *   go only once every file of the group is committed;
 * - a run that starts again after a pause, a pulled card or a crash first looks at each file that
 *   is not final, as the recovery table does (§10.2): a copy cut short starts again, a lost file
 *   is an alarm;
 * - renames stay `committed` until the plan closes (§9.3 step 4);
 * - each run ends with a log, rendered as the core renders it (§9.8).
 *
 * Nothing here touches a disk: the web demo never moves a file (SPEC §5.5). Time is simulated: the
 * caller advances it, so tests are exact. The rate is the review's provisional 90 MB/s, each
 * copied byte read three times (SPEC §9).
 */
import {
  type LogEnd,
  type LogOp,
  type LogReason,
  type LogState,
  logTime,
  renderLog,
} from "./demo-log";
import type {
  ApplyEnd,
  ApplyKind,
  ApplyProgress,
  ApplyReport,
  ApplyState,
  ApplyStep,
  CurrentFile,
  FailReason,
  Halt,
  HeldPhoto,
  OpState,
  PauseReason,
  ReportGroup,
  ReviewPlan,
} from "./types";

/** A rename on one volume, simulated (ms). */
export const RENAME_MS = 120;
/** One source removal through the audited function (ms). */
export const REMOVE_MS = 60;
/** The backoff for a held file: 100 + 200 + 400 + 800 + 1600 ms (§9.4). */
export const RETRY_MS = 3100;
/** How long a pulled card stays out in the demo before it is "inserted" again (ms). */
export const RETURN_MS = 1500;

/** What can go wrong during a demo apply (`?scenario=`, demo only). */
export interface ApplyScenario {
  /** Divides the simulated rate: 0.2 makes a slow card reader. */
  readonly speed?: number;
  /**
   * The source card is pulled half-way through a file. `clean`: during the first copy (or the
   * first rename when nothing is copied), which starts again once the card is back. `lost`: during
   * the first rename (or the first verify), and when the card is back the file is in neither
   * place, an alarm (§10.2).
   */
  readonly pull?: "clean" | "lost";
  /** The destination fills up half-way through the first copy (§9.4). */
  readonly diskFull?: boolean;
  /** The first copy does not match when read back (§9.4). */
  readonly verifyMismatch?: boolean;
}

export interface SimConfig {
  readonly planId: string;
  /** `plan.id` in the log. */
  readonly planNumber: number;
  readonly sourcePath: string;
  readonly rejectedPath: string;
  /** Null when the plan keeps nothing. */
  readonly destinationPath: string | null;
  readonly sourceVolume: string;
  readonly destinationVolume: string | null;
  /** Bytes per second. */
  readonly rate: number;
  readonly scenario: ApplyScenario;
  /** Group names another app holds open during the first run (P12); closed for "Retry failed". */
  readonly locked: ReadonlySet<string>;
  readonly held: readonly HeldPhoto[];
  /** The wall clock, for the log's times. */
  readonly wall: () => Date;
  readonly version: string;
}

type FileKind = "jpeg" | "nef" | "edit";

export interface SimFile {
  readonly id: number;
  /** The group's id in the log. */
  readonly group: number;
  readonly photoId: string;
  readonly photoName: string;
  readonly fileName: string;
  readonly action: "keep" | "reject";
  readonly kind: ApplyKind;
  readonly from: string;
  readonly to: string;
  readonly srcRel: readonly string[];
  readonly dstRel: readonly string[];
  readonly bytes: number;
  readonly fileKind: FileKind;
  readonly cameraProtected: boolean;
  state: OpState;
  reason: FailReason | null;
  attempts: number;
  hash: string | null;
  everCommitted: boolean;
  /** "Finish moves" re-verifies this `copied_source_kept` file before it is committed again. */
  reverify: boolean;
}

interface Work {
  readonly file: number;
  readonly step: ApplyStep;
  readonly total: number;
  left: number;
}

interface Run {
  readonly id: string;
  readonly kind: "first" | "resume";
  readonly started: string;
  /** The log, once the run has ended; null for a run cut by a crash (no log is written). */
  text: string | null;
}

export type SimPhase = "running" | "paused" | "interrupted" | "ended" | "closed";

export interface Sim {
  readonly cfg: SimConfig;
  readonly files: SimFile[];
  readonly groups: readonly { readonly start: number; readonly end: number }[];
  phase: SimPhase;
  halt: Halt | null;
  pause: { reason: PauseReason; volume: string | null; back: boolean; waited: number } | null;
  /** Interrupted at launch and waiting for this volume. */
  missing: { volume: string; waited: number } | null;
  end: ApplyEnd | null;
  work: Work | null;
  cursor: number;
  entered: boolean;
  /** The groups a "Retry failed" or "Finish moves" run is limited to; null for all. */
  scope: Set<number> | null;
  readonly runs: Run[];
  fired: { pull: boolean; full: boolean; mismatch: boolean };
  /** The file a `lost` pull left in neither place. */
  lostFile: number | null;
  locksClosed: boolean;
}

const FINAL: ReadonlySet<OpState> = new Set(["done", "copiedSourceKept", "failed"]);

const kindOf = (kind: ReviewPlan["operations"][number]["kind"]): ApplyKind =>
  kind === "move" ? "rename" : kind === "moveAcrossDrives" ? "xmove" : "copy";

const relative = (path: string, root: string): string[] => path.slice(root.length + 1).split("\\");

const fileKindOf = (fileName: string, photoName: string): FileKind => {
  const rest = fileName.slice(photoName.length).toUpperCase();
  if (rest === ".NEF") return "nef";
  if (rest === ".JPG" || rest === ".JPEG") return "jpeg";
  return "edit";
};

/** A made-up H1 for the demo's log: 64 hex digits from the name and size. */
export function demoHash(name: string, bytes: number): string {
  let out = "";
  for (let round = 0; round < 8; round += 1) {
    let h = (2166136261 ^ round) >>> 0;
    for (const ch of `${name}:${bytes}:${round}`) {
      h = Math.imul(h ^ (ch.codePointAt(0) ?? 0), 16777619) >>> 0;
    }
    out += h.toString(16).padStart(8, "0");
  }
  return out;
}

/**
 * The simulated run of `plan`, started: the first run. `groupOf` gives a photo's group id for the
 * log. Rejects go first, then keeps, each in the plan's order (§9.5).
 */
export function createSim(
  plan: ReviewPlan,
  cfg: SimConfig,
  groupOf: (photoId: string) => number,
): Sim {
  const ordered = [
    ...plan.operations.filter((op) => op.action === "reject"),
    ...plan.operations.filter((op) => op.action === "keep"),
  ];
  const files: SimFile[] = ordered.map((op, i) => {
    const root = op.action === "keep" ? (cfg.destinationPath ?? "") : cfg.rejectedPath;
    return {
      id: i + 1,
      group: groupOf(op.photoId),
      photoId: op.photoId,
      photoName: op.photoName,
      fileName: op.from.slice(op.from.lastIndexOf("\\") + 1),
      action: op.action,
      kind: kindOf(op.kind),
      from: op.from,
      to: op.to,
      srcRel: relative(op.from, cfg.sourcePath),
      dstRel: relative(op.to, root),
      bytes: op.bytes,
      fileKind: fileKindOf(op.from.slice(op.from.lastIndexOf("\\") + 1), op.photoName),
      cameraProtected: op.cameraProtected,
      state: "pending",
      reason: null,
      attempts: 0,
      hash: null,
      everCommitted: false,
      reverify: false,
    };
  });
  const groups: { start: number; end: number }[] = [];
  for (const [i, f] of files.entries()) {
    const last = groups.at(-1);
    if (last !== undefined && files[last.start]?.photoId === f.photoId) {
      last.end = i + 1;
    } else {
      groups.push({ start: i, end: i + 1 });
    }
  }
  const sim: Sim = {
    cfg,
    files,
    groups,
    phase: "running",
    halt: null,
    pause: null,
    missing: null,
    end: null,
    work: null,
    cursor: 0,
    entered: false,
    scope: null,
    runs: [],
    fired: { pull: false, full: false, mismatch: false },
    lostFile: null,
    locksClosed: false,
  };
  newRun(sim, "first");
  return sim;
}

function newRun(sim: Sim, kind: "first" | "resume") {
  sim.runs.push({
    id: `${sim.cfg.planId}-run-${sim.runs.length + 1}`,
    kind,
    started: logTimeOf(sim),
    text: null,
  });
}

const logTimeOf = (sim: Sim): string => logTime(sim.cfg.wall());

const SNAKE: Readonly<Record<OpState, LogState>> = {
  pending: "pending",
  renaming: "renaming",
  copying: "copying",
  verifying: "verifying",
  committed: "committed",
  sourceRemoved: "source_removed",
  done: "done",
  copiedSourceKept: "copied_source_kept",
  failed: "failed",
};

const REASON: Readonly<Record<FailReason, LogReason>> = {
  locked: "locked",
  conflict: "conflict",
  planStale: "plan_stale",
  accessDenied: "access_denied",
  verifyMismatch: "verify_mismatch",
  shortCopy: "short_copy",
  sourceChanged: "source_changed",
  sourceMissing: "source_missing",
  lost: "lost",
  fsInconsistent: "fs_inconsistent",
  groupFailed: "group_failed",
  io: "io",
};

const logOps = (sim: Sim): LogOp[] =>
  sim.files.map((f) => ({
    id: f.id,
    group: f.group,
    action: f.action,
    kind: f.kind,
    src: f.srcRel,
    dst: f.dstRel,
    root: f.action === "keep" ? "destination" : "rejected",
    state: SNAKE[f.state],
    reason: f.reason === null ? null : REASON[f.reason],
    code: null,
    attempts: f.attempts,
    hash: f.hash,
  }));

/** Ends the current run and writes its log (§9.8). */
function endRun(sim: Sim, end: LogEnd) {
  const run = sim.runs.at(-1);
  if (run === undefined || run.text !== null) return;
  run.text = renderLog(
    {
      version: sim.cfg.version,
      plan: sim.cfg.planNumber,
      purpose: "apply",
      run: run.kind,
      started: run.started,
      source: sim.cfg.sourcePath,
      destination: sim.files.some((f) => f.action === "keep") ? sim.cfg.destinationPath : null,
    },
    logOps(sim),
    { end, ended: logTimeOf(sim) },
  );
}

/** The log of a run, or null for an unknown run or one still going. */
export function logOf(sim: Sim, runId: string): string | null {
  return sim.runs.find((r) => r.id === runId)?.text ?? null;
}

const speedOf = (sim: Sim): number => sim.cfg.scenario.speed ?? 1;

/** Milliseconds of simulated work for `bytes` read `reads` times. */
const readMs = (sim: Sim, bytes: number, reads: number): number =>
  (bytes * reads * 1000) / (sim.cfg.rate * speedOf(sim));

const fixedMs = (sim: Sim, ms: number): number => ms / speedOf(sim);

const inGroup = (sim: Sim, g: number): SimFile[] => {
  const range = sim.groups[g];
  return range === undefined ? [] : sim.files.slice(range.start, range.end);
};

const groupIndexOf = (sim: Sim, file: number): number =>
  sim.groups.findIndex((g) => file >= g.start && file < g.end);

/** A file needs nothing more in this plan: final, or a rename held at committed (§9.3). */
const settled = (f: SimFile): boolean =>
  FINAL.has(f.state) || (f.kind === "rename" && f.state === "committed");

const inScope = (sim: Sim, g: number): boolean => sim.scope === null || sim.scope.has(g);

/** §9.5 group failure. */
function failGroup(sim: Sim, g: number) {
  for (const f of inGroup(sim, g)) {
    if (f.state === "pending" || f.state === "copying" || f.state === "verifying") {
      f.state = "failed";
      f.reason = "groupFailed";
      f.reverify = false;
    } else if (f.state === "committed" && f.kind === "xmove") {
      f.state = "copiedSourceKept";
    } else if (f.state === "committed" && f.kind === "copy") {
      f.state = "done";
    } else if (f.state === "copiedSourceKept") {
      f.reverify = false;
    }
  }
}

/** A cross-volume group has started: a stop now cuts it short (`halted` in the executor). */
const xmoveStarted = (sim: Sim, g: number): boolean =>
  inGroup(sim, g).some(
    (f) =>
      f.kind === "xmove" &&
      (f.state !== "pending" || f.attempts > 0 || f.everCommitted) &&
      f.state !== "failed",
  );

function stop(sim: Sim, halt: Halt) {
  sim.halt = null;
  if (halt === "pause") {
    sim.phase = "paused";
    sim.pause = { reason: "user", volume: null, back: true, waited: 0 };
    sim.entered = false;
    endRun(sim, { kind: "paused", reason: "user" });
    return;
  }
  const g = sim.cursor;
  if (g < sim.groups.length && xmoveStarted(sim, g)) {
    failGroup(sim, g);
  }
  sim.phase = "ended";
  sim.end = "cancelled";
  endRun(sim, { kind: "cancelled" });
}

/** The recovery table's look at a group whose files may be part-way (§10.2), simplified. */
function enter(sim: Sim, g: number) {
  for (const f of inGroup(sim, g)) {
    const i = f.id - 1;
    if (f.state === "sourceRemoved") {
      f.state = "done";
    } else if (f.state === "renaming" || f.state === "verifying") {
      if (sim.lostFile === i) {
        // S0F0 (S0P0F0): neither place, an alarm; nothing is touched.
        f.state = "failed";
        f.reason = "lost";
        sim.lostFile = null;
      } else if (f.state === "renaming") {
        // S0F1 with the planned size and mtime: the rename was done.
        f.state = "committed";
        f.everCommitted = true;
      }
      // A copy left in verifying is verified again, from its partial (S1P1F0).
    } else if (f.state === "copying") {
      // S1P1F0: the partial is removed and the copy starts again.
      f.state = "pending";
    }
  }
  sim.entered = true;
}

function start(sim: Sim, i: number) {
  const f = sim.files[i];
  if (f === undefined) return;
  if (f.kind === "rename") {
    f.state = "renaming";
    sim.work = { file: i, step: "renaming", total: fixedMs(sim, RENAME_MS), left: 0 };
  } else {
    f.state = "copying";
    f.attempts += 1;
    const held = !sim.locksClosed && sim.cfg.locked.has(f.photoName);
    sim.work = held
      ? { file: i, step: "retrying", total: fixedMs(sim, RETRY_MS), left: 0 }
      : { file: i, step: "copying", total: readMs(sim, f.bytes, 1), left: 0 };
  }
  sim.work.left = sim.work.total;
}

/** Starts the next piece of work, or ends the run. Instant. */
function next(sim: Sim) {
  for (;;) {
    const g = sim.cursor;
    if (g >= sim.groups.length) {
      sim.phase = "ended";
      sim.end = "finished";
      sim.halt = null;
      endRun(sim, { kind: "finished" });
      return;
    }
    const files = inGroup(sim, g);
    if (!inScope(sim, g) || files.every(settled)) {
      sim.cursor += 1;
      sim.entered = false;
      continue;
    }
    if (!sim.entered) {
      if (sim.halt !== null) {
        stop(sim, sim.halt);
        return;
      }
      enter(sim, g);
    }
    if (files.some((f) => f.state === "failed")) {
      failGroup(sim, g);
      sim.cursor += 1;
      sim.entered = false;
      continue;
    }
    const range = sim.groups[g] ?? { start: 0, end: 0 };
    const at = (pred: (f: SimFile) => boolean): number => {
      const k = files.findIndex(pred);
      return k < 0 ? -1 : range.start + k;
    };
    const again = at((f) => f.state === "verifying");
    if (again >= 0) {
      const f = sim.files[again] as SimFile;
      sim.work = {
        file: again,
        step: "verifying",
        total: readMs(sim, f.bytes, 2),
        left: readMs(sim, f.bytes, 2),
      };
      return;
    }
    const reverify = at((f) => f.reverify);
    const pending = at((f) => f.state === "pending");
    if (reverify >= 0 || pending >= 0) {
      if (sim.halt !== null) {
        stop(sim, sim.halt);
        return;
      }
      if (reverify >= 0) {
        const f = sim.files[reverify] as SimFile;
        const ms = readMs(sim, f.bytes, 2);
        sim.work = { file: reverify, step: "reverifying", total: ms, left: ms };
      } else {
        start(sim, pending);
      }
      return;
    }
    const remove = at((f) => f.kind === "xmove" && f.state === "committed");
    if (remove >= 0) {
      const ms = fixedMs(sim, REMOVE_MS);
      sim.work = { file: remove, step: "removingSource", total: ms, left: ms };
      return;
    }
    sim.cursor += 1;
    sim.entered = false;
  }
}

function complete(sim: Sim, work: Work) {
  const f = sim.files[work.file] as SimFile;
  sim.work = null;
  switch (work.step) {
    case "renaming":
      f.state = "committed";
      f.everCommitted = true;
      return;
    case "retrying":
      // Held through the whole backoff: failed (locked), the partial removed (§9.4).
      f.state = "failed";
      f.reason = "locked";
      return;
    case "copying": {
      f.state = "verifying";
      f.hash = demoHash(f.fileName, f.bytes);
      const ms = readMs(sim, f.bytes, 2);
      sim.work = { file: work.file, step: "verifying", total: ms, left: ms };
      return;
    }
    case "verifying":
      if (sim.cfg.scenario.verifyMismatch === true && !sim.fired.mismatch) {
        sim.fired.mismatch = true;
        f.state = "failed";
        f.reason = "verifyMismatch";
        return;
      }
      f.state = f.kind === "copy" ? "done" : "committed";
      f.everCommitted = true;
      return;
    case "reverifying":
      f.state = "committed";
      f.reverify = false;
      return;
    case "removingSource":
      f.state = "done";
      return;
  }
}

/** The file a scenario event waits for, if `work` is it: pull and disk full half-way through. */
function triggerOf(sim: Sim, work: Work): "pull" | "full" | null {
  const s = sim.cfg.scenario;
  const copies = sim.files.some((f) => f.kind !== "rename");
  const renames = sim.files.some((f) => f.kind === "rename");
  if (s.pull !== undefined && !sim.fired.pull) {
    const step =
      s.pull === "clean" ? (copies ? "copying" : "renaming") : renames ? "renaming" : "verifying";
    if (work.step === step) return "pull";
  }
  if (s.diskFull === true && !sim.fired.full && work.step === "copying") return "full";
  return null;
}

function fire(sim: Sim, what: "pull" | "full", work: Work) {
  const f = sim.files[work.file] as SimFile;
  sim.work = null;
  sim.entered = false;
  if (what === "pull") {
    sim.fired.pull = true;
    if (sim.cfg.scenario.pull === "lost") sim.lostFile = work.file;
    // The current file stays in its journal state; the recovery table takes it on return.
    sim.phase = "paused";
    sim.pause = { reason: "volumeLost", volume: sim.cfg.sourceVolume, back: false, waited: 0 };
    endRun(sim, { kind: "paused", reason: "volumeLost" });
    return;
  }
  sim.fired.full = true;
  // Full while copying: back to pending, its partial removed (§9.4).
  f.state = "pending";
  const volume = f.action === "keep" ? sim.cfg.destinationVolume : sim.cfg.sourceVolume;
  sim.phase = "paused";
  sim.pause = { reason: "diskFull", volume, back: true, waited: 0 };
  endRun(sim, { kind: "paused", reason: "diskFull" });
}

/** Lets `ms` of simulated time pass. */
export function advance(sim: Sim, ms: number) {
  let dt = Math.max(0, ms);
  for (let guard = 0; guard < 100_000; guard += 1) {
    if (sim.phase === "paused") {
      const p = sim.pause;
      if (p !== null && !p.back) {
        p.waited += dt;
        if (p.waited >= RETURN_MS) p.back = true;
      }
      return;
    }
    if (sim.phase === "interrupted") {
      const m = sim.missing;
      if (m !== null) {
        m.waited += dt;
        if (m.waited >= RETURN_MS) sim.missing = null;
      }
      return;
    }
    if (sim.phase !== "running") return;
    const work = sim.work;
    if (work === null) {
      next(sim);
      continue;
    }
    if (dt <= 0) return;
    const trigger = triggerOf(sim, work);
    const half = work.total / 2;
    if (trigger !== null && work.left > half && work.left - dt <= half) {
      dt -= work.left - half;
      work.left = half;
      fire(sim, trigger, work);
      continue;
    }
    const used = Math.min(dt, work.left);
    work.left -= used;
    dt -= used;
    if (work.left <= 0) complete(sim, work);
  }
}

/** Asks for a pause or a stop; a stop wins over a pause. From a pause, a stop ends at once. */
export function requestHalt(sim: Sim, halt: Halt) {
  if (sim.phase === "running") {
    sim.halt = halt === "cancel" ? "cancel" : (sim.halt ?? "pause");
    return;
  }
  if (sim.phase === "paused" && halt === "cancel") {
    // A run starts and stops before its first operation: a cut cross-volume group fails.
    newRun(sim, "resume");
    sim.phase = "running";
    sim.pause = null;
    stop(sim, "cancel");
  }
}

/** Continues a pause, or an interrupted plan once its volumes are present: a new run. */
export function resume(sim: Sim): boolean {
  const pausedReady = sim.phase === "paused" && sim.pause?.back === true;
  const interruptedReady = sim.phase === "interrupted" && sim.missing === null;
  if (!pausedReady && !interruptedReady) return false;
  sim.phase = "running";
  sim.pause = null;
  sim.entered = false;
  newRun(sim, "resume");
  return true;
}

/** A crash: the run stops where it is, with no log; the next launch finds it interrupted. */
export function crash(sim: Sim, missing: string | null) {
  sim.work = null;
  sim.halt = null;
  sim.entered = false;
  sim.phase = "interrupted";
  sim.missing = missing === null ? null : { volume: missing, waited: 0 };
}

/** Failed groups none of whose files ever reached committed (§9.6); not alarms. */
export function retryableGroups(sim: Sim): number[] {
  return sim.groups
    .map((_, g) => g)
    .filter((g) => {
      const files = inGroup(sim, g);
      return (
        files.some((f) => f.state === "failed") &&
        files.every((f) => !f.everCommitted && (f.state === "failed" || f.state === "pending")) &&
        !files.some((f) => f.reason === "lost" || f.reason === "fsInconsistent")
      );
    });
}

/** Groups with a file left "copied, source kept". */
export function finishableGroups(sim: Sim): number[] {
  return sim.groups
    .map((_, g) => g)
    .filter((g) => inGroup(sim, g).some((f) => f.state === "copiedSourceKept"));
}

function rerun(sim: Sim, groups: readonly number[]) {
  sim.scope = new Set(groups);
  sim.cursor = 0;
  sim.entered = false;
  sim.phase = "running";
  sim.end = null;
  newRun(sim, "resume");
}

/** "Retry failed": `failed → pending` for those groups, in the same plan. The held files are closed now. */
export function retryFailed(sim: Sim): boolean {
  const groups = sim.phase === "ended" ? retryableGroups(sim) : [];
  if (groups.length === 0) return false;
  for (const g of groups) {
    for (const f of inGroup(sim, g)) {
      if (f.state === "failed") {
        f.state = "pending";
        f.reason = null;
      }
    }
  }
  sim.locksClosed = true;
  rerun(sim, groups);
  return true;
}

/** "Finish moves": each kept copy is checked again from scratch, the failed files run again. */
export function finishMoves(sim: Sim): boolean {
  const groups = sim.phase === "ended" ? finishableGroups(sim) : [];
  if (groups.length === 0) return false;
  for (const g of groups) {
    for (const f of inGroup(sim, g)) {
      if (f.state === "failed" && f.reason !== "lost" && f.reason !== "fsInconsistent") {
        f.state = "pending";
        f.reason = null;
      } else if (f.state === "copiedSourceKept") {
        f.reverify = true;
      }
    }
  }
  sim.locksClosed = true;
  rerun(sim, groups);
  return true;
}

/** Closes the plan (§9.1): renames still committed become done. Only once the run has ended. */
export function closeSim(sim: Sim): boolean {
  if (sim.phase !== "ended") return false;
  for (const f of sim.files) {
    if (f.kind === "rename" && f.state === "committed") f.state = "done";
  }
  sim.phase = "closed";
  return true;
}

/** Names of the groups applied by this plan: some file reached committed (§5.4). */
export const appliedNames = (sim: Sim): string[] => [
  ...new Set(sim.files.filter((f) => f.everCommitted).map((f) => f.photoName)),
];

// --- Snapshots for the UI ---

/** How far a file is, from 0 to 1, by its share of the work (a copy is read three times). */
function fraction(sim: Sim, i: number): number {
  const f = sim.files[i] as SimFile;
  const w = sim.work;
  if (w !== null && w.file === i) {
    const done = w.total === 0 ? 1 : 1 - w.left / w.total;
    if (w.step === "copying") return done / 3;
    if (w.step === "verifying") return 1 / 3 + (2 / 3) * done;
    if (w.step === "removingSource") return 1;
    return f.state === "committed" ? 1 : 0;
  }
  if (f.state === "pending" || f.state === "renaming" || f.state === "copying") return 0;
  if (f.state === "verifying") return 1 / 3;
  if (f.reverify) return 0;
  return 1;
}

const workMs = (sim: Sim, f: SimFile): number => {
  if (f.kind === "rename") return fixedMs(sim, RENAME_MS);
  return readMs(sim, f.bytes, 3) + (f.kind === "xmove" ? fixedMs(sim, REMOVE_MS) : 0);
};

export function progressOf(sim: Sim): ApplyProgress {
  let bytesDone = 0;
  let left = 0;
  const bytes = sim.files.reduce((n, f) => n + f.bytes, 0);
  for (const [i, f] of sim.files.entries()) {
    const frac = fraction(sim, i);
    bytesDone += f.bytes * frac;
    if (!settled(f) && inScope(sim, groupIndexOf(sim, i))) left += (1 - frac) * workMs(sim, f);
  }
  const kinds = new Set(sim.files.map((f) => (f.kind === "copy" ? "copy" : "move")));
  return {
    photosDone: sim.groups.filter((_, g) => inGroup(sim, g).every(settled)).length,
    photos: sim.groups.length,
    bytesDone: Math.round(bytesDone),
    bytes,
    secondsLeft: Math.ceil(left / 1000),
    measured: false,
    mode: kinds.size > 1 ? "mixed" : kinds.has("copy") ? "copy" : "move",
  };
}

export function currentOf(sim: Sim): CurrentFile | null {
  const w = sim.work;
  if (w === null || sim.phase !== "running") return null;
  const f = sim.files[w.file] as SimFile;
  const g = groupIndexOf(sim, w.file);
  const files = inGroup(sim, g);
  const range = sim.groups[g] ?? { start: 0, end: 0 };
  return {
    photoId: f.photoId,
    photoName: f.photoName,
    jpeg: files.filter((x) => x.fileKind === "jpeg").length,
    nef: files.filter((x) => x.fileKind === "nef").length,
    edit: files.filter((x) => x.fileKind === "edit").length,
    fileName: f.fileName,
    index: w.file - range.start + 1,
    count: files.length,
    kind: f.kind,
    step: w.step,
    action: f.action,
    to: f.to.slice(0, f.to.lastIndexOf("\\")),
    from: f.from.slice(0, f.from.lastIndexOf("\\")),
    cameraProtected: f.cameraProtected,
  };
}

export function stateOf(sim: Sim): ApplyState {
  switch (sim.phase) {
    case "running":
      return { kind: "running", halt: sim.halt };
    case "paused": {
      const p = sim.pause ?? { reason: "user" as const, volume: null, back: true };
      return { kind: "paused", reason: p.reason, volume: p.volume, back: p.back };
    }
    case "interrupted":
      return { kind: "interrupted", missing: sim.missing?.volume ?? null };
    case "ended":
    case "closed":
      return { kind: "ended", end: sim.end ?? "finished" };
  }
}

export function reportOf(sim: Sim): ApplyReport | null {
  if (sim.phase !== "ended" && sim.phase !== "closed") return null;
  const retryable = new Set(retryableGroups(sim));
  const groups: ReportGroup[] = sim.groups.map((_, g) => {
    const files = inGroup(sim, g);
    const first = files[0] as SimFile;
    return {
      photoId: first.photoId,
      name: first.photoName,
      action: first.action,
      kind: first.kind,
      cameraProtected: first.cameraProtected,
      retryable: retryable.has(g),
      files: files.map((f) => ({
        fileName: f.fileName,
        from: f.from,
        to: f.to,
        state: f.state,
        reason: f.reason,
        code: null,
      })),
    };
  });
  return {
    runId: sim.runs.at(-1)?.id ?? "",
    end: sim.end ?? "finished",
    groups,
    held: sim.cfg.held,
    partialsKept: [],
    sourceVolume: sim.cfg.sourceVolume,
    destinationVolume: sim.files.some((f) => f.action === "keep")
      ? sim.cfg.destinationVolume
      : null,
  };
}

export const runIdOf = (sim: Sim): string => sim.runs.at(-1)?.id ?? "";
