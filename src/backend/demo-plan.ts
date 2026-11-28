/**
 * The demo backend's plan and preflight (design/file-engine.md §7, §8), as display strings for the
 * review screen. In the app these come from the core's `plan()` and `preflight()`; the rules here
 * follow them, so the demo shows the plan the app would build:
 *
 * - only decided groups get operations; undecided ones get none (INV-04);
 * - rejects go to `<source>\_rejected\<subfolders>`, keeps to `<destination>\<subfolders>` (§7.2);
 * - a reject is a rename; a keep is a rename on the same volume, a move across drives otherwise,
 *   or a copy; a group with a read-only file among those it moves is copied whole (P13);
 * - the NEF and the sidecars that describe it follow only with "RAW follows JPEG" (ADR 0023);
 * - a taken name gives the whole group the smallest free suffix `_1`, `_2`… up to 9999, against
 *   what exists and what the plan already claimed, case-insensitively (§7.3);
 * - preflight holds a group on its first rule in P8, P9, P10, P14 order, and checks the blockers
 *   (P1 to P5, P16) and warnings (P12, P15, P17) over the groups that are not held (SPEC §10).
 *
 * Not modelled in the demo: the rule that a target folder name taken by a file claims it for the
 * plan (§7.3), and the TargetFolderBlocked hold it leads to.
 *
 * Facts the demo cannot look up (locks, links, changed files, free space) come from its scenarios.
 * Pure: no I/O, no DOM. Nothing here is sent back: the backend re-derives paths from opaque ids.
 */
import type { GroupState } from "../decisions/events";
import { type DemoGroup, filesOf, joinPath, rejectedRoot } from "./demo-routes";
import type {
  Blocker,
  BranchPlan,
  Collision,
  HeldPhoto,
  HoldReason,
  KeepMode,
  OperationKind,
  OtherFile,
  PlannedFile,
  PlanWarning,
  ReviewPlan,
  StayingFile,
  VolumeChoice,
} from "./types";

/**
 * Read rate for the time estimate until B-13 measures the reader (bytes per second). Proposed and
 * provisional (SPEC §10, US-34): 90 MB/s, the order of a UHS-I card.
 */
export const DEFAULT_READ_RATE = 90_000_000;

/** The largest collision suffix tried (§7.3). */
export const MAX_SUFFIX = 9999;

/** P5 margin: max(1 GiB, 2 % of the bytes written) (SPEC §10, preflight (d)). */
export const MIN_MARGIN = 1024 ** 3;

/** P14: the largest file a FAT32 volume holds. */
export const FAT32_MAX_FILE = 4 * 1024 ** 3 - 1;

/** P15: target paths over this many characters get a warning. */
export const LONG_PATH = 259;

/** Case-insensitive key of a display path, as Windows compares names (§3.4, simplified). */
export const folded = (path: string): string => path.toLowerCase();

/** A deterministic size for a demo file: JPEG 8 to 14 MB, NEF 24 to 31 MB, edit files a few KB. */
export function fileBytes(name: string): number {
  let hash = 2166136261;
  for (let i = 0; i < name.length; i += 1) {
    hash = Math.imul(hash ^ name.charCodeAt(i), 16777619) >>> 0;
  }
  const upper = name.toUpperCase();
  if (upper.endsWith(".NEF")) {
    return 24_000_000 + (hash % 7_000_000);
  }
  if (upper.endsWith(".JPG") || upper.endsWith(".JPEG")) {
    return 8_000_000 + (hash % 6_000_000);
  }
  return 4_000 + (hash % 8_000);
}

type FileKind = "jpeg" | "nef" | "edit";

const fileKindOf = (group: DemoGroup, name: string): FileKind => {
  const rest = name.slice(group.name.length).toUpperCase();
  if (rest === ".NEF") return "nef";
  if (rest === ".JPG" || rest === ".JPEG") return "jpeg";
  return "edit";
};

export interface PlanGroup {
  /** Opaque photo (group) id. */
  readonly photoId: string;
  readonly group: DemoGroup;
  readonly state: GroupState;
}

export interface PlanSettings {
  readonly sourcePath: string;
  readonly sourceVolume: string;
  readonly destinationPath: string;
  readonly destinationVolume: string;
  readonly keepMode: KeepMode;
  readonly rawFollows: boolean;
  /** Source and destination on one volume (decided by GUID in the app, §6.3 step 4). */
  readonly sameVolume: boolean;
  /** The destination's file system is FAT32 (P14). */
  readonly destinationFat32: boolean;
}

/** What preflight looks up, read-only, in the app; set by the demo's scenarios. */
export interface DemoFacts {
  readonly sourceMounted: boolean;
  readonly destinationMounted: boolean;
  /** P1: two mounted volumes share the source's serial; the user picks one. */
  readonly sourceChoices: readonly VolumeChoice[] | null;
  readonly sourceReadOnly: boolean;
  readonly destinationReadOnly: boolean;
  readonly sourceFree: number;
  readonly destinationFree: number;
  readonly clusterBytes: number;
  readonly previewsBusy: boolean;
  /** The destination is inside OneDrive or another sync root (P17). */
  readonly syncRoot: boolean;
  /** Group names whose files changed since the scan (P10). */
  readonly changed: ReadonlySet<string>;
  /** Group names with a link or cloud placeholder on the way (P9). */
  readonly links: ReadonlySet<string>;
  /** Group names with a file another app holds (P12). */
  readonly locked: ReadonlySet<string>;
  /** Group names whose targets the demo treats as over 260 characters (P15). */
  readonly longPaths: ReadonlySet<string>;
  /** Group names with a target path too long or a name the destination refuses (P8). */
  readonly tooLong: ReadonlySet<string>;
  /** Group names applied by an earlier plan (INV-04). */
  readonly applied: ReadonlySet<string>;
  /** Group names with a file missing since the scan (§6.2). */
  readonly missing: ReadonlySet<string>;
}

export const NORMAL_FACTS: DemoFacts = {
  sourceMounted: true,
  destinationMounted: true,
  sourceChoices: null,
  sourceReadOnly: false,
  destinationReadOnly: false,
  sourceFree: 412_000_000_000,
  destinationFree: 812_000_000_000,
  clusterBytes: 4096,
  previewsBusy: false,
  syncRoot: false,
  changed: new Set(),
  links: new Set(),
  locked: new Set(),
  longPaths: new Set(),
  tooLong: new Set(),
  applied: new Set(),
  missing: new Set(),
};

/** One planned group: its operations and the files that stay. */
export interface DraftGroup {
  readonly photoId: string;
  readonly name: string;
  /** `MGF_7271_1`, or the name itself with no suffix. */
  readonly newName: string;
  readonly suffix: number;
  readonly sourceFolder: string;
  readonly action: "keep" | "reject";
  readonly root: string;
  readonly targetFolder: string;
  readonly kind: OperationKind;
  readonly cameraProtected: boolean;
  readonly ops: readonly PlannedFile[];
  /** The kind of each file of `ops`, in the same order. */
  readonly fileKinds: readonly FileKind[];
  readonly staying: readonly StayingFile[];
}

export interface Draft {
  readonly groups: readonly DraftGroup[];
  /** Plan holds: applied by an earlier plan, missing files, no free suffix. */
  readonly held: readonly HeldPhoto[];
  readonly undecided: number;
}

const decisionOf = (state: GroupState): "kept" | "rejected" | null =>
  state === "kept" ? "kept" : state === "rejected" ? "rejected" : null;

/** The kind of a group's operations (§7.2), before and after camera protection (P13). */
function kindOf(
  action: "keep" | "reject",
  settings: PlanSettings,
  cameraProtected: boolean,
): OperationKind {
  if (cameraProtected) return "copy";
  if (action === "reject") return "move";
  if (settings.keepMode === "copy") return "copy";
  return settings.sameVolume ? "move" : "moveAcrossDrives";
}

/**
 * How many kept groups would get operations, so need move or copy and a destination: those not
 * held by the plan as applied or missing (SPEC §10, plan() (c)). Holds that depend on the
 * destination (no free suffix) cannot be known without one, so they count, as in the core.
 */
export function keptNeedingRoute(groups: readonly PlanGroup[], facts: DemoFacts): number {
  return groups.filter(
    ({ group, state }) =>
      state === "kept" && !facts.applied.has(group.name) && !facts.missing.has(group.name),
  ).length;
}

/**
 * Plans the decided groups in display order (§7). `existing` holds the folded display paths of the
 * files and folders under the destination and `_rejected`.
 */
export function draftPlan(
  groups: readonly PlanGroup[],
  settings: PlanSettings,
  existing: ReadonlySet<string>,
  facts: DemoFacts,
): Draft {
  const claimed = new Set<string>();
  const planned: DraftGroup[] = [];
  const held: HeldPhoto[] = [];
  let undecided = 0;
  let opId = 0;
  for (const { photoId, group, state } of groups) {
    const decision = decisionOf(state);
    const sourceFolder = joinPath(settings.sourcePath, group.folder);
    if (decision === null) {
      undecided += 1;
      continue;
    }
    const hold = (reason: HoldReason) =>
      held.push({ photoId, name: group.name, folder: sourceFolder, decision, reason });
    if (facts.applied.has(group.name)) {
      hold("applied");
      continue;
    }
    if (facts.missing.has(group.name)) {
      hold("missing");
      continue;
    }
    const action: "keep" | "reject" = decision === "kept" ? "keep" : "reject";
    const files = filesOf(group, settings.rawFollows);
    const root = action === "keep" ? settings.destinationPath : rejectedRoot(settings.sourcePath);
    const targetFolder = joinPath(root, group.folder);
    // Each member keeps its own stem case (`MGF_7001.JPG` beside `mgf_7001.nef` in the app); the
    // suffix goes after the stem (§7.3). The demo's members all spell the stem as the group does.
    const targetsFor = (n: number): string[] => {
      const tail = n === 0 ? "" : `_${n}`;
      return files.going.map((name) => {
        const stem = name.slice(0, group.name.length);
        return joinPath(targetFolder, [stem + tail + name.slice(group.name.length)]);
      });
    };
    let suffix = -1;
    for (let n = 0; n <= MAX_SUFFIX; n += 1) {
      const keys = targetsFor(n).map(folded);
      if (keys.every((k) => !existing.has(k) && !claimed.has(k))) {
        suffix = n;
        break;
      }
    }
    if (suffix < 0) {
      hold("noFreeName");
      continue;
    }
    const targets = targetsFor(suffix);
    for (const t of targets) claimed.add(folded(t));
    const cameraProtected = files.going.some((name) =>
      group.readOnly.includes(name.slice(group.name.length)),
    );
    const kind = kindOf(action, settings, cameraProtected);
    const ops = files.going.map((name, i) => {
      opId += 1;
      return {
        id: `op-${opId}`,
        photoId,
        photoName: group.name,
        action,
        from: joinPath(sourceFolder, [name]),
        to: targets[i] ?? "",
        kind,
        // Only where the read-only attribute changed the kind (SPEC §10, plan() (f)).
        cameraProtected: cameraProtected && kindOf(action, settings, false) !== "copy",
        bytes: fileBytes(name),
      };
    });
    planned.push({
      photoId,
      name: group.name,
      newName: suffix === 0 ? group.name : `${group.name}_${suffix}`,
      suffix,
      sourceFolder,
      action,
      root,
      targetFolder,
      kind,
      cameraProtected: ops.some((op) => op.cameraProtected),
      ops,
      fileKinds: files.going.map((name) => fileKindOf(group, name)),
      staying: files.staying.map((name) => ({
        photoId,
        photoName: group.name,
        path: joinPath(sourceFolder, [name]),
      })),
    });
  }
  return { groups: planned, held, undecided };
}

export interface Preflight {
  readonly runnable: readonly DraftGroup[];
  readonly held: readonly HeldPhoto[];
  readonly blockers: readonly Blocker[];
  readonly warnings: readonly PlanWarning[];
}

const roundUp = (bytes: number, cluster: number): number =>
  cluster <= 0 ? bytes : Math.ceil(bytes / cluster) * cluster;

const margin = (bytes: number): number => Math.max(MIN_MARGIN, Math.ceil(bytes * 0.02));

const writes = (kind: OperationKind): boolean => kind !== "move";

/** Preflight over a draft (§8): holds first, then blockers and warnings over what is not held. */
export function preflight(draft: Draft, settings: PlanSettings, facts: DemoFacts): Preflight {
  const held: HeldPhoto[] = [];
  const runnable: DraftGroup[] = [];
  for (const g of draft.groups) {
    const reason = holdOf(g, settings, facts);
    if (reason === null) {
      runnable.push(g);
    } else {
      const decision = g.action === "keep" ? "kept" : "rejected";
      held.push({ photoId: g.photoId, name: g.name, folder: g.sourceFolder, decision, reason });
    }
  }

  const blockers: Blocker[] = [];
  const anyKeep = draft.groups.some((g) => g.action === "keep");
  if (!facts.sourceMounted) {
    blockers.push({ kind: "volumeMissing", role: "source", volume: settings.sourceVolume });
  } else if (facts.sourceChoices !== null) {
    blockers.push({ kind: "volumeAmbiguous", role: "source", choices: facts.sourceChoices });
  }
  // P2: bound as soon as any operation keeps into it, held or not (SPEC §10, preflight (c)).
  if (anyKeep && !facts.destinationMounted) {
    blockers.push({
      kind: "volumeMissing",
      role: "destination",
      volume: settings.destinationVolume,
    });
  }
  // P3: every rename, every move across drives (its source goes) and every copy into _rejected
  // writes to the source volume.
  const touchesSource = runnable.some(
    (g) => g.kind === "move" || g.kind === "moveAcrossDrives" || g.action === "reject",
  );
  if (facts.sourceReadOnly && touchesSource) {
    blockers.push({ kind: "sourceWriteProtected", volume: settings.sourceVolume });
  }
  const keeps = runnable.filter((g) => g.action === "keep");
  if (facts.destinationReadOnly && keeps.length > 0 && facts.destinationMounted) {
    blockers.push({ kind: "destinationReadOnly", volume: settings.destinationVolume });
  }
  // P5: copies and moves across drives, rounded to clusters, plus the margin; one sum when the
  // source and the destination are one volume.
  const sum = (gs: readonly DraftGroup[]) =>
    gs
      .filter((g) => writes(g.kind))
      .flatMap((g) => g.ops)
      .reduce((n, op) => n + roundUp(op.bytes, facts.clusterBytes), 0);
  const toDestination = sum(keeps);
  const toSource = sum(runnable.filter((g) => g.action === "reject"));
  const space = (volume: string, bytes: number, free: number) => {
    if (bytes === 0) return;
    const needed = bytes + margin(bytes);
    if (needed > free) blockers.push({ kind: "notEnoughSpace", volume, needed, free });
  };
  if (settings.sameVolume) {
    space(settings.sourceVolume, toDestination + toSource, facts.sourceFree);
  } else {
    if (facts.destinationMounted) {
      space(settings.destinationVolume, toDestination, facts.destinationFree);
    }
    space(settings.sourceVolume, toSource, facts.sourceFree);
  }
  if (facts.previewsBusy) {
    blockers.push({ kind: "previewsBusy" });
  }

  const warnings: PlanWarning[] = [];
  if (facts.syncRoot && keeps.length > 0) {
    warnings.push({ kind: "cloudSynced", folder: settings.destinationPath });
  }
  const locked = runnable.filter((g) => facts.locked.has(g.name));
  const firstLocked = locked[0];
  if (firstLocked !== undefined) {
    warnings.push({
      kind: "locked",
      files: locked.reduce((n, g) => n + g.ops.length, 0),
      example: firstLocked.name,
    });
  }
  const long = runnable
    .flatMap((g) => g.ops.map((op) => ({ g, op })))
    .filter(({ g, op }) => facts.longPaths.has(g.name) || op.to.length > LONG_PATH);
  if (long.length > 0) {
    warnings.push({ kind: "longPaths", files: long.length });
  }
  return { runnable, held, blockers, warnings };
}

/** The first hold in rule order P8, P9, P10, P14, or null (SPEC §10, preflight (b)). */
function holdOf(g: DraftGroup, settings: PlanSettings, facts: DemoFacts): HoldReason | null {
  if (facts.tooLong.has(g.name)) return "pathTooLong";
  if (facts.links.has(g.name)) return "linkOrPlaceholder";
  if (facts.changed.has(g.name)) return "changedSinceScan";
  const fat =
    g.action === "keep" && settings.destinationFat32 && g.ops.some((op) => writes(op.kind));
  if (fat && g.ops.some((op) => op.bytes > FAT32_MAX_FILE)) return "tooLargeForFat32";
  return null;
}

function branchOf(
  groups: readonly DraftGroup[],
  action: "keep" | "reject",
  existing: ReadonlySet<string>,
  root: string,
): BranchPlan | null {
  const mine = groups.filter((g) => g.action === action);
  if (mine.length === 0) return null;
  const ops = mine.flatMap((g) => g.ops);
  const kinds = mine.flatMap((g) => g.fileKinds);
  const folders: { folder: string; photos: number; newFolder: boolean }[] = [];
  const byFolder = new Map<string, { folder: string; photos: number; newFolder: boolean }>();
  for (const g of mine) {
    const key = folded(g.targetFolder);
    const entry = byFolder.get(key);
    if (entry === undefined) {
      const created = { folder: g.targetFolder, photos: 1, newFolder: !existing.has(key) };
      byFolder.set(key, created);
      folders.push(created);
    } else {
      entry.photos += 1;
    }
  }
  const copied = mine.filter((g) => g.kind === "copy");
  return {
    photos: mine.length,
    moved: mine.length - copied.length,
    copied: copied.length,
    protectedCopies: copied.filter((g) => g.cameraProtected).length,
    to: root,
    toExists: existing.has(folded(root)),
    jpeg: kinds.filter((k) => k === "jpeg").length,
    nef: kinds.filter((k) => k === "nef").length,
    edit: kinds.filter((k) => k === "edit").length,
    bytes: ops.reduce((n, op) => n + op.bytes, 0),
    folders,
    photoIds: mine.map((g) => g.photoId),
  };
}

function collisionsOf(groups: readonly DraftGroup[]): Collision[] {
  const byRoot = new Map<string, DraftGroup[]>();
  for (const g of groups) {
    if (g.suffix > 0) {
      const list = byRoot.get(g.root) ?? [];
      list.push(g);
      byRoot.set(g.root, list);
    }
  }
  return [...byRoot].map(([where, gs]) => {
    const first = gs[0] as DraftGroup;
    return { where, photos: gs.length, suffix: `_${first.suffix}`, example: first.newName };
  });
}

export interface ReviewInput {
  readonly planId: string;
  readonly uptoSeq: number;
  readonly groups: readonly PlanGroup[];
  readonly settings: PlanSettings;
  readonly existing: ReadonlySet<string>;
  readonly facts: DemoFacts;
  readonly otherFiles: readonly OtherFile[];
  /** The draft this one replaces, to list the names that changed meanwhile (P11). */
  readonly previous?: ReviewPlan;
}

/** Plans, preflights and lays the result out for the review screen. */
export function reviewPlanOf(input: ReviewInput): ReviewPlan {
  const { settings, existing, facts } = input;
  const draft = draftPlan(input.groups, settings, existing, facts);
  const checked = preflight(draft, settings, facts);
  const runnable = checked.runnable;
  const operations = runnable.flatMap((g) => g.ops);
  const bytesToWrite = operations
    .filter((op) => writes(op.kind))
    .reduce((n, op) => n + op.bytes, 0);
  // Each copied byte is read from the source, read back from the copy, then read again from the
  // source with no OS cache (ADR 0021, SPEC §9).
  const bytesToRead = bytesToWrite * 3;
  const kept = branchOf(runnable, "keep", existing, settings.destinationPath);
  const rejected = branchOf(runnable, "reject", existing, rejectedRoot(settings.sourcePath));
  return {
    planId: input.planId,
    uptoSeq: input.uptoSeq,
    counts: {
      kept: kept?.photos ?? 0,
      rejected: rejected?.photos ?? 0,
      undecided: draft.undecided,
      held: draft.held.length + checked.held.length,
    },
    kept,
    rejected,
    operations,
    staying: runnable.flatMap((g) => g.staying),
    held: [...draft.held, ...checked.held],
    blockers: checked.blockers,
    warnings: checked.warnings,
    replanned: input.previous === undefined ? [] : replannedSince(input.previous, runnable),
    collisions: collisionsOf(runnable),
    otherFiles: input.otherFiles,
    estimate: {
      bytesToWrite,
      bytesToRead,
      seconds: Math.ceil(bytesToRead / DEFAULT_READ_RATE),
      measured: false,
      changesDrive: runnable.some((g) => g.kind === "moveAcrossDrives"),
      sourceVolume: settings.sourceVolume,
    },
  };
}

/** Groups whose target name grew a suffix since the previous draft: a name taken meanwhile. */
function replannedSince(previous: ReviewPlan, runnable: readonly DraftGroup[]) {
  const before = new Map<string, string>();
  for (const op of previous.operations) {
    if (!before.has(op.photoId)) before.set(op.photoId, op.to);
  }
  return runnable
    .filter((g) => {
      const was = before.get(g.photoId);
      const now = g.ops[0]?.to;
      return was !== undefined && now !== undefined && folded(was) !== folded(now) && g.suffix > 0;
    })
    .map((g) => ({ name: g.name, newName: g.newName, suffix: `_${g.suffix}`, where: g.root }));
}
