import type { PhotoExif } from "./types";

/**
 * The EXIF overlay's display strings (US-40), as the backend writes them from a photo's metadata.
 * The web demo keeps its photos' metadata pre-extracted (SPEC §5.5) in the shape the core's
 * `PhotoMeta` gives: text already cleaned (control and format characters dropped, capped at 128
 * characters), the lens from `LensModel` or else `LensSpecification`, and the capture time with
 * where it came from (ADR 0028). Pure, so every format is tested. No GPS position and no body serial
 * exist in this shape, so none can reach the screen.
 */

export interface Ratio {
  readonly num: number;
  readonly den: number;
}

export interface DemoMeta {
  readonly camera: string | null;
  readonly lens: string | null;
  /** Seconds. */
  readonly shutter: Ratio | null;
  readonly aperture: Ratio | null;
  readonly iso: number | null;
  /** Millimetres. */
  readonly focalLength: Ratio | null;
  /** EV, as written by the camera (`-1/3`). */
  readonly exposureCompensation: Ratio | null;
  /** Milliseconds since 1970-01-01 00:00 in the camera's local time (`CaptureTime::local_ms`). */
  readonly captured: { readonly localMs: number; readonly fromFile: boolean } | null;
}

const value = (r: Ratio): number | null =>
  r.den !== 0 && Number.isFinite(r.num / r.den) ? r.num / r.den : null;

/** One decimal at most: `4`, `2.8`, `1.3`. */
const short = (n: number): string => String(Math.round(n * 10) / 10);

/**
 * `1/250 s` under a second, `2 s` or `1.3 s` from a second up. Between 1/3 s and a second the
 * denominator keeps one decimal, as cameras show it: 0.77 s is `1/1.3 s`, 0.625 s is `1/1.6 s`.
 */
export function shutterText(r: Ratio): string | null {
  const t = value(r);
  if (t === null || t <= 0) return null;
  if (t >= 1) return `${short(t)} s`;
  const d = 1 / t;
  return `1/${d >= 3 ? Math.round(d) : short(d)} s`;
}

/** `f/4`, `f/2.8`. */
export function apertureText(r: Ratio): string | null {
  const f = value(r);
  return f === null || f <= 0 ? null : `f/${short(f)}`;
}

export function isoText(iso: number): string | null {
  return Number.isInteger(iso) && iso > 0 ? `ISO ${iso}` : null;
}

/** `50 mm`. */
export function focalText(r: Ratio): string | null {
  const mm = value(r);
  return mm === null || mm <= 0 ? null : `${short(mm)} mm`;
}

/** `−0.3 EV` with a true minus sign, `+0.7 EV`, `0 EV`. */
export function compensationText(r: Ratio): string | null {
  const ev = value(r);
  if (ev === null) return null;
  const rounded = Math.round(ev * 10) / 10;
  if (rounded === 0) return "0 EV";
  return `${rounded < 0 ? "−" : "+"}${short(Math.abs(rounded))} EV`;
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** `28 Nov 2026, 14:03:22`, in the camera's local time as written: no time zone is applied. */
export function capturedText(localMs: number): string | null {
  if (!Number.isFinite(localMs)) return null;
  const d = new Date(localMs);
  const two = (n: number) => String(n).padStart(2, "0");
  const time = `${two(d.getUTCHours())}:${two(d.getUTCMinutes())}:${two(d.getUTCSeconds())}`;
  return `${d.getUTCDate()} ${MONTHS[d.getUTCMonth()] ?? ""} ${d.getUTCFullYear()}, ${time}`;
}

const or = <T>(v: T | null, f: (v: T) => string | null): string | null =>
  v === null ? null : f(v);

/** The overlay's strings for a photo whose metadata the EXIF pass has read. */
export function exifOf(meta: DemoMeta): PhotoExif {
  return {
    kind: "ready",
    camera: meta.camera,
    lens: meta.lens,
    shutter: or(meta.shutter, shutterText),
    aperture: or(meta.aperture, apertureText),
    iso: or(meta.iso, isoText),
    focalLength: or(meta.focalLength, focalText),
    exposureCompensation: or(meta.exposureCompensation, compensationText),
    captured: or(meta.captured, (c) => capturedText(c.localMs)),
    capturedFromFile: meta.captured?.fromFile === true,
  };
}
