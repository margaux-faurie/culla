/**
 * The swipe screen's words for where a photo goes (SPEC §4.2, US-30). The paths are the backend's
 * display strings (`PhotoRoute`), printed as they are; this module only picks the sentence around
 * them. Before a destination is chosen (the first time) there is no kept route yet, so the words
 * say it is chosen on the review screen (decided by Margaux 28 Nov 2026). Pure, no DOM.
 */
import type { PhotoRoute } from "../backend/types";

/** Shown in place of a kept route while no destination is chosen. */
export const KEPT_CHOSEN_ON_REVIEW = "You choose where on Review";

/** The line under the Keep button: `moved to D:\Photos\Selects 2026`. */
export const keepLine = (route: PhotoRoute): string => route.keptTo ?? KEPT_CHOSEN_ON_REVIEW;

/** The note on a decided photo: `Kept: will be moved to D:\…`. */
export function decidedNote(route: PhotoRoute, state: "kept" | "rejected"): string {
  if (state === "rejected") return `Rejected: will be ${route.rejectedTo}.`;
  return route.keptTo === null
    ? "Kept: you choose where on Review."
    : `Kept: will be ${route.keptTo}.`;
}

/** The toast on an arrow pressed on a decided photo (ADR 0006). */
export function alreadyDecidedToast(route: PhotoRoute, state: "kept" | "rejected"): string {
  if (state === "rejected")
    return `Already rejected. Will be ${route.rejectedTo}. Press U to undo.`;
  return route.keptTo === null
    ? "Already kept. You choose where on Review. Press U to undo."
    : `Already kept. Will be ${route.keptTo}. Press U to undo.`;
}
