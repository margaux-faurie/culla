/**
 * Where photos go (SPEC §2 "Where photos go"), as the demo backend computes it: the display strings
 * of `PhotoRoute`. The UI only prints them. In the app these come from the core's plan; the rules
 * here follow it (ADR 0023 for RAW follows JPEG, SPEC §2 for camera-protected groups). Nothing
 * here is ever sent back: the backend re-derives every real path from opaque ids
 * (design/file-engine.md §4).
 */
import type { FilesLabel, KeepMode, PhotoRoute } from "./types";

export interface Sidecar {
  /** Everything after the stem, as on disk: `.xmp`, `.NEF.xmp`, `.pp3`. */
  readonly suffix: string;
  /** `raw` stays with the NEF when "RAW follows JPEG" is off (SPEC §3.1, ADR 0023). */
  readonly follows: "primary" | "raw";
}

/** A group as the demo backend knows it: the file facts the UI never sees. */
export interface DemoGroup {
  /** The group's stem, e.g. `MGF_7273`. */
  readonly name: string;
  /** Subfolders from the source root, e.g. `["Barcelona"]`. */
  readonly folder: readonly string[];
  /** Extension of the primary as on disk: `.JPG`, or `.NEF` for a lone NEF. */
  readonly primary: string;
  /** Extension of the NEF beside a JPEG primary, or null. */
  readonly raw: string | null;
  readonly sidecars: readonly Sidecar[];
  readonly cameraProtected: boolean;
}

/** Joins a display folder (`E:\2026`) and relative segments with `\`, as Windows shows them. */
export function joinPath(root: string, segments: readonly string[]): string {
  const base = root.endsWith("\\") ? root.slice(0, -1) : root;
  return [base, ...segments].join("\\");
}

export interface Files {
  /** Full file names that go with the decision, e.g. `MGF_7273.JPG`. */
  readonly going: readonly string[];
  /** Full file names that stay where they are (the NEF and its sidecars when RAW does not follow). */
  readonly staying: readonly string[];
}

/**
 * Which of a group's files a decision moves or copies: the primary and its own sidecars always;
 * the NEF and the sidecars that describe it only when "RAW follows JPEG" is on (ADR 0023).
 */
export function filesOf(group: DemoGroup, rawFollows: boolean): Files {
  const going: string[] = [`${group.name}${group.primary}`];
  const staying: string[] = [];
  if (group.raw !== null) {
    (rawFollows ? going : staying).push(`${group.name}${group.raw}`);
  }
  for (const sidecar of group.sidecars) {
    const follows = sidecar.follows === "primary" || group.raw === null || rawFollows;
    (follows ? going : staying).push(`${group.name}${sidecar.suffix}`);
  }
  return { going, staying };
}

/** The compact form `MGF_7273` `.JPG .NEF .xmp` of a file list. */
export function label(group: DemoGroup, names: readonly string[]): FilesLabel {
  return {
    name: group.name,
    extensions: names.map((name) => name.slice(group.name.length)).join(" "),
  };
}

/** What a group holds, for the card's caption. */
export const kindOf = (group: DemoGroup): string =>
  group.raw !== null ? "JPEG + NEF" : group.primary.toUpperCase() === ".NEF" ? "NEF" : "JPEG";

export const rejectedRoot = (sourcePath: string): string => joinPath(sourcePath, ["_rejected"]);

export type Verb = "moved" | "copied";

/** A camera-protected group is copied, never moved, for a keep and a reject alike (SPEC §2). */
export const keepVerb = (mode: KeepMode, group: DemoGroup): Verb =>
  mode === "copy" || group.cameraProtected ? "copied" : "moved";

export const rejectVerb = (group: DemoGroup): Verb => (group.cameraProtected ? "copied" : "moved");

export interface RouteOptions {
  readonly sourcePath: string;
  readonly rawFollows: boolean;
  readonly keepMode: KeepMode | null;
  /** The chosen destination's folder, or null while none is chosen or it is not available. */
  readonly destinationPath: string | null;
}

/** Where a kept group goes; the folder alone (no verb) until moved or copied is chosen. */
function keptTo(group: DemoGroup, options: RouteOptions): string | null {
  if (options.destinationPath === null) {
    return null;
  }
  const folder = joinPath(options.destinationPath, group.folder);
  if (group.cameraProtected) {
    return `copied to ${folder}`;
  }
  return options.keepMode === null ? folder : `${keepVerb(options.keepMode, group)} to ${folder}`;
}

/** The display strings of one group's route under these options. */
export function routeOf(group: DemoGroup, options: RouteOptions): PhotoRoute {
  const files = filesOf(group, options.rawFollows);
  return {
    folder: joinPath(options.sourcePath, group.folder),
    origin: joinPath(options.sourcePath, [...group.folder, group.name]),
    going: label(group, files.going),
    staying: files.staying.length === 0 ? null : label(group, files.staying),
    keptTo: keptTo(group, options),
    rejectedTo: `${rejectVerb(group)} to ${joinPath(rejectedRoot(options.sourcePath), group.folder)}`,
  };
}
