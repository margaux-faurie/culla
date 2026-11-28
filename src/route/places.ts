import type { DestinationSummary, KeepMode } from "../backend/types";

/** The Kept branch's place on the apply screen: no verb until moved or copied is chosen. */
export function keptPlace(
  destination: DestinationSummary | null,
  keepMode: KeepMode | null,
): string {
  const where =
    destination === null || !destination.available
      ? "the destination you choose"
      : destination.folder.path;
  if (keepMode === null) {
    return `To ${where}`;
  }
  return `${keepMode === "copy" ? "Copied" : "Moved"} to ${where}`;
}

/** Under "n left in place": undecided photos, and held-back ones when the plan held some. */
export const leftInPlaceNote = (held: number): string =>
  held > 0
    ? "Undecided and held-back photos are not touched."
    : "Undecided photos are not touched.";
