/**
 * Where photos go (SPEC §2 "Where photos go"): example paths for the route view on the start,
 * swipe and review screens. Pure string work on display data from the backend. Nothing here is
 * ever sent back: the backend re-derives every real path from opaque ids (design/file-engine.md §4).
 */
import type { KeepMode, PhotoSummary } from "../backend/types";

/** Joins a display folder (`E:\2026`) and relative segments with `\`, as Windows shows them. */
export function joinPath(root: string, segments: readonly string[]): string {
  const base = root.endsWith("\\") ? root.slice(0, -1) : root;
  return [base, ...segments].join("\\");
}

export interface Files {
  /** Full file names that go with the decision, e.g. `MGF_7273.JPG`. */
  readonly going: readonly string[];
  /** Full file names that stay at the origin (the NEF and its sidecars when RAW does not follow). */
  readonly staying: readonly string[];
}

/**
 * Which of a photo's files a decision moves or copies: the primary and its own sidecars always;
 * the NEF and the sidecars that describe it only when "RAW follows JPEG" is on (ADR 0023).
 */
export function filesOf(photo: PhotoSummary, rawFollows: boolean): Files {
  const going: string[] = [`${photo.name}${photo.primary}`];
  const staying: string[] = [];
  if (photo.raw !== null) {
    (rawFollows ? going : staying).push(`${photo.name}${photo.raw}`);
  }
  for (const sidecar of photo.sidecars) {
    const follows = sidecar.follows === "primary" || photo.raw === null || rawFollows;
    (follows ? going : staying).push(`${photo.name}${sidecar.suffix}`);
  }
  return { going, staying };
}

/** The extensions of a file list, for the compact form `MGF_7273 .JPG .NEF .xmp`. */
export function suffixes(photo: PhotoSummary, names: readonly string[]): string {
  return names.map((name) => name.slice(photo.name.length)).join(" ");
}

export const photoFolder = (sourcePath: string, photo: PhotoSummary): string =>
  joinPath(sourcePath, photo.folder);

export const keptFolder = (destinationPath: string, photo: PhotoSummary): string =>
  joinPath(destinationPath, photo.folder);

/** `<source>\_rejected\…`, mirroring the photo's subfolders. */
export const rejectedFolder = (sourcePath: string, photo: PhotoSummary): string =>
  joinPath(sourcePath, ["_rejected", ...photo.folder]);

export type Verb = "moved" | "copied";

/** A camera-protected group is copied, never moved, for a keep and a reject alike (SPEC §2). */
export const keepVerb = (mode: KeepMode, photo: PhotoSummary): Verb =>
  mode === "copy" || photo.cameraProtected ? "copied" : "moved";

export const rejectVerb = (photo: PhotoSummary): Verb =>
  photo.cameraProtected ? "copied" : "moved";
