import type { PhotoExif } from "../backend/types";

/**
 * Lays out the EXIF overlay (US-40) from the backend's display strings: the UI joins and labels
 * them, and never formats a value itself. Pure, so it is tested without a DOM.
 */

export const NOT_RECORDED = "Not recorded";

export interface ExifRow {
  readonly label: string;
  readonly value: string;
}

type Ready = Extract<PhotoExif, { kind: "ready" }>;

/** `1/250 s · f/4 · ISO 400 · 50 mm · −0.3 EV`, leaving out what the file does not record. */
export function exposureLine(exif: Ready): string {
  const parts = [
    exif.shutter,
    exif.aperture,
    exif.iso,
    exif.focalLength,
    exif.exposureCompensation,
  ].filter((part): part is string => part !== null);
  return parts.length === 0 ? NOT_RECORDED : parts.join(" · ");
}

export function exifRows(exif: Ready): ExifRow[] {
  const taken =
    exif.captured === null
      ? NOT_RECORDED
      : exif.capturedFromFile
        ? `${exif.captured}, from the file time`
        : exif.captured;
  return [
    { label: "Taken", value: taken },
    { label: "Camera", value: exif.camera ?? NOT_RECORDED },
    { label: "Lens", value: exif.lens ?? NOT_RECORDED },
    { label: "Exposure", value: exposureLine(exif) },
  ];
}
