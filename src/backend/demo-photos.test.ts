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

  it("give every photo of every demo source a scene of its own", () => {
    for (const specs of [source2026(), sourceAlps(), sourceCard()]) {
      expect(specs.length).toBeLessThanOrEqual(SCENE_COUNT);
    }
  });

  it("are files, never data: URLs, which the demo's CSP blocks (img-src 'self')", () => {
    source2026().forEach((spec, n) => {
      const { previewUrl } = demoPhoto(`p-${n}`, spec, n);
      expect(previewUrl).not.toMatch(/^data:/i);
      expect(previewUrl).toMatch(/scene-\d{2}[^/]*\.svg$/);
    });
  });
});
