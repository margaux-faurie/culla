import type { DecisionEvent } from "../decisions/events";

/**
 * Everything the UI asks of the outside world goes through this interface (ADR 0014).
 * `backend/tauri.ts` talks to the Rust side; `backend/demo.ts` simulates it in the browser.
 *
 * The UI only ever holds opaque ids. Folder strings in `DisplayFolder`, `SessionInfo` and
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
  /**
   * Starts sorting a folder at once (SPEC §4.2, decided by Margaux 28 Nov 2026). The backend fills
   * in the rest without asking: "RAW follows JPEG" on, move or copy as last chosen, the destination
   * remembered for this folder or else the last one used. Either may be null the first time.
   * A folder with a session in progress resumes it (US-37, proposed in SPEC §10 "Resume"): its
   * stored events come back in `SessionInfo.events`, with "RAW follows JPEG" as it had it.
   */
  startSession(options: StartOptions): Promise<SessionInfo>;
  /**
   * "Start over" on a resumed session (US-37, FE §6.1 step 4): the session is archived with its
   * events, and a new one starts on the same folder with every photo undecided. Refused while the
   * session has a confirmed plan. Nothing on any disk changes.
   */
  startOver(sessionId: string): Promise<SessionInfo>;
  /**
   * Changes how the session applies (review screen): move or copy, the destination, "RAW follows
   * JPEG". Remembers move or copy and the destination for next time. Answers the session with
   * every photo's route recomputed. Nothing on any disk changes.
   */
  updateSession(sessionId: string, change: SessionChange): Promise<SessionInfo>;
  /** Opens the native folder dialog for the destination. Null when the user cancels. */
  chooseDestination(): Promise<DestinationSummary | null>;
  /**
   * The photo's full image for zoom to 100 % (US-39), loaded on demand by its opaque id:
   * `culla://full/<id>` in the app, never a path. Rejects when the file cannot be read (card
   * pulled, damaged file); the preview stays.
   */
  fullImage(photoId: string): Promise<FullImage>;
  /**
   * The EXIF overlay's display strings for a photo (US-40), or `pending` until the EXIF pass has
   * read it; the overlay asks again. Read-only.
   */
  photoExif(photoId: string): Promise<PhotoExif>;
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
  /**
   * Starts the apply of a confirmed plan (design/file-engine.md §9, US-36), or picks up an
   * interrupted one (§10, US-38). Idempotent: a plan already running answers its status.
   */
  startApply(planId: string): Promise<ApplyStatus>;
  /** Where the apply stands now. The apply screen polls it a few times a second. */
  applyStatus(planId: string): Promise<ApplyStatus>;
  /**
   * Asks the run to pause after the current file (§9.5): the executor checks between groups and
   * before each operation. Answers at once, with the request recorded in `state.halt`.
   */
  pauseApply(planId: string): Promise<ApplyStatus>;
  /** Carries on after a pause, from the journal: a new run (a resume, with its own log, §9.8). */
  continueApply(planId: string): Promise<ApplyStatus>;
  /**
   * Stops after the current file (cancel, §9.5): no new operation starts, and a cross-volume group
   * cut short ends "copied, source kept". From a pause it ends the run at once.
   */
  stopApply(planId: string): Promise<ApplyStatus>;
  /** From the report: runs again, in the same plan, the failed groups that never reached committed. */
  retryFailed(planId: string): Promise<ApplyStatus>;
  /**
   * From the report: re-verifies each "copied, source kept" file from scratch, runs the group's
   * failed files again, then removes the sources through the audited removal (§9.6, SPEC §2).
   */
  finishMoves(planId: string): Promise<ApplyStatus>;
  /**
   * Closes the plan once its run has ended (`completed` or `cancelled`, §9.1). After it the session
   * can plan again: "Apply remaining" builds a new plan from the current decisions (§9.6).
   */
  closeApply(planId: string): Promise<void>;
  /**
   * Opens the log of one run of a plan by its id (§9.8, NFR-REL-14). The app opens it in the
   * default text editor and never hands the webview its path (threat model T7); the demo answers
   * the text, rendered as the core renders it.
   */
  openLog(runId: string): Promise<LogOpened>;
  /**
   * An apply found interrupted at launch (crash, power cut, pulled card), with what the screen
   * needs to show it, or null (§10.1, US-38). The plan is already set to `interrupted`.
   */
  interruptedApply(): Promise<InterruptedApply | null>;
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
   * `moved to D:\Selects\Barcelona` or `copied to …`; `moved or copied to …` while move or copy is
   * not chosen; null while no available destination is chosen.
   */
  readonly keptTo: string | null;
  /** `moved to E:\2026\_rejected\Barcelona`, or `copied to …` for a camera-protected photo. */
  readonly rejectedTo: string;
  /**
   * A file that goes with the decision is read-only (camera "protect"), so the group is copied, not
   * moved (SPEC §2). A read-only NEF that stays because RAW does not follow does not count. The
   * swipe screen's routes word it as a copy (`keptTo`, `rejectedTo`).
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

/** A photo's full image, for zoom (US-39). */
export interface FullImage {
  /** `culla://full/<id>` in the app; a bundled image in the demo. */
  readonly url: string;
  /** Its size in pixels. */
  readonly width: number;
  readonly height: number;
  /**
   * The largest JPEG embedded in a NEF (a lone NEF, or an HE/HE* NEF), not RAW pixels (SPEC §9,
   * ADR 0011): the screen says so.
   */
  readonly embeddedPreview: boolean;
}

/**
 * What the EXIF overlay shows (US-40), as display strings the backend writes; null for a field the
 * file does not record. Text from the card (camera, lens) is cleaned by the core first (control and
 * format characters dropped). There is no GPS position and no body serial here, by design.
 */
export type PhotoExif =
  | { readonly kind: "pending" }
  | {
      readonly kind: "ready";
      /** `NIKON Z 6_3` */
      readonly camera: string | null;
      /** `NIKKOR Z 24-70mm f/4 S`, or `24-200mm f/4-6.3` from `LensSpecification`. */
      readonly lens: string | null;
      /** `1/250 s` */
      readonly shutter: string | null;
      /** `f/4` */
      readonly aperture: string | null;
      /** `ISO 400` */
      readonly iso: string | null;
      /** `50 mm` */
      readonly focalLength: string | null;
      /** `−0.3 EV` */
      readonly exposureCompensation: string | null;
      /** `28 Nov 2026, 14:03:22`, in the camera's local time. */
      readonly captured: string | null;
      /** The time is the file's, not the camera's (no DateTimeOriginal or CreateDate, ADR 0028). */
      readonly capturedFromFile: boolean;
    };

export interface DestinationSummary {
  /** Opaque destination id. */
  readonly id: string;
  readonly folder: DisplayFolder;
  /** False when its volume is not mounted: it is shown as not available and cannot be used. */
  readonly available: boolean;
}

export type KeepMode = "move" | "copy";

/** What the start screen sends: the folder, and whether its subfolders are sorted too. */
export interface StartOptions {
  readonly sourceId: string;
  readonly includeSubfolders: boolean;
}

/** The fields of `SessionOptions` the review screen can change; absent fields stay as they are. */
export interface SessionChange {
  readonly keepMode?: KeepMode;
  readonly destinationId?: string;
  readonly rawFollows?: boolean;
}

export interface SessionOptions {
  readonly sourceId: string;
  /** Fixed for the session: it decides which photos are sorted (SPEC §10, proposed). */
  readonly includeSubfolders: boolean;
  readonly rawFollows: boolean;
  /** Null until chosen: the first time there is no last choice. */
  readonly keepMode: KeepMode | null;
  /** Null until chosen: a first run has no destination to propose. */
  readonly destinationId: string | null;
}

export interface SessionInfo {
  /** Opaque session id. */
  readonly id: string;
  readonly source: SourceSummary;
  /** Null until one is chosen; may be not available (its drive unplugged), then it cannot be used. */
  readonly destination: DestinationSummary | null;
  /** The destination is the one remembered, not changed in this session ("same as last time"). */
  readonly sameAsLastTime: boolean;
  readonly options: SessionOptions;
  /** `E:\2026\_rejected`, as Windows shows it. Display only. */
  readonly rejectedFolder: string;
  readonly rejectedFolderExists: boolean;
  /** Groups in display order, each with its route under the session's options. */
  readonly photos: readonly SessionPhoto[];
  /** The session was resumed: it existed before this start (US-37). */
  readonly resumed: boolean;
  /** The stored decision log, in order; empty for a new session. The UI folds it to resume. */
  readonly events: readonly DecisionEvent[];
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

/** A run's request from the user, checked between operations (`journal::run::Halt`). */
export type Halt = "pause" | "cancel";

/** Why a run paused (`plan.pause_reason`, file-engine §9.4). */
export type PauseReason = "user" | "volumeLost" | "writeProtected" | "diskFull";

/** How an apply ended: every operation handled, or stopped by the user (`End`). */
export type ApplyEnd = "finished" | "cancelled";

export type ApplyState =
  /** Running; `halt` is a pause or a stop asked for and not yet reached. */
  | { readonly kind: "running"; readonly halt: Halt | null }
  /**
   * Paused, to continue later. `volume` is the volume a `volumeLost`, `writeProtected` or
   * `diskFull` pause is about (`Photos drive (E:)`, display only); `back` is true once a lost
   * volume is mounted again under its serial, so the apply can continue.
   */
  | {
      readonly kind: "paused";
      readonly reason: PauseReason;
      readonly volume: string | null;
      readonly back: boolean;
    }
  /**
   * Found interrupted at launch (§10.1): it waits for `missing` (a volume label, display only)
   * and touches nothing until it is mounted; null once every volume is present.
   */
  | { readonly kind: "interrupted"; readonly missing: string | null }
  /** The run ended; the report is in `ApplyStatus.report`. The plan is open until `closeApply`. */
  | { readonly kind: "ended"; readonly end: ApplyEnd };

export interface ApplyStatus {
  readonly planId: string;
  /** Opaque id of the current or last run of the plan; each run writes its own log (§9.8). */
  readonly runId: string;
  readonly state: ApplyState;
  readonly progress: ApplyProgress;
  /** Photos of the confirmed plan per branch, for the short route (held photos stay in place). */
  readonly counts: PlanCounts;
  /** The file being worked on, or null between files and while not running. */
  readonly current: CurrentFile | null;
  /** Set once the run has ended. */
  readonly report: ApplyReport | null;
}

export interface ApplyProgress {
  /** Photos (groups) handled, whatever their outcome, of `photos`. */
  readonly photosDone: number;
  readonly photos: number;
  /** Bytes of the files handled, the current copy's progress included, of `bytes`. */
  readonly bytesDone: number;
  readonly bytes: number;
  /** Time left at the read rate, verification reads included (SPEC §9). */
  readonly secondsLeft: number;
  /** False while the rate is the provisional default (SPEC §10), so the time is "about". */
  readonly measured: boolean;
  /** Every planned photo moves, every one is copied, or some of each: the screen's title. */
  readonly mode: "move" | "copy" | "mixed";
}

/** What the engine does with one file (`Kind`): a rename, a verified copy, or a verified move. */
export type ApplyKind = "rename" | "copy" | "xmove";

/** The step of the current file, in the order of file-engine §9.3. */
export type ApplyStep =
  /** A rename on the same volume. */
  | "renaming"
  /** Another app holds the file: retried with backoff, 100 to 1600 ms (§9.4). */
  | "retrying"
  | "copying"
  /** Reading the copy back and the source again, unbuffered, and comparing the hashes. */
  | "verifying"
  /** "Finish moves": checking a copy kept with its source again, from scratch (§9.6). */
  | "reverifying"
  /** Every file of the group checked: the original leaves through the audited removal. */
  | "removingSource";

export interface CurrentFile {
  /** Opaque photo (group) id. */
  readonly photoId: string;
  readonly photoName: string;
  /** The photo's files that this plan moves or copies, by kind; sidecars are edit files. */
  readonly jpeg: number;
  readonly nef: number;
  readonly edit: number;
  /** The file's name, `MGF_7502.NEF`, and its place among the photo's files, from 1. */
  readonly fileName: string;
  readonly index: number;
  readonly count: number;
  readonly kind: ApplyKind;
  readonly step: ApplyStep;
  readonly action: "keep" | "reject";
  /** The folder the file goes to, and the one it comes from, as Windows shows them. Display only. */
  readonly to: string;
  readonly from: string;
  /** The group is copied because the camera protected one of its files (P13). */
  readonly cameraProtected: boolean;
}

/** An operation's journal state (file-engine §9.2), camel-cased. */
export type OpState =
  | "pending"
  | "renaming"
  | "copying"
  | "verifying"
  | "committed"
  | "sourceRemoved"
  | "done"
  | "copiedSourceKept"
  | "failed";

/** Why an operation failed (`FailReason`, §9.4, §10.2). `lost` and `fsInconsistent` are alarms. */
export type FailReason =
  | "locked"
  | "conflict"
  | "planStale"
  | "accessDenied"
  | "verifyMismatch"
  | "shortCopy"
  | "sourceChanged"
  | "sourceMissing"
  | "lost"
  | "fsInconsistent"
  | "groupFailed"
  | "io";

export interface ReportFile {
  /** `MGF_7502.NEF`. */
  readonly fileName: string;
  /** Source and target as Windows shows them, suffix included. Display only. */
  readonly from: string;
  readonly to: string;
  readonly state: OpState;
  /** Set when `state` is `failed`. */
  readonly reason: FailReason | null;
  /** The Win32 code of an `io` failure. */
  readonly code: number | null;
}

/** One photo (group) of the plan as the run left it, in plan order. */
export interface ReportGroup {
  /** Opaque photo (group) id. */
  readonly photoId: string;
  readonly name: string;
  readonly action: "keep" | "reject";
  readonly kind: ApplyKind;
  readonly cameraProtected: boolean;
  /** "Retry failed" can run it: it failed and none of its files ever reached committed (§9.6). */
  readonly retryable: boolean;
  readonly files: readonly ReportFile[];
}

/** The end of a run, for the report (US-36, file-engine §9.6). */
export interface ApplyReport {
  /** Opaque id of the run whose log "Open log" opens. */
  readonly runId: string;
  readonly end: ApplyEnd;
  /** Every planned group, in plan order. */
  readonly groups: readonly ReportGroup[];
  /** Decided photos the plan left out, with the reason (as on review). */
  readonly held: readonly HeldPhoto[];
  /** Partial copies kept because their source vanished (they may be the only copy). Display only. */
  readonly partialsKept: readonly string[];
  /** Display labels of the source and destination volumes, for the chkdsk advice. */
  readonly sourceVolume: string;
  readonly destinationVolume: string | null;
}

export type LogOpened =
  /** The app opened the log in the default text editor. */
  | { readonly kind: "opened" }
  /** The web demo has no editor to open: it answers the text, which the UI shows in a dialog. */
  | { readonly kind: "text"; readonly text: string };

export interface InterruptedApply {
  readonly session: SessionInfo;
  readonly planId: string;
}
