/**
 * The review screen's words and state from a `ReviewPlan` (US-34, US-35). Pure, no DOM, so Vitest
 * covers it. Paths and names come from the backend as display strings and are printed as they are:
 * this module counts and words, it never builds a path.
 *
 * Copy follows the design canvas (Review board) where it has it; any other wording is proposed by
 * the agent (stories.md M4, SPEC §10).
 */
import type {
  Blocker,
  BranchPlan,
  Collision,
  DestinationSummary,
  HoldReason,
  KeepMode,
  OperationKind,
  OtherFile,
  PlanWarning,
  Replanned,
  ReviewPlan,
} from "../backend/types";

/** `1 photo`, `2 photos`. */
export const count = (n: number, one: string, many = `${one}s`): string =>
  `${n.toLocaleString("en-GB")} ${n === 1 ? one : many}`;

/** Bytes in decimal units, one decimal from MB up: `2.1 GB`, `840 KB`. */
export function formatBytes(bytes: number): string {
  if (bytes < 1000) return `${bytes} B`;
  if (bytes < 1_000_000) return `${Math.round(bytes / 1000)} KB`;
  if (bytes < 1_000_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
  if (bytes < 1_000_000_000_000) return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
  return `${(bytes / 1_000_000_000_000).toFixed(1)} TB`;
}

/** Exact to the byte, for the estimate (US-34): `2,104,553,210 bytes`. */
export const exactBytes = (bytes: number): string => count(bytes, "byte");

/** Photos that get an operation. */
export const plannedPhotos = (plan: ReviewPlan): number => plan.counts.kept + plan.counts.rejected;

/** Every planned photo moves (on one drive or to another), none is copied. */
export const allMove = (plan: ReviewPlan): boolean =>
  (plan.kept?.copied ?? 0) + (plan.rejected?.copied ?? 0) === 0;

/** Apply is possible: no blocker, and some photo gets an operation (US-34, US-35). */
export const canApply = (plan: ReviewPlan): boolean =>
  plan.blockers.length === 0 && plannedPhotos(plan) > 0;

/**
 * The primary button and the confirm button: "Move 138 photos" when every planned photo moves (the
 * canvas), "Apply to 138 photos" when some are copied (proposed, US-35).
 */
export function primaryLabel(plan: ReviewPlan): string {
  const photos = count(plannedPhotos(plan), "photo");
  return allMove(plan) ? `Move ${photos}` : `Apply to ${photos}`;
}

/** Why Apply is disabled, or null when it is not. */
export function disabledReason(plan: ReviewPlan): string | null {
  if (plan.blockers.length > 0) {
    return plan.blockers.length === 1
      ? "Apply waits until the problem above is solved."
      : "Apply waits until the problems above are solved.";
  }
  if (plannedPhotos(plan) === 0) {
    return "Nothing to apply yet. Keep or reject some photos first.";
  }
  return null;
}

/**
 * Why Apply waits for the kept photos' route, once the backend said the plan needs one (a
 * `NoKeepRouteError`: kept photos that would get operations, SPEC §10 plan() (c)). Move or copy and
 * a connected destination are set on the review screen, the first time by hand (decided by Margaux
 * 28 Nov 2026). Wording proposed by the agent (SPEC §10, "Simple start").
 */
export function keepRouteHint(
  keepMode: KeepMode | null,
  destination: DestinationSummary | null,
): string {
  const noDestination = destination === null;
  const unplugged = destination !== null && !destination.available;
  if (keepMode === null) {
    if (noDestination) return "Choose move or copy and a destination for the kept photos first.";
    if (unplugged) {
      return "Choose move or copy, and a destination that is connected, for the kept photos first.";
    }
    return "Choose move or copy for the kept photos first.";
  }
  if (noDestination) return "Choose a destination for the kept photos first.";
  if (unplugged) {
    return "The destination's drive is not connected. Connect it, or choose another destination.";
  }
  // Both look set, yet the backend asked for them (a change it has not answered yet).
  return "Choose move or copy and a destination for the kept photos first.";
}

/** `96 kept · moved`, `1 kept · copied`, `40 kept · 39 moved, 1 copied`. */
export function branchTitle(branch: BranchPlan, what: "kept" | "rejected"): string {
  const head = `${branch.photos.toLocaleString("en-GB")} ${what}`;
  if (branch.copied === 0) return `${head} · moved`;
  if (branch.moved === 0) return `${head} · copied`;
  return `${head} · ${branch.moved.toLocaleString("en-GB")} moved, ${branch.copied.toLocaleString("en-GB")} copied`;
}

/** `96 JPEG · 96 NEF · 12 edit files · 2.1 GB` (the canvas), parts with no file left out. */
export function branchFiles(branch: BranchPlan): string {
  const parts: string[] = [];
  if (branch.jpeg > 0) parts.push(`${branch.jpeg.toLocaleString("en-GB")} JPEG`);
  if (branch.nef > 0) parts.push(`${branch.nef.toLocaleString("en-GB")} NEF`);
  if (branch.edit > 0) parts.push(count(branch.edit, "edit file"));
  parts.push(formatBytes(branch.bytes));
  return parts.join(" · ");
}

/** `58 photos · new folder`. */
export const folderNote = (photos: number, newFolder: boolean): string =>
  newFolder ? `${count(photos, "photo")} · new folder` : count(photos, "photo");

export function blockerText(blocker: Blocker): string {
  switch (blocker.kind) {
    case "volumeMissing":
      return blocker.role === "source"
        ? `${blocker.volume} is not connected. Connect it, then check again.`
        : `The destination drive, ${blocker.volume}, is not connected. Connect it, then check again.`;
    case "volumeAmbiguous":
      return blocker.role === "source"
        ? "Two connected drives look like the photos drive. Choose the one that holds your photos."
        : "Two connected drives look like the destination. Choose the right one.";
    case "sourceWriteProtected":
      return `${blocker.volume} is write-protected. If it is a memory card, slide the lock switch on its side up, away from Lock, then check again.`;
    case "destinationReadOnly":
      return `The destination drive, ${blocker.volume}, is read-only. Choose another destination or make it writable.`;
    case "notEnoughSpace":
      return `Not enough space on ${blocker.volume}: ${formatBytes(blocker.needed)} needed with a safety margin, ${formatBytes(blocker.free)} free. Free ${formatBytes(blocker.needed - blocker.free)}, then check again.`;
    case "destinationRefused":
      switch (blocker.reason) {
        case "isSource":
          return "The destination is the folder you sort. Choose another destination.";
        case "insideRejected":
          return "The destination is inside a _rejected folder. Choose another destination.";
        case "placeholder":
          return "The destination is a cloud placeholder that is not on this PC. Choose another destination.";
        case "volumeType":
          return "Culla cannot use this kind of drive as a destination (network or unlabelled). Choose another destination.";
      }
      return "Choose another destination.";
    case "planBug":
      return "Culla found a problem in its own plan, so nothing will run. Go back to sorting and open the review again.";
    case "previewsBusy":
      return "Culla is still reading previews from these photos. Apply waits until it has finished.";
  }
}

export function holdText(reason: HoldReason): string {
  switch (reason) {
    case "changedSinceScan":
      return "changed since scan, rescan";
    case "missing":
      return "a file is missing since the scan";
    case "linkOrPlaceholder":
      return "a link or a cloud placeholder is on its way, so Culla does not follow it";
    case "pathTooLong":
      return "a path or a name is too long for the destination";
    case "tooLargeForFat32":
      return "a file is over 4 GB and the destination drive is FAT32";
    case "applied":
      return "already applied";
    case "noFreeName":
      return "no free name up to _9999";
  }
}

/** The held-back summary line: `2 photos are held back. They stay where they are.` */
export function heldSummary(held: number): string {
  return held === 1
    ? "1 photo is held back. It stays where it is, and the rest can be applied."
    : `${count(held, "photo")} are held back. They stay where they are, and the rest can be applied.`;
}

export function warningText(warning: PlanWarning): string {
  switch (warning.kind) {
    case "cloudSynced":
      return `${warning.folder} is inside a cloud-synced folder. The sync app may upload, change or move the photos during and after the apply.`;
    case "locked":
      return warning.files === 1
        ? `1 file is open in another app (${warning.example}). Culla retries it, then skips it if it stays open.`
        : `${count(warning.files, "file")} are open in another app (${warning.example} and others). Culla retries them, then skips any that stay open.`;
    case "longPaths":
      return warning.files === 1
        ? "1 file will have a path over 260 characters. Explorer and some apps may fail to open it."
        : `${count(warning.files, "file")} will have paths over 260 characters. Explorer and some apps may fail to open them.`;
    case "syncRootUnknown":
      return "Culla could not check whether the destination is in a cloud-synced folder.";
  }
}

/** "2 names are taken in Selects 2026. Those photos get _1 (MGF_7101_1). Nothing is replaced." */
export function collisionText(c: Collision): string {
  return c.photos === 1
    ? `1 name is taken in ${c.where}. That photo gets ${c.suffix} (${c.example}). Nothing is replaced.`
    : `${count(c.photos, "name")} are taken in ${c.where}. Those photos get ${c.suffix} (${c.example}). Nothing is replaced.`;
}

export function replannedText(r: Replanned): string {
  return `${r.name} was taken in ${r.where} since the plan was made. It now gets ${r.suffix} (${r.newName}). Nothing is replaced.`;
}

/** "1 kept photo is protected on the camera. It is copied, and the original stays on the photos drive." */
export function protectedText(
  branch: BranchPlan,
  what: "kept" | "rejected",
  sourceVolume: string,
): string | null {
  const n = branch.protectedCopies;
  if (n === 0) return null;
  return n === 1
    ? `1 ${what} photo is protected on the camera. It is copied, and the original stays on ${sourceVolume}.`
    : `${n.toLocaleString("en-GB")} ${what} photos are protected on the camera. They are copied, and the originals stay on ${sourceVolume}.`;
}

/** "3 other files (.mov, .txt) are left alone." */
export function otherFilesText(files: readonly OtherFile[]): string | null {
  if (files.length === 0) return null;
  const kinds = [
    ...new Set(files.map((f) => f.extension.toLowerCase()).filter((e) => e !== "")),
  ].join(", ");
  const what = kinds === "" ? "" : ` (${kinds})`;
  return files.length === 1
    ? `1 other file${what} is left alone.`
    : `${count(files.length, "other file")}${what} are left alone.`;
}

/** `about 4 minutes`, `under a minute`, `a few seconds`. */
export function duration(seconds: number): string {
  if (seconds <= 0) return "a few seconds";
  if (seconds < 60) return "under a minute";
  const minutes = Math.round(seconds / 60);
  if (minutes < 90) return `about ${count(minutes, "minute")}`;
  const hours = Math.round((minutes / 60) * 2) / 2;
  return `about ${hours} hours`;
}

const capital = (s: string): string => s.charAt(0).toUpperCase() + s.slice(1);

/**
 * The estimate line (US-34): why it takes the time it does, then the time. The rate is the
 * provisional default until B-13 measures it (SPEC §10), so the time is always "about".
 */
export function estimateText(plan: ReviewPlan): string {
  const e = plan.estimate;
  if (e.bytesToWrite === 0) {
    return "Every photo stays on its drive, so this takes a few seconds.";
  }
  const time = capital(duration(e.seconds));
  if (e.changesDrive) {
    return `Kept photos change drive, so each copy is read back and checked before its original leaves ${e.sourceVolume}. ${time}.`;
  }
  return `Each copy is read back and checked against its original. ${time}.`;
}

/** `3.0 GB to write (3,021,554,112 bytes)`, or null when nothing is written. */
export function bytesText(plan: ReviewPlan): string | null {
  const bytes = plan.estimate.bytesToWrite;
  return bytes === 0 ? null : `${formatBytes(bytes)} to write (${exactBytes(bytes)})`;
}

/** Every byte of the plan's files, moved or copied, for the confirm dialog. */
export const planBytes = (plan: ReviewPlan): number =>
  plan.operations.reduce((n, op) => n + op.bytes, 0);

export function kindText(kind: OperationKind, cameraProtected: boolean): string {
  if (cameraProtected) return "Copied (camera-protected, original kept)";
  switch (kind) {
    case "move":
      return "Moved";
    case "moveAcrossDrives":
      return "Moved to another drive, checked first";
    case "copy":
      return "Copied";
  }
}

/** One branch's sentence in the confirm dialog. */
function branchSentence(branch: BranchPlan, what: "kept" | "rejected"): string {
  const photos = `${branch.photos.toLocaleString("en-GB")} ${what} ${branch.photos === 1 ? "photo" : "photos"}`;
  if (branch.copied === 0) return `${photos} will be moved to ${branch.to}.`;
  if (branch.moved === 0) return `${photos} will be copied to ${branch.to}.`;
  return `${photos} go to ${branch.to}: ${branch.moved.toLocaleString("en-GB")} moved, ${branch.copied.toLocaleString("en-GB")} copied.`;
}

export interface ConfirmText {
  readonly title: string;
  readonly body: readonly string[];
  readonly confirm: string;
  readonly cancel: string;
}

/**
 * The confirm dialog (US-35, proposed wording): "Move 138 photos?", then where each branch goes,
 * "Nothing is deleted.", the undecided and held-back photos, and the files, bytes and time.
 */
export function confirmText(plan: ReviewPlan): ConfirmText {
  const label = primaryLabel(plan);
  const body: string[] = [];
  if (plan.kept !== null) body.push(branchSentence(plan.kept, "kept"));
  if (plan.rejected !== null) body.push(branchSentence(plan.rejected, "rejected"));
  body.push("Nothing is deleted.");
  const { undecided, held } = plan.counts;
  if (undecided > 0) {
    body.push(
      undecided === 1
        ? "1 undecided photo stays where it is."
        : `${count(undecided, "undecided photo")} stay where they are.`,
    );
  }
  if (held > 0) {
    body.push(
      held === 1
        ? "1 held-back photo is left out."
        : `${count(held, "held-back photo")} are left out.`,
    );
  }
  body.push(
    `${count(plan.operations.length, "file")}, ${formatBytes(planBytes(plan))}, ${duration(plan.estimate.seconds)}.`,
  );
  return { title: `${label}?`, body, confirm: label, cancel: "Go back" };
}
