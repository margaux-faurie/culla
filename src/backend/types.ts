import type { DecisionEvent } from "../decisions/events";

/**
 * Everything the UI asks of the outside world goes through this interface (ADR 0014).
 * `backend/tauri.ts` talks to the Rust side; `backend/demo.ts` simulates it in the browser.
 *
 * The UI only ever holds opaque ids. Folder strings in `DisplayFolder`, `SourceDetails` and
 * `PhotoRoute` are display strings computed by the backend: the UI prints them as they are, never
 * builds, parses or sends back a path, and the backend re-derives every real path from the ids
 * (design/file-engine.md §4, threat model T7; the shape is proposed in SPEC §10).
 */
export interface Backend {
  /** Version of the Rust side, or of the demo build. */
  version(): Promise<string>;
  /** Folders culled before, most recent first, with their session status (US-44). */
  recentSources(): Promise<readonly SourceSummary[]>;
  /** Opens the native folder dialog (from Rust in the app). Null when the user cancels. */
  chooseSource(): Promise<SourceSummary | null>;
  /** What the route view needs to know about a source with these options. */
  describeSource(sourceId: string, includeSubfolders: boolean): Promise<SourceDetails>;
  /** One real photo of the source and where it would go with these options. Null when none. */
  exampleRoute(query: RouteQuery): Promise<RouteExample | null>;
  /** The destination remembered for this source, else the last one used anywhere, else null. */
  proposeDestination(sourceId: string): Promise<DestinationProposal | null>;
  /** Opens the native folder dialog for the destination. Null when the user cancels. */
  chooseDestination(): Promise<DestinationSummary | null>;
  /** Starts a session and remembers its destination for the source (SPEC §2, "Keep"). */
  startSession(options: SessionOptions): Promise<SessionInfo>;
  /** Mirrors one decision event, already committed in the UI reducer (autosave, ADR 0005). */
  recordEvent(sessionId: string, event: DecisionEvent): Promise<void>;
  /**
   * Builds a draft plan from the session's recorded decisions up to `uptoSeq` (0 for none) and
   * preflights it (design/file-engine.md §7, §8, US-34). Only the last draft can be confirmed
   * (§7.4). Read-only: nothing on any disk changes.
   */
  reviewPlan(sessionId: string, uptoSeq: number): Promise<ReviewPlan>;
  /**
   * Picks the mounted volume a serial binds to when two share it (P1, P2, §6.3 step 2), then
   * plans again. `volumeId` is one of the blocker's opaque choices.
   */
  chooseVolume(planId: string, volumeId: string): Promise<ReviewPlan>;
  /**
   * Confirms a draft: preflight runs again, then the plan is confirmed with the decisions it was
   * built from and its groups are locked (§7.4, §9.1, US-35). The webview sends the plan id only.
   */
  confirmPlan(planId: string): Promise<ConfirmResult>;
}

export interface DisplayFolder {
  /** The folder as Windows shows it, e.g. `E:\2026`. Display only. */
  readonly path: string;
  /** Volume label and drive letter, e.g. `Photos drive (E:)`. */
  readonly volume: string;
}

export type SourceStatus =
  | { readonly kind: "new" }
  | { readonly kind: "inProgress"; readonly decided: number }
  | { readonly kind: "applied"; readonly on: string };

export interface SourceSummary {
  /** Opaque source id. */
  readonly id: string;
  /** The folder's own name, e.g. `2026`. */
  readonly name: string;
  readonly folder: DisplayFolder;
  readonly photoCount: number;
  readonly status: SourceStatus;
  /** False when its volume is not mounted (SD card out, drive unplugged). */
  readonly connected: boolean;
}

export interface SourceDetails {
  readonly sourceId: string;
  readonly includeSubfolders: boolean;
  readonly photoCount: number;
  /** Names of the subfolders that hold photos, when subfolders are included. */
  readonly subfolders: readonly string[];
  /** `E:\2026\_rejected`, as Windows shows it. Display only. */
  readonly rejectedFolder: string;
  /** `_rejected\` is marked as a new folder only while it does not exist yet (SPEC §2). */
  readonly rejectedFolderExists: boolean;
}

/** The options the start screen shows an example route for. Null fields are not chosen yet. */
export interface RouteQuery {
  readonly sourceId: string;
  readonly includeSubfolders: boolean;
  readonly rawFollows: boolean;
  readonly keepMode: KeepMode | null;
  readonly destinationId: string | null;
}

/** One real photo of the source with its route under the queried options. */
export interface RouteExample {
  readonly photo: PhotoSummary;
  readonly route: PhotoRoute;
}

/** Files in the compact form `MGF_7273 .JPG .NEF .xmp`, split only for emphasis. Display only. */
export interface FilesLabel {
  /** `MGF_7273` */
  readonly name: string;
  /** `.JPG .NEF .xmp` */
  readonly extensions: string;
}

/**
 * Where one photo's files go, as display strings computed by the backend (the demo now, the core's
 * plan in the app later). The UI prints them; it never parses, joins or sends them back.
 */
export interface PhotoRoute {
  /** The folder the photo is in now, e.g. `E:\2026\Barcelona`. */
  readonly folder: string;
  /** The group in that folder, without extensions: `E:\2026\Barcelona\MGF_7273`. */
  readonly origin: string;
  /** The files that go with a decision. */
  readonly going: FilesLabel;
  /** The files that stay in `folder` (the NEF and its sidecars when RAW does not follow). */
  readonly staying: FilesLabel | null;
  /**
   * `moved to D:\Selects\Barcelona` or `copied to …`; the folder alone while moved or copied is not
   * chosen; null while no available destination is chosen.
   */
  readonly keptTo: string | null;
  /** `moved to E:\2026\_rejected\Barcelona`, or `copied to …` for a camera-protected photo. */
  readonly rejectedTo: string;
  /**
   * A file that goes with the decision is read-only (camera "protect"), so the group is copied, not
   * moved (SPEC §2). A read-only NEF that stays because RAW does not follow does not count. The
   * route's Kept example notes it only when moved is chosen or not yet chosen: a keep by copy is a
   * copy anyway, as with the core's `protected`.
   */
  readonly cameraProtected: boolean;
}

/** One group, the unit of decision (SPEC §3.1). Display data only, apart from the opaque `id`. */
export interface PhotoSummary {
  /** Opaque group id. */
  readonly id: string;
  /** The group's display name, e.g. `MGF_7273`. */
  readonly name: string;
  /** What the group holds, for the caption: `JPEG + NEF`, `JPEG` or `NEF`. */
  readonly kind: string;
  /**
   * A file of the group has the read-only attribute set by the camera. Whether that makes the group
   * copied depends on which files go: see `PhotoRoute.cameraProtected` (SPEC §2).
   */
  readonly cameraProtected: boolean;
  /** `culla://preview/<id>` in the app; a bundled image in the demo. */
  readonly previewUrl: string;
  readonly alt: string;
}

/** A photo of a session, with its route under the session's options. */
export interface SessionPhoto extends PhotoSummary {
  readonly route: PhotoRoute;
}

export interface DestinationSummary {
  /** Opaque destination id. */
  readonly id: string;
  readonly folder: DisplayFolder;
  /** False when its volume is not mounted: it is shown as not available and cannot be used. */
  readonly available: boolean;
}

export interface DestinationProposal {
  readonly destination: DestinationSummary;
  /** `thisSource`: remembered for this folder; `lastUsed`: the last one used anywhere. */
  readonly reason: "thisSource" | "lastUsed";
}

export type KeepMode = "move" | "copy";

export interface SessionOptions {
  readonly sourceId: string;
  readonly includeSubfolders: boolean;
  readonly rawFollows: boolean;
  readonly keepMode: KeepMode;
  readonly destinationId: string;
}

export interface SessionInfo {
  /** Opaque session id. */
  readonly id: string;
  readonly source: SourceSummary;
  readonly destination: DestinationSummary;
  readonly options: SessionOptions;
  /** `E:\2026\_rejected`, as Windows shows it. Display only. */
  readonly rejectedFolder: string;
  readonly rejectedFolderExists: boolean;
  /** Groups in display order, each with its route under the session's options. */
  readonly photos: readonly SessionPhoto[];
  /** The photo the review's route view shows as its example, or null when there is none. */
  readonly exampleId: string | null;
}

/**
 * A draft plan and its preflight, for the review screen (US-34). Paths, names and suffixes are
 * display strings computed by the backend (the demo now, the core's `plan` and `preflight` in the
 * app later); the UI prints them, counts and words them, and never builds a path. Held groups are
 * left out of `counts.kept`, `counts.rejected`, the branches and the operations.
 */
export interface ReviewPlan {
  /** Opaque draft id; only the latest one for a session can be confirmed. */
  readonly planId: string;
  /** The last decision event the plan was built from (`decisions_upto_seq`). */
  readonly uptoSeq: number;
  /** Photos (groups): planned kept and rejected, undecided, and decided but held back. */
  readonly counts: PlanCounts;
  /** Null when no kept photo gets an operation. */
  readonly kept: BranchPlan | null;
  /** Null when no rejected photo gets an operation. */
  readonly rejected: BranchPlan | null;
  /** Every operation of the plan, in plan order. */
  readonly operations: readonly PlannedFile[];
  /** Files of planned photos that stay where they are (RAW does not follow, ADR 0023). */
  readonly staying: readonly StayingFile[];
  /** Decided photos that get no operation, with the reason (plan holds, P8, P9, P10, P14). */
  readonly held: readonly HeldPhoto[];
  /** Preflight blockers (P1 to P7, P16): Apply stays disabled while there is one. */
  readonly blockers: readonly Blocker[];
  /** Preflight warnings (P12, P15, P17): apply is still allowed. */
  readonly warnings: readonly PlanWarning[];
  /** Names that changed since the last draft because a target was taken meanwhile (P11). */
  readonly replanned: readonly Replanned[];
  /** Taken names, per target root: those groups get a suffix (§7.3, US-15). */
  readonly collisions: readonly Collision[];
  /** Files Culla does not handle, left alone and not in the plan (US-04, INV-20). */
  readonly otherFiles: readonly OtherFile[];
  readonly estimate: Estimate;
}

export interface PlanCounts {
  readonly kept: number;
  readonly rejected: number;
  readonly undecided: number;
  readonly held: number;
}

/** One branch of the route (Kept or Rejected) as planned. */
export interface BranchPlan {
  readonly photos: number;
  /** Photos moved (same drive or across drives) and photos copied. */
  readonly moved: number;
  readonly copied: number;
  /** Of the copied photos, those copied only because the camera protected a file (P13). */
  readonly protectedCopies: number;
  /** The branch's root: the destination, or `E:\2026\_rejected`. Display only. */
  readonly to: string;
  readonly toExists: boolean;
  /** The branch's files by kind; sidecars are "edit files". */
  readonly jpeg: number;
  readonly nef: number;
  readonly edit: number;
  readonly bytes: number;
  /** Every target folder with photos, in plan order. Display only. */
  readonly folders: readonly PlanFolder[];
  /** The planned photos of the branch, in plan order. */
  readonly photoIds: readonly string[];
}

export interface PlanFolder {
  /** `D:\Photos\Selects 2026\Barcelona`. Display only. */
  readonly folder: string;
  readonly photos: number;
  /** Does not exist yet: the apply creates it. */
  readonly newFolder: boolean;
}

/** `move` is a rename on one volume, `moveAcrossDrives` a verified copy then removal (§7.2). */
export type OperationKind = "move" | "moveAcrossDrives" | "copy";

export interface PlannedFile {
  /** Opaque operation id. */
  readonly id: string;
  /** Opaque photo (group) id. */
  readonly photoId: string;
  readonly photoName: string;
  readonly action: "keep" | "reject";
  /** Source and target as Windows shows them, suffix included. Display only. */
  readonly from: string;
  readonly to: string;
  readonly kind: OperationKind;
  /** The group is copied because the camera protected one of its files (P13). */
  readonly cameraProtected: boolean;
  readonly bytes: number;
}

export interface StayingFile {
  readonly photoId: string;
  readonly photoName: string;
  /** The file as Windows shows it. Display only. */
  readonly path: string;
}

export type HoldReason =
  /** P10: size or modification time differs from the scan. */
  | "changedSinceScan"
  /** A file of the group is no longer there (§6.2). */
  | "missing"
  /** P9: a link, a junction or a cloud placeholder on the way. */
  | "linkOrPlaceholder"
  /** P8: a target or partial path too long, or a name the destination refuses. */
  | "pathTooLong"
  /** P14: a file over 4 GiB going to a FAT32 drive. */
  | "tooLargeForFat32"
  /** Applied by an earlier plan (INV-04). */
  | "applied"
  /** No free suffix up to 9999 (§7.3). */
  | "noFreeName";

export interface HeldPhoto {
  readonly photoId: string;
  readonly name: string;
  /** The folder the photo is in. Display only. */
  readonly folder: string;
  readonly decision: "kept" | "rejected";
  readonly reason: HoldReason;
}

/** A preflight blocker (design/file-engine.md §8), with the display strings its message needs. */
export type Blocker =
  /** P1 or P2: the volume is not mounted. */
  | {
      readonly kind: "volumeMissing";
      readonly role: "source" | "destination";
      readonly volume: string;
    }
  /** P1 or P2: two mounted volumes share the serial; the user picks one (§6.3). */
  | {
      readonly kind: "volumeAmbiguous";
      readonly role: "source" | "destination";
      readonly choices: readonly VolumeChoice[];
    }
  /** P3: the source volume is read-only (an SD card's lock switch). */
  | { readonly kind: "sourceWriteProtected"; readonly volume: string }
  /** P4. */
  | { readonly kind: "destinationReadOnly"; readonly volume: string }
  /** P5: `needed` includes the margin. */
  | {
      readonly kind: "notEnoughSpace";
      readonly volume: string;
      readonly needed: number;
      readonly free: number;
    }
  /** P6. */
  | {
      readonly kind: "destinationRefused";
      readonly reason: "isSource" | "insideRejected" | "placeholder" | "volumeType";
    }
  /** P7: a bug in the plan; never expected. */
  | { readonly kind: "planBug" }
  /** P16: the preview pipeline still holds files; the apply waits for it. */
  | { readonly kind: "previewsBusy" };

export interface VolumeChoice {
  /** Opaque volume id. */
  readonly id: string;
  /** `Photos drive (E:)`. Display only. */
  readonly label: string;
}

export type PlanWarning =
  /** P17: the destination is inside OneDrive or another cloud-synced folder. */
  | { readonly kind: "cloudSynced"; readonly folder: string }
  /** P12: files another app holds open; the apply retries them. */
  | { readonly kind: "locked"; readonly files: number; readonly example: string }
  /** P15: target paths over 260 characters. */
  | { readonly kind: "longPaths"; readonly files: number }
  /** The sync-root lookup did not run (preflight fails closed). */
  | { readonly kind: "syncRootUnknown" };

export interface Replanned {
  /** `MGF_8040` */
  readonly name: string;
  /** `MGF_8040_1` */
  readonly newName: string;
  /** `_1` */
  readonly suffix: string;
  /** The branch root the name was taken in. Display only. */
  readonly where: string;
}

export interface Collision {
  /** The branch root the names are taken in. Display only. */
  readonly where: string;
  readonly photos: number;
  /** `_1`, the first group's suffix. */
  readonly suffix: string;
  /** `MGF_7101_1`, the first group's new name. */
  readonly example: string;
}

export interface OtherFile {
  /** The file as Windows shows it. Display only. */
  readonly path: string;
  /** `.MOV`, `.txt`, as on disk; empty for a name with no extension. */
  readonly extension: string;
  readonly reason: "unknownType" | "orphanSidecar" | "invalidName" | "foreignPartial";
}

/**
 * Bytes to write (copies and moves across drives) and a time estimate that counts the second
 * read of each source (SPEC §9, NFR-PERF-12). `measured` is false while the rate is the
 * provisional default (90 MB/s, SPEC §10) rather than one B-13 measured.
 */
export interface Estimate {
  readonly bytesToWrite: number;
  readonly bytesToRead: number;
  readonly seconds: number;
  readonly measured: boolean;
  /** Some kept photos are moved to another drive: each copy is read back before its original goes. */
  readonly changesDrive: boolean;
  /** `Photos drive (E:)`, the source volume. Display only. */
  readonly sourceVolume: string;
}

export type ConfirmResult =
  /** Confirmed and locked; the apply screen takes over. The demo never touches the disk. */
  | { readonly kind: "confirmed"; readonly planId: string }
  /** Preflight at confirm found a blocker: nothing starts, the review shows it. */
  | { readonly kind: "blocked"; readonly plan: ReviewPlan }
  /** Preflight at confirm found a new hold or a taken name: the review shows the rebuilt plan. */
  | { readonly kind: "replanned"; readonly plan: ReviewPlan };
