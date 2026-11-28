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
