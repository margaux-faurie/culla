// The demo video must follow the app (docs/videos.md, "Keeping the video current"). When a screen
// it films changes, this fails until the video is rendered again (and the hash recorded with
// `node docs/assets/video/hash.mjs --write`) or the change is listed in docs/assets/video/pending.md.
import { fileURLToPath, pathToFileURL } from "node:url";
import { describe, expect, it } from "vitest";

interface Entry {
  readonly date: string;
  readonly ref: string;
  readonly screen: string;
}

interface Freshness {
  readonly fresh: boolean;
  readonly pending: readonly Entry[];
  readonly message: string;
}

/** The parts of docs/assets/video/hash.mjs used here (plain Node, no types of its own). */
interface HashModule {
  globToRegExp(glob: string): RegExp;
  pendingEntries(text: string): Entry[];
  freshness(input: {
    current: string;
    renderedFrom: string;
    renderedAt: string;
    pendingText: string;
  }): Freshness;
  load(): {
    screens: { paths: string[]; exclude: string[]; renderedFrom: string; renderedAt: string };
    files: string[];
    current: string;
    pendingText: string;
  };
}

const script = fileURLToPath(new URL("../docs/assets/video/hash.mjs", import.meta.url));
const hash = (await import(pathToFileURL(script).href)) as HashModule;

const LINE = "- 2026-11-28 #57: review (lean layout) changed — re-render before release";

describe("video freshness (docs/videos.md)", () => {
  it("reads the patterns of screens.json", () => {
    expect(hash.globToRegExp("src/review/**").test("src/review/model.ts")).toBe(true);
    expect(hash.globToRegExp("src/review/**").test("src/reviews.ts")).toBe(false);
    expect(hash.globToRegExp("src/backend/demo*.ts").test("src/backend/demo-plan.ts")).toBe(true);
    expect(hash.globToRegExp("src/backend/demo*.ts").test("src/backend/demo/x.ts")).toBe(false);
    expect(hash.globToRegExp("**/*.test.ts").test("src/swipe/keys.test.ts")).toBe(true);
  });

  it("reads pending.md lines only in the agreed form", () => {
    expect(hash.pendingEntries(`# Title\n\n${LINE}\n- 2026-11-28 #57: review\n`)).toEqual([
      { date: "2026-11-28", ref: "57", screen: "review (lean layout)" },
    ]);
    expect(
      hash.pendingEntries("- 2026-11-29 #feat/x: swipe changed — re-render before release"),
    ).toEqual([{ date: "2026-11-29", ref: "feat/x", screen: "swipe" }]);
  });

  it("is fresh at the render's hash, or with a pending line dated since the render", () => {
    const base = { current: "b", renderedFrom: "a", renderedAt: "2026-11-28" };
    expect(hash.freshness({ ...base, current: "a", pendingText: "" }).fresh).toBe(true);
    expect(hash.freshness({ ...base, pendingText: LINE }).fresh).toBe(true);
    const old = LINE.replace("2026-11-28", "2026-11-27");
    const stale = hash.freshness({ ...base, pendingText: old });
    expect(stale.fresh).toBe(false);
    expect(stale.message).toContain("render-all");
    expect(stale.message).toContain("hash.mjs --write");
    expect(stale.message).toContain("pending.md");
  });

  it("every pattern of screens.json still matches a file", () => {
    const { screens, files } = hash.load();
    for (const glob of screens.paths) {
      const re = hash.globToRegExp(glob);
      expect(
        files.some((f) => re.test(f)),
        `${glob} matches no file`,
      ).toBe(true);
    }
  });

  it("the video follows the screens it films, or the change is listed in pending.md", () => {
    const { screens, current, pendingText } = hash.load();
    const result = hash.freshness({ current, ...screens, pendingText });
    expect(result.fresh, result.message).toBe(true);
  });
});
