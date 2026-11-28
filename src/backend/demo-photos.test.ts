import { describe, expect, it } from "vitest";
import {
  demoPhoto,
  SCENE_COUNT,
  sceneFileName,
  sceneSvg,
  source2026,
  sourceAlps,
  sourceCard,
} from "./demo-photos";

// The committed scene files, as text, keyed like `./demo-scenes/scene-01.svg`.
const files: Readonly<Record<string, string>> = import.meta.glob<string>("./demo-scenes/*.svg", {
  eager: true,
  query: "?raw",
  import: "default",
});

describe("demo scenes", () => {
  it("are exactly the files sceneSvg draws, so a change to it needs the files regenerated", () => {
    const expected = Array.from(
      { length: SCENE_COUNT },
      (_, n) => `./demo-scenes/${sceneFileName(n)}`,
    );
    expect(Object.keys(files).sort()).toEqual(expected);
    for (let n = 0; n < SCENE_COUNT; n += 1) {
      // Compared without line endings, so a CRLF checkout still passes.
      expect(files[`./demo-scenes/${sceneFileName(n)}`]?.trim(), sceneFileName(n)).toBe(
        sceneSvg(n).trim(),
      );
    }
  });

  it("cover every photo of every demo source", () => {
    for (const specs of [source2026(), sourceAlps(), sourceCard()]) {
      expect(specs.length).toBeLessThanOrEqual(SCENE_COUNT);
    }
  });

  // Whether the URL is a file is the demo build's to check (tests/demo-site.test.ts, e2e): Vitest
  // runs with Vite's default inline limit, so here a small SVG comes back as a data: URL.
  it("give each photo of a source its own scene file", () => {
    const urls = source2026().map((spec, n) => demoPhoto(`p-${n}`, spec, n).previewUrl);
    expect(new Set(urls).size).toBe(urls.length);
  });
});
