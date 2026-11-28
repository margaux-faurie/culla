import { describe, expect, it } from "vitest";
import {
  apertureText,
  capturedText,
  compensationText,
  type DemoMeta,
  exifOf,
  focalText,
  isoText,
  shutterText,
} from "./demo-exif";

const captured = { localMs: Date.UTC(2026, 10, 28, 14, 3, 22), fromFile: false };

const meta: DemoMeta = {
  camera: "NIKON Z 6_3",
  lens: "NIKKOR Z 24-70mm f/4 S",
  shutter: { num: 1, den: 250 },
  aperture: { num: 40, den: 10 },
  iso: 400,
  focalLength: { num: 50, den: 1 },
  exposureCompensation: { num: -1, den: 3 },
  captured,
};

describe("EXIF overlay strings (US-40)", () => {
  it("writes every field as the overlay shows it", () => {
    expect(exifOf(meta)).toEqual({
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
    });
  });

  it("writes shutter speeds under and over a second", () => {
    expect(shutterText({ num: 1, den: 8000 })).toBe("1/8000 s");
    expect(shutterText({ num: 10, den: 1250 })).toBe("1/125 s");
    expect(shutterText({ num: 1, den: 3 })).toBe("1/3 s");
    expect(shutterText({ num: 13, den: 10 })).toBe("1.3 s");
    expect(shutterText({ num: 30, den: 1 })).toBe("30 s");
    expect(shutterText({ num: 0, den: 1 })).toBeNull();
    expect(shutterText({ num: 1, den: 0 })).toBeNull();
  });

  it("writes aperture, ISO, focal length and compensation", () => {
    expect(apertureText({ num: 28, den: 10 })).toBe("f/2.8");
    expect(apertureText({ num: 0, den: 1 })).toBeNull();
    expect(isoText(64)).toBe("ISO 64");
    expect(isoText(0)).toBeNull();
    expect(focalText({ num: 245, den: 10 })).toBe("24.5 mm");
    expect(compensationText({ num: 2, den: 3 })).toBe("+0.7 EV");
    expect(compensationText({ num: 0, den: 1 })).toBe("0 EV");
    expect(compensationText({ num: -2, den: 1 })).toBe("−2 EV");
  });

  it("writes the capture time as the camera recorded it, and says when it is the file's", () => {
    expect(capturedText(Date.UTC(2027, 0, 3, 9, 5, 7))).toBe("3 Jan 2027, 09:05:07");
    const fromFile = exifOf({ ...meta, captured: { ...captured, fromFile: true } });
    expect(fromFile.kind === "ready" && fromFile.capturedFromFile).toBe(true);
  });

  it("leaves a field the file does not record as null", () => {
    const none = exifOf({
      camera: null,
      lens: null,
      shutter: null,
      aperture: null,
      iso: null,
      focalLength: null,
      exposureCompensation: null,
      captured: null,
    });
    expect(none).toEqual({
      kind: "ready",
      camera: null,
      lens: null,
      shutter: null,
      aperture: null,
      iso: null,
      focalLength: null,
      exposureCompensation: null,
      captured: null,
      capturedFromFile: false,
    });
  });
});
