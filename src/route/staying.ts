import type { FilesLabel, KeepMode } from "../backend/types";

/** How many files a label names: its extensions are one per file, separated by spaces. */
export const fileCount = (files: FilesLabel): number =>
  files.extensions.split(" ").filter((ext) => ext !== "").length;

/** "stays" for one file, "stay" for several, to follow a files label in a sentence. */
export const stayVerb = (files: FilesLabel): string => (fileCount(files) === 1 ? "stays" : "stay");

/**
 * The camera-protected note for one branch's example, or "" when the read-only attribute did not
 * change the kind: a keep by copy is a copy anyway (the core's `protected`, SPEC §10 plan() (f)).
 * Pass the chosen keep mode for the Kept branch and "move" for the Rejected branch.
 */
export const protectedNote = (cameraProtected: boolean, mode: KeepMode | null): string =>
  cameraProtected && mode !== "copy" ? " (camera-protected: copied, original kept)" : "";
