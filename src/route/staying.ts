import type { FilesLabel } from "../backend/types";

/** How many files a label names: its extensions are one per file, separated by spaces. */
export const fileCount = (files: FilesLabel): number =>
  files.extensions.split(" ").filter((ext) => ext !== "").length;

/** "stays" for one file, "stay" for several, to follow a files label in a sentence. */
export const stayVerb = (files: FilesLabel): string => (fileCount(files) === 1 ? "stays" : "stay");
