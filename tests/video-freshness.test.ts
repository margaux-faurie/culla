// The demo video must follow the app (docs/videos.md, "Keeping the video current"). Every pull
// request that changes a screen it films acknowledges it (Margaux, 28 Nov 2026): this fails until
// the video is rendered again (`node docs/assets/video/hash.mjs --write`) or the change is listed in
// docs/assets/video/pending.md with `hash.mjs --pending "<line>"`, which records the hash it saw.
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
  pendingLine(line: string): string | null;
  freshness(input: {
    current: string;
    renderedFrom: string;
    renderedAt: string;
    pendingFrom: string | null;
    pendingText: string;
  }): Freshness;
  load(): {
    screens: {
      paths: string[];
      exclude: string[];
      renderedFrom: string;
      renderedAt: string;
      pendingFrom: string | null;
    };
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
    expect(hash.pendingLine(LINE.slice(2))).toBe(LINE);
    expect(hash.pendingLine(LINE)).toBe(LINE);
    expect(hash.pendingLine("2026-11-28 #57: review")).toBeNull();
  });

  it("passes at the render's hash, or at the hash a pull request acknowledged with its line", () => {
    const base = { current: "b", renderedFrom: "a", renderedAt: "2026-11-28", pendingFrom: "b" };
    expect(hash.freshness({ ...base, current: "a", pendingText: "" }).fresh).toBe(true);
    expect(hash.freshness({ ...base, pendingText: LINE }).fresh).toBe(true);
    // A later change to the screens is not covered by an earlier pull request's line.
    const later = hash.freshness({ ...base, current: "c", pendingText: LINE });
    expect(later.fresh).toBe(false);
    expect(later.message).toContain('node docs/assets/video/hash.mjs --pending "');
    expect(later.message).toContain("#<PR>: <screen> changed — re-render before release");
    expect(later.message).toContain("node docs/assets/video/hash.mjs --write");
    // The acknowledged hash needs its line, dated since the render.
    expect(hash.freshness({ ...base, pendingText: "" }).fresh).toBe(false);
    const old = LINE.replace("2026-11-28", "2026-11-27");
    expect(hash.freshness({ ...base, pendingText: old }).fresh).toBe(false);
  });

  it("every pattern of screens.json still matches a file", () => {
    const { screens, files } = hash.load();
    expect(screens.paths.length).toBeGreaterThan(0);
    expect(files.length).toBeGreaterThan(0);
    for (const glob of screens.paths) {
      const re = hash.globToRegExp(glob);
      expect(
        files.some((f) => re.test(f)),
        `${glob} matches no file`,
      ).toBe(true);
    }
  });

  it("the video follows the screens it films, or a pull request acknowledged the change", () => {
    const { screens, current, pendingText } = hash.load();
    const result = hash.freshness({ current, ...screens, pendingText });
    expect(result.fresh, result.message).toBe(true);
  });
});
