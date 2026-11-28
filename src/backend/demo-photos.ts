/**
 * Sample photos for the web demo. The author's photos (about 25 at 1600 px, SPEC §5.5) are not
 * chosen yet (SPEC §10), so the demo draws simple scenes as inline SVG: no network, no file.
 */
import { type DemoGroup, kindOf, type Sidecar } from "./demo-routes";
import type { PhotoSummary } from "./types";

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

const svg = (scene: Scene, n: number): string => {
  const [w, h] = scene.portrait ? [1067, 1600] : [1600, 1067];
  const sunX = 200 + ((n * 173) % (w - 400));
  const horizon = Math.round(h * (0.55 + ((n * 37) % 20) / 100));
  const sunY = Math.round(horizon * 0.4);
  const sunR = Math.round(w / 12);
  const markup = [
    `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">`,
    `<rect width="${w}" height="${h}" fill="${scene.sky}"/>`,
    `<circle cx="${sunX}" cy="${sunY}" r="${sunR}" fill="${scene.sun}"/>`,
    `<rect y="${horizon}" width="${w}" height="${h - horizon}" fill="${scene.ground}"/>`,
    "</svg>",
  ].join("");
  return `data:image/svg+xml,${encodeURIComponent(markup)}`;
};

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
    previewUrl: svg(scene, n),
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
