/**
 * Commit rule for a horizontal drag (SPEC §4.1, US-25): a swipe commits past an offset or a velocity
 * threshold, else the card springs back. Pure, so it is tested without Motion; the card stack feeds
 * it Motion's `info.offset.x` and `info.velocity.x` from `onDragEnd` once Motion is added (ADR 0015).
 *
 * Threshold values are proposed by the agent and awaiting Margaux (SPEC §10): a quarter of the card
 * width, at least 120 px; or a flick of 800 px/s in the drag's direction after at least 24 px.
 */
import type { Verdict } from "../decisions/events";

export const OFFSET_FRACTION = 0.25;
export const MIN_OFFSET_PX = 120;
export const VELOCITY_PX_PER_S = 800;
export const MIN_FLICK_PX = 24;

export function dragOutcome(offsetX: number, velocityX: number, cardWidth: number): Verdict | null {
  const threshold = Math.max(MIN_OFFSET_PX, cardWidth * OFFSET_FRACTION);
  const verdict: Verdict = offsetX > 0 ? "keep" : "reject";
  if (Math.abs(offsetX) >= threshold) {
    return verdict;
  }
  const sameWay = Math.sign(velocityX) === Math.sign(offsetX);
  const flick = sameWay && Math.abs(velocityX) >= VELOCITY_PX_PER_S;
  return flick && Math.abs(offsetX) >= MIN_FLICK_PX ? verdict : null;
}
