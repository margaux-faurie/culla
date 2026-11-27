import type { DecisionEvent } from "../decisions/events";

/**
 * Everything the UI asks of the outside world goes through this interface (ADR 0014).
 * `backend/tauri.ts` talks to the Rust side; `backend/demo.ts` simulates it in the browser.
 *
 * The UI only ever holds opaque ids. Folder strings in `DisplayFolder` are display data: they are
 * shown, never sent back, and the backend re-derives every real path from the ids
 * (design/file-engine.md §4, threat model T7).
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
  /** `_rejected\` is marked as a new folder only while it does not exist yet (SPEC §2). */
  readonly rejectedFolderExists: boolean;
  /** One real photo of the source, for the route's example paths. Null when there is none. */
  readonly example: PhotoSummary | null;
}

export interface Sidecar {
  /** Everything after the stem, as on disk: `.xmp`, `.NEF.xmp`, `.pp3`. */
  readonly suffix: string;
  /** `raw` stays with the NEF when "RAW follows JPEG" is off (SPEC §3.1, ADR 0023). */
  readonly follows: "primary" | "raw";
}

/** One group, the unit of decision (SPEC §3.1). Display data only, apart from the opaque `id`. */
export interface PhotoSummary {
  /** Opaque group id. */
  readonly id: string;
  /** The group's stem, e.g. `MGF_7273`. */
  readonly name: string;
  /** Subfolders from the source root, e.g. `["Barcelona"]`. */
  readonly folder: readonly string[];
  /** Extension of the primary as on disk: `.JPG`, or `.NEF` for a lone NEF. */
  readonly primary: string;
  /** Extension of the NEF beside a JPEG primary, or null. */
  readonly raw: string | null;
  readonly sidecars: readonly Sidecar[];
  /** Read-only attribute set by the camera: the group is copied, never moved (SPEC §2). */
  readonly cameraProtected: boolean;
  /** `culla://preview/<id>` in the app; a bundled image in the demo. */
  readonly previewUrl: string;
  readonly alt: string;
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
  readonly rejectedFolderExists: boolean;
  /** Groups in display order. */
  readonly photos: readonly PhotoSummary[];
}
