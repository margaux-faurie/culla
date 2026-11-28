/**
 * Zoom geometry for the swipe screen (US-39, SPEC §4.1): where the full image sits at 100 % in the
 * view, so the point under the pointer stays put, and how the arrows and a drag pan it. Pure, so it
 * is tested without a DOM. Coordinates are CSS pixels in the view (the stage), origin top left.
 */

export interface Size {
  readonly w: number;
  readonly h: number;
}

export interface Point {
  readonly x: number;
  readonly y: number;
}

/** The fitted preview as shown in the view. */
export interface Rect extends Point, Size {}

export type PanDirection = "left" | "right" | "up" | "down";

/** Each arrow press pans a tenth of the view (proposed, US-39). */
export const PAN_STEP = 0.1;

/**
 * The full image's size in CSS pixels at 100 %: one image pixel on one screen pixel, so a 2880 px
 * wide screen at 200 % scaling shows 2880 image pixels across 1440 CSS pixels.
 */
export function cssSize(pixels: Size, devicePixelRatio: number): Size {
  const ratio = devicePixelRatio > 0 && Number.isFinite(devicePixelRatio) ? devicePixelRatio : 1;
  return { w: pixels.w / ratio, h: pixels.h / ratio };
}

/** Along one axis: centred when the image is smaller than the view, else kept inside its edges. */
function clampAxis(view: number, image: number, offset: number): number {
  if (image <= view) {
    return (view - image) / 2;
  }
  return Math.min(0, Math.max(view - image, offset));
}

/** The image's top-left corner, kept so panning stops at the photo's edges. */
export function clampOffset(view: Size, image: Size, offset: Point): Point {
  return { x: clampAxis(view.w, image.w, offset.x), y: clampAxis(view.h, image.h, offset.y) };
}

const inside = (rect: Rect, p: Point): boolean =>
  p.x >= rect.x && p.x <= rect.x + rect.w && p.y >= rect.y && p.y <= rect.y + rect.h;

/**
 * Where the full image goes when zoom starts: the point of the photo under the pointer stays under
 * it; with the pointer elsewhere (or none, from the keyboard), the photo's centre goes to the view's
 * centre (SPEC §4.1). The result is clamped to the edges.
 */
export function zoomOffset(view: Size, image: Size, fitted: Rect, pointer: Point | null): Point {
  if (pointer !== null && fitted.w > 0 && fitted.h > 0 && inside(fitted, pointer)) {
    const ix = ((pointer.x - fitted.x) / fitted.w) * image.w;
    const iy = ((pointer.y - fitted.y) / fitted.h) * image.h;
    return clampOffset(view, image, { x: pointer.x - ix, y: pointer.y - iy });
  }
  return clampOffset(view, image, { x: view.w / 2 - image.w / 2, y: view.h / 2 - image.h / 2 });
}

/** Moves the image by a drag of `dx`, `dy`: the photo follows the pointer. */
export function panBy(view: Size, image: Size, offset: Point, dx: number, dy: number): Point {
  return clampOffset(view, image, { x: offset.x + dx, y: offset.y + dy });
}

/** An arrow press: → shows more of the right side, so the image moves left; a tenth of the view. */
export function pan(view: Size, image: Size, offset: Point, direction: PanDirection): Point {
  const dx = view.w * PAN_STEP;
  const dy = view.h * PAN_STEP;
  switch (direction) {
    case "left":
      return panBy(view, image, offset, dx, 0);
    case "right":
      return panBy(view, image, offset, -dx, 0);
    case "up":
      return panBy(view, image, offset, 0, dy);
    case "down":
      return panBy(view, image, offset, 0, -dy);
  }
}
