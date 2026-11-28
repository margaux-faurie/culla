import { describe, expect, it } from "vitest";
import type { PhotoExif } from "../backend/types";
import { exifRows, exposureLine } from "./exif-text";

const ready: Extract<PhotoExif, { kind: "ready" }> = {
  kind: "ready",
  camera: "NIKON Z 6_3",
  lens: "NIKKOR Z 24-70mm f/4 S",
  shutter: "1/250 s",
  aperture: "f/4",
  iso: "ISO 400",
  focalLength: "50 mm",
  exposureCompensation: "−0.3 EV",
  captured: "28 Nov 2026, 14:03:22",
  capturedFromFile: false,
};

describe("EXIF overlay layout (US-40)", () => {
  it("shows the date, camera, lens and one exposure line", () => {
    expect(exifRows(ready)).toEqual([
      { label: "Taken", value: "28 Nov 2026, 14:03:22" },
      { label: "Camera", value: "NIKON Z 6_3" },
      { label: "Lens", value: "NIKKOR Z 24-70mm f/4 S" },
      { label: "Exposure", value: "1/250 s · f/4 · ISO 400 · 50 mm · −0.3 EV" },
    ]);
  });

  it("says when the time is the file's, and Not recorded for a missing field", () => {
    const rows = exifRows({ ...ready, lens: null, camera: null, capturedFromFile: true });
    expect(rows.map((r) => r.value)).toEqual([
      "28 Nov 2026, 14:03:22, from the file time",
      "Not recorded",
      "Not recorded",
      "1/250 s · f/4 · ISO 400 · 50 mm · −0.3 EV",
    ]);
    expect(exifRows({ ...ready, captured: null })[0]?.value).toBe("Not recorded");
  });

  it("leaves a missing exposure value out of the line, and says so when none is left", () => {
    expect(exposureLine({ ...ready, iso: null, exposureCompensation: null })).toBe(
      "1/250 s · f/4 · 50 mm",
    );
    expect(
      exposureLine({
        ...ready,
        shutter: null,
        aperture: null,
        iso: null,
        focalLength: null,
        exposureCompensation: null,
      }),
    ).toBe("Not recorded");
  });

  it("prints text from the card as it comes, for React to render as text, never markup", () => {
    const rows = exifRows({ ...ready, lens: "<b>Lens</b>" });
    expect(rows[2]?.value).toBe("<b>Lens</b>");
  });
});
