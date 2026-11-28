import { describe, expect, it } from "vitest";
import { clampOffset, cssSize, pan, panBy, zoomOffset } from "./zoom";

const view = { w: 1000, h: 600 };
const image = { w: 6000, h: 4000 };
// The fitted preview: 900 × 600 centred in the view.
const fitted = { x: 50, y: 0, w: 900, h: 600 };

describe("zoom geometry (US-39)", () => {
  it("keeps the point under the pointer where it is", () => {
    const pointer = { x: 500, y: 300 };
    const o = zoomOffset(view, image, fitted, pointer);
    // The photo's centre (3000, 2000) is under the pointer.
    expect(o).toEqual({ x: 500 - 3000, y: 300 - 2000 });
    const corner = zoomOffset(view, image, fitted, { x: 140, y: 60 });
    // 10 % across and down the photo: image point (600, 400) under (140, 60).
    expect(corner).toEqual({ x: 140 - 600, y: 60 - 400 });
  });

  it("zooms on the centre when the pointer is off the photo or absent", () => {
    const centre = { x: 500 - 3000, y: 300 - 2000 };
    expect(zoomOffset(view, image, fitted, null)).toEqual(centre);
    expect(zoomOffset(view, image, fitted, { x: 10, y: 300 })).toEqual(centre);
  });

  it("stops at the photo's edges, also when zooming near a corner", () => {
    expect(zoomOffset(view, image, fitted, { x: 50, y: 0 })).toEqual({ x: 0, y: 0 });
    expect(zoomOffset(view, image, fitted, { x: 950, y: 600 })).toEqual({ x: -5000, y: -3400 });
    expect(clampOffset(view, image, { x: 200, y: -9000 })).toEqual({ x: 0, y: -3400 });
  });

  it("centres an image smaller than the view on that axis", () => {
    expect(clampOffset(view, { w: 800, h: 4000 }, { x: -300, y: 50 })).toEqual({ x: 100, y: 0 });
  });

  it("pans a tenth of the view per arrow, the way the arrow points", () => {
    const start = { x: -2500, y: -1700 };
    expect(pan(view, image, start, "right")).toEqual({ x: -2600, y: -1700 });
    expect(pan(view, image, start, "left")).toEqual({ x: -2400, y: -1700 });
    expect(pan(view, image, start, "down")).toEqual({ x: -2500, y: -1760 });
    expect(pan(view, image, start, "up")).toEqual({ x: -2500, y: -1640 });
    expect(pan(view, image, { x: 0, y: 0 }, "left")).toEqual({ x: 0, y: 0 });
    expect(pan(view, image, { x: -5000, y: -3400 }, "down")).toEqual({ x: -5000, y: -3400 });
  });

  it("drags the photo with the pointer, within its edges", () => {
    expect(panBy(view, image, { x: -2500, y: -1700 }, 30, -20)).toEqual({ x: -2470, y: -1720 });
    expect(panBy(view, image, { x: -10, y: 0 }, 30, 0)).toEqual({ x: 0, y: 0 });
  });

  it("shows one image pixel per screen pixel", () => {
    expect(cssSize({ w: 6048, h: 4032 }, 2)).toEqual({ w: 3024, h: 2016 });
    expect(cssSize({ w: 6048, h: 4032 }, 1)).toEqual({ w: 6048, h: 4032 });
    expect(cssSize({ w: 6048, h: 4032 }, 0)).toEqual({ w: 6048, h: 4032 });
  });
});
