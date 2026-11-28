/**
 * Sample photos for the web demo. The author's photos (about 25 at 1600 px, SPEC §5.5) are not
 * chosen yet (SPEC §10), so the demo shows simple drawn scenes. They are SVG files in
 * `demo-scenes/`, bundled by Vite under /assets/, because the demo's CSP allows images from its
 * own origin only (`img-src 'self'`, public-demo/_headers): a `data:` URL would be blocked.
 * `sceneSvg` is their one source; `demo-photos.test.ts` fails if a file drifts from it.
 */
import { type DemoGroup, kindOf, type Sidecar } from "./demo-routes";
import type { FullImage, PhotoSummary } from "./types";

interface Scene {
  readonly sky: string;
  readonly ground: string;
  readonly sun: string;
  readonly portrait: boolean;
  readonly alt: string;
}

const SCENES: readonly Scene[] = [
  {
    sky: "#9fc6e8",
    ground: "#5b7f4a",
    sun: "#ffe3a3",
    portrait: false,
    alt: "Green hills under a pale blue sky",
  },
  {
    sky: "#f2b48c",
    ground: "#3d3a52",
    sun: "#fff1c9",
    portrait: false,
    alt: "Sunset over a dark ridge",
  },
  {
    sky: "#2d3e66",
    ground: "#14182a",
    sun: "#e8e4d0",
    portrait: true,
    alt: "Moon above a city at night",
  },
  {
    sky: "#cfe3e9",
    ground: "#d8c49a",
    sun: "#ffffff",
    portrait: false,
    alt: "A beach on a hazy morning",
  },
  {
    sky: "#7fa7c9",
    ground: "#a35f3c",
    sun: "#fff4d6",
    portrait: true,
    alt: "Red rooftops under a clear sky",
  },
  {
    sky: "#e7d9c3",
    ground: "#6c5a49",
    sun: "#f6b26b",
    portrait: false,
    alt: "A dusty road at noon",
  },
  {
    sky: "#bcd7c6",
    ground: "#2f5c46",
    sun: "#fdf6dc",
    portrait: false,
    alt: "A pine forest under a green sky",
  },
  {
    sky: "#e9a9b5",
    ground: "#57415f",
    sun: "#fbe7c6",
    portrait: true,
    alt: "Pink dusk over purple mountains",
  },
];

/** How many scene files there are: one per photo of the largest demo source (`source2026`). */
export const SCENE_COUNT = 24;

/** Where scene `n` puts its sun and horizon, in the preview's own pixels. */
function layout(n: number) {
  const scene = SCENES[n % SCENES.length] ?? (SCENES[0] as Scene);
  const [w, h] = scene.portrait ? [1067, 1600] : [1600, 1067];
  const sunX = 200 + ((n * 173) % (w - 400));
  const horizon = Math.round(h * (0.55 + ((n * 37) % 20) / 100));
  const sunY = Math.round(horizon * 0.4);
  const sunR = Math.round(w / 12);
  return { scene, w, h, sunX, horizon, sunY, sunR };
}

/** The markup of scene file `n` (0-based): `demo-scenes/scene-01.svg` is `sceneSvg(0)`. */
export const sceneSvg = (n: number): string => {
  const { scene, w, h, sunX, horizon, sunY, sunR } = layout(n);
  const markup = [
    `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">`,
    // The alt text is plain words (no markup characters), so it goes in as is.
    `<title>${scene.alt}</title>`,
    `<rect width="${w}" height="${h}" fill="${scene.sky}"/>`,
    `<circle cx="${sunX}" cy="${sunY}" r="${sunR}" fill="${scene.sun}"/>`,
    `<rect y="${horizon}" width="${w}" height="${h - horizon}" fill="${scene.ground}"/>`,
    "</svg>",
  ].join("");
  return `${markup}\n`;
};

/** A 24 MP frame (Z6 class): the size of the full image zoom shows at 100 % (US-39). */
export const FULL_LONG = 6048;
export const FULL_SHORT = 4032;

/**
 * The markup of full-size scene file `n` (0-based), `demo-scenes-full/full-01.svg` for n = 0: the
 * same picture as `sceneSvg(n)` drawn at 6048 × 4032 (4032 × 6048 upright), with fine detail a
 * fitted preview cannot show (a grid in the sky, grass on the ground), so zoom and panning have
 * something to look at.
 */
export const fullSceneSvg = (n: number): string => {
  const { scene, w, h, sunX, horizon, sunY, sunR } = layout(n);
  const [fw, fh] = scene.portrait ? [FULL_SHORT, FULL_LONG] : [FULL_LONG, FULL_SHORT];
  const markup = [
    `<svg xmlns="http://www.w3.org/2000/svg" width="${fw}" height="${fh}" viewBox="0 0 ${w} ${h}" preserveAspectRatio="none">`,
    `<title>${scene.alt}, full size</title>`,
    "<defs>",
    `<pattern id="grid" width="6" height="6" patternUnits="userSpaceOnUse"><path d="M6 0H0V6" fill="none" stroke="${scene.sun}" stroke-opacity="0.4" stroke-width="0.25"/></pattern>`,
    `<pattern id="grass" width="2" height="4" patternUnits="userSpaceOnUse"><path d="M0.3 4L1 0.6L1.7 4" fill="none" stroke="${scene.sky}" stroke-opacity="0.5" stroke-width="0.25"/></pattern>`,
    "</defs>",
    `<rect width="${w}" height="${h}" fill="${scene.sky}"/>`,
    `<rect width="${w}" height="${horizon}" fill="url(#grid)"/>`,
    `<circle cx="${sunX}" cy="${sunY}" r="${sunR}" fill="${scene.sun}"/>`,
    `<rect y="${horizon}" width="${w}" height="${h - horizon}" fill="${scene.ground}"/>`,
    `<rect y="${horizon}" width="${w}" height="${h - horizon}" fill="url(#grass)"/>`,
    "</svg>",
  ].join("");
  return `${markup}\n`;
};

/** The scene file name for photo `n` (0-based). */
export const sceneFileName = (n: number): string =>
  `scene-${String((n % SCENE_COUNT) + 1).padStart(2, "0")}.svg`;

// Vite emits each file under /assets/ with a hashed name and gives its URL.
const SCENE_URLS: Readonly<Record<string, string>> = import.meta.glob<string>(
  "./demo-scenes/*.svg",
  { eager: true, query: "?url", import: "default" },
);

const sceneUrl = (n: number): string => {
  const url = SCENE_URLS[`./demo-scenes/${sceneFileName(n)}`];
  if (url === undefined) {
    throw new Error(`missing demo scene ${sceneFileName(n)}`);
  }
  return url;
};

/** The full-size scene file name for photo `n` (0-based). */
export const fullSceneFileName = (n: number): string =>
  `full-${String((n % SCENE_COUNT) + 1).padStart(2, "0")}.svg`;

const FULL_URLS: Readonly<Record<string, string>> = import.meta.glob<string>(
  "./demo-scenes-full/*.svg",
  { eager: true, query: "?url", import: "default" },
);

/**
 * The full image zoom loads for photo `n` (US-39): its full-size scene file. A lone NEF shows its
 * largest embedded JPEG, not RAW pixels (ADR 0011).
 */
export function demoFullImage(spec: DemoPhotoSpec, n: number): FullImage {
  const url = FULL_URLS[`./demo-scenes-full/${fullSceneFileName(n)}`];
  if (url === undefined) {
    throw new Error(`missing demo scene ${fullSceneFileName(n)}`);
  }
  const portrait = layout(n).scene.portrait;
  return {
    url,
    width: portrait ? FULL_SHORT : FULL_LONG,
    height: portrait ? FULL_LONG : FULL_SHORT,
    embeddedPreview: spec.lone === true,
  };
}

export interface DemoPhotoSpec {
  readonly name: string;
  readonly folder: readonly string[];
  readonly lone?: boolean;
  readonly jpegOnly?: boolean;
  readonly sidecars?: readonly Sidecar[];
  readonly cameraProtected?: boolean;
}

/** The file facts of a spec, which only the backend sees. */
export function demoGroup(spec: DemoPhotoSpec): DemoGroup {
  const lone = spec.lone === true;
  const raw = lone || spec.jpegOnly === true ? null : ".NEF";
  // The camera's protect marks the JPEG and the NEF it wrote.
  const written = [lone ? ".NEF" : ".JPG", ...(raw === null ? [] : [raw])];
  return {
    name: spec.name,
    folder: spec.folder,
    primary: lone ? ".NEF" : ".JPG",
    raw,
    sidecars: spec.sidecars ?? [],
    readOnly: spec.cameraProtected === true ? written : [],
  };
}

/** What the UI gets for a spec: display data and an opaque id. */
export function demoPhoto(id: string, spec: DemoPhotoSpec, n: number): PhotoSummary {
  const scene = SCENES[n % SCENES.length] ?? (SCENES[0] as Scene);
  const group = demoGroup(spec);
  return {
    id,
    name: group.name,
    kind: kindOf(group),
    cameraProtected: group.readOnly.length > 0,
    previewUrl: sceneUrl(n),
    alt: spec.lone === true ? `${scene.alt}, from the NEF's embedded preview` : scene.alt,
  };
}

/** Photos of the demo's main source: 4 at its root, then Barcelona and Lyon. */
export function source2026(): DemoPhotoSpec[] {
  const specs: DemoPhotoSpec[] = [
    { name: "MGF_7001", folder: [] },
    { name: "MGF_7002", folder: [], sidecars: [{ suffix: ".xmp", follows: "raw" }] },
    { name: "MGF_7003", folder: [], lone: true },
    { name: "MGF_7004", folder: [], jpegOnly: true },
  ];
  for (let i = 0; i < 10; i += 1) {
    specs.push({
      name: `MGF_${7270 + i}`,
      folder: ["Barcelona"],
      ...(i === 3 ? { sidecars: [{ suffix: ".xmp", follows: "raw" } as const] } : {}),
      ...(i === 6 ? { cameraProtected: true } : {}),
    });
  }
  for (let i = 0; i < 10; i += 1) {
    specs.push({
      name: `MGF_${8040 + i}`,
      folder: ["Lyon"],
      ...(i === 2 ? { sidecars: [{ suffix: ".NEF.xmp", follows: "raw" } as const] } : {}),
    });
  }
  return specs;
}

export function sourceAlps(): DemoPhotoSpec[] {
  return Array.from({ length: 6 }, (_, i) => ({ name: `MGF_${5100 + i}`, folder: [] }));
}

export function sourceCard(): DemoPhotoSpec[] {
  return Array.from({ length: 8 }, (_, i) => ({ name: `DSC_${1200 + i}`, folder: [] }));
}
