/**
 * Films the web demo for the 30 s demo video (docs/videos.md (a)): the swipe, review and apply
 * shots. Not part of the e2e suite: it runs only with its own config, `playwright.config.ts` here.
 *
 *   pnpm build:demo
 *   CULLA_VIDEO_PHOTOS=<dir> CULLA_VIDEO_BPM=120 \
 *     pnpm exec playwright test -c docs/assets/video/playwright.config.ts
 *   node docs/assets/video/compose.mjs --capture <dir> --photos <dir> --fonts <dir> --out <dir>
 *
 * Real photos (Margaux's choice, 28 Nov 2026) without committing them: with CULLA_VIDEO_PHOTOS set
 * to a folder of JPEGs (already oriented, sized and stripped of all metadata), the page's drawn
 * demo photos are swapped for those files as the page sets them, and Playwright serves them from
 * that folder. The order follows CULLA_VIDEO_PHOTO_ORDER (file names, comma separated; the demo's
 * session order) or else the folder's sorted names. Neither the photos nor anything filmed with
 * them goes into the repository: the capture folder (CULLA_VIDEO_CAPTURE) must be outside it.
 * Without CULLA_VIDEO_PHOTOS the demo's own drawn photos are filmed.
 *
 * Each shot is filmed frame by frame on a stopped clock (Playwright's `page.clock` plus every CSS
 * animation paused and stepped), one JPEG screenshot (quality 92) per 1/30 s, into
 * `<CULLA_VIDEO_CAPTURE>/<shot>/` with a `timeline.json` (frames, and the decisions to label on
 * screen). Chromium's screencast drops the frames of the 300 ms swipe exit; this does not.
 * Actions fall on beats of CULLA_VIDEO_BPM (default 120), so the cuts compose.mjs makes on the
 * same tempo land on the music. 1920×1080 at device scale 1; zooms are a CSS transform on #root,
 * drawn by the browser, so zoomed text stays sharp.
 */
import { readdirSync, readFileSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect, type Page, test } from "@playwright/test";

const OUT = process.env.CULLA_VIDEO_CAPTURE ?? join(tmpdir(), "culla-video-capture");
const PHOTOS = process.env.CULLA_VIDEO_PHOTOS ?? null;
const BPM = Number(process.env.CULLA_VIDEO_BPM ?? "120");
const BEAT = 60 / BPM;
const VIEW = { width: 1920, height: 1080 } as const;

/** The demo remembers move, as after a first apply (e2e/helpers.ts `rememberKeepMode`). */
const MEMORY = JSON.stringify({
  bySource: { "src-alps": "dst-archive" },
  lastUsed: "dst-selects",
  keepMode: "move",
});

/** The photo files in session order, or none to film the demo's drawn photos. */
function photoFiles(): string[] {
  if (PHOTOS === null) {
    return [];
  }
  const all = readdirSync(PHOTOS).filter((f) => /\.jpe?g$/i.test(f)).sort();
  const order = (process.env.CULLA_VIDEO_PHOTO_ORDER ?? "").split(",").filter((f) => f !== "");
  return [...order, ...all.filter((f) => !order.includes(f))];
}

/**
 * Page helpers, as a string so this file needs no DOM types. The photo swap finds which demo photo
 * a drawn preview is from its sun and horizon, as `svg()` in src/backend/demo-photos.ts draws them
 * (keep the two in step), and points the image at /__video-photos/<n>.
 */
const pageHelpers = (swap: boolean) => `
localStorage.setItem("culla-demo:destinations", ${JSON.stringify(MEMORY)});
// The swipe screen's focus outline runs round the whole frame once the video is full-bleed; the
// video hides it (and only it).
window.addEventListener("DOMContentLoaded", () => {
  const style = document.createElement("style");
  style.textContent = "main.swipe:focus, main.swipe:focus-visible { outline: none !important; }";
  document.head.appendChild(style);
});
if (${swap}) {
  const indexOf = (url) => {
    const svg = decodeURIComponent(url.slice(url.indexOf(",") + 1));
    const num = (re) => Number((svg.match(re) || [])[1]);
    const w = num(/width="(\\d+)"/), h = num(/height="(\\d+)"/);
    const cx = num(/cx="(\\d+)"/), y = num(/rect y="(\\d+)"/);
    for (let n = 0; n < 256; n += 1) {
      const sunX = 200 + ((n * 173) % (w - 400));
      const horizon = Math.round(h * (0.55 + ((n * 37) % 20) / 100));
      if (sunX === cx && horizon === y) return n;
    }
    return -1;
  };
  const swapUrl = (v) => {
    if (typeof v !== "string" || !v.startsWith("data:image/svg+xml")) return v;
    const n = indexOf(v);
    return n < 0 ? v : "/__video-photos/" + n;
  };
  const src = Object.getOwnPropertyDescriptor(HTMLImageElement.prototype, "src");
  Object.defineProperty(HTMLImageElement.prototype, "src", {
    configurable: true,
    get() { return src.get.call(this); },
    set(v) { src.set.call(this, swapUrl(v)); },
  });
  const setAttribute = Element.prototype.setAttribute;
  Element.prototype.setAttribute = function (name, value) {
    return setAttribute.call(this, name, this instanceof HTMLImageElement && name === "src" ? swapUrl(value) : value);
  };
}
window.__cullaZoomPlan = (selectors, maxScale, anchorY) => {
  const root = document.getElementById("root");
  const from = root.__zoom || { s: 1, x: 0, y: 0 };
  let to = { s: 1, x: 0, y: 0 };
  if (selectors.length > 0) {
    const keep = root.style.transform;
    root.style.transform = "";
    const base = root.getBoundingClientRect();
    const rects = selectors
      .flatMap((s) => [...document.querySelectorAll(s)].map((e) => e.getBoundingClientRect()))
      .filter((r) => r.width > 0 && r.height > 0);
    root.style.transform = keep;
    const l = Math.min(...rects.map((r) => r.left)), t = Math.min(...rects.map((r) => r.top));
    const r = Math.max(...rects.map((r) => r.right)), b = Math.max(...rects.map((r) => r.bottom));
    const vw = innerWidth, vh = innerHeight;
    const s = Math.min(maxScale, (vw * 0.88) / (r - l), (vh * 0.8) / (b - t));
    const lx = (l + r) / 2 - base.left, ly = (t + b) / 2 - base.top;
    let x = vw / 2 - base.left - s * lx, y = vh * anchorY - base.top - s * ly;
    x = Math.min(-base.left, Math.max(vw - base.left - s * base.width, x));
    y = Math.min(-base.top, Math.max(vh - base.top - s * Math.max(base.height, vh), y));
    to = { s, x, y };
  }
  root.__zoomPlan = { from, to };
  root.style.transformOrigin = "0 0";
};
window.__cullaZoomAt = (k) => {
  const root = document.getElementById("root");
  const { from, to } = root.__zoomPlan;
  const z = { s: from.s + (to.s - from.s) * k, x: from.x + (to.x - from.x) * k, y: from.y + (to.y - from.y) * k };
  root.style.transform = z.s === 1 && z.x === 0 && z.y === 0 ? "" : "translate(" + z.x + "px," + z.y + "px) scale(" + z.s + ")";
  root.__zoom = z;
};
window.__cullaSeen = new WeakSet();
window.__cullaStep = (ms) => {
  for (const a of document.getAnimations()) {
    if (!window.__cullaSeen.has(a)) {
      window.__cullaSeen.add(a);
      a.pause();
      a.currentTime = 0;
    }
    if (a.playState === "paused") a.currentTime = (a.currentTime || 0) + ms;
  }
};
`;

const FPS = 30;
const FRAME_MS = 1000 / FPS;

interface Frame {
  readonly t: number;
  readonly file: string;
}
interface KeyEvent {
  readonly t: number;
  readonly keys: readonly string[];
  readonly label: string;
}

/**
 * One shot, filmed frame by frame on a stopped clock: each frame moves the page's clock (timers,
 * Date, requestAnimationFrame) and every CSS animation on by 1/30 s, then takes a JPEG screenshot
 * (quality 92). So the swipe exit and the fades are all there however slow the machine is.
 */
class Shot {
  private readonly frames: Frame[] = [];
  private readonly events: KeyEvent[] = [];
  private last: Buffer | null = null;
  private lastFile = "";
  private constructor(
    private readonly page: Page,
    private readonly dir: string,
  ) {}

  static async begin(page: Page, name: string): Promise<Shot> {
    const dir = join(OUT, name);
    rmSync(dir, { recursive: true, force: true });
    mkdirSync(dir, { recursive: true });
    const shot = new Shot(page, dir);
    await shot.frame();
    return shot;
  }

  /** Seconds of video filmed so far. */
  now(): number {
    return this.frames.length / FPS;
  }

  /** Films one frame, then moves time on by one frame (`speed` frames' worth of page time). */
  async frame(speed = 1) {
    const data = await this.page.screenshot({ type: "jpeg", quality: 92 });
    if (this.last === null || !data.equals(this.last)) {
      this.lastFile = `${String(this.frames.length).padStart(5, "0")}.jpg`;
      writeFileSync(join(this.dir, this.lastFile), data);
      this.last = data;
    }
    this.frames.push({ t: this.now(), file: this.lastFile });
    await this.page.evaluate(`window.__cullaStep(${FRAME_MS * speed})`);
    await this.page.clock.runFor(FRAME_MS * speed);
  }

  /** Films until `seconds` of this shot. */
  async until(seconds: number) {
    while (this.now() < seconds - 1e-6) {
      await this.frame();
    }
  }

  /** Films while a condition is not met yet, at most `seconds` more, the page `speed` times faster. */
  async filmUntil(done: () => Promise<boolean>, seconds: number, speed = 1) {
    const end = this.now() + seconds;
    while (!(await done())) {
      if (this.now() > end) {
        throw new Error("the page did not get there in time");
      }
      await this.frame(speed);
    }
  }

  /** Presses a key (Playwright's name) and logs the keycaps and the word to show on screen. */
  async press(key: string, keys: readonly string[], label: string) {
    this.events.push({ t: this.now(), keys, label });
    await this.page.keyboard.press(key);
  }

  /**
   * Zooms so the union of `selectors` fills the view, its middle at `anchorY` of the view height
   * (no selectors: back to the whole page).
   */
  async zoom(selectors: readonly string[], maxScale: number, seconds: number, anchorY = 0.5) {
    await this.page.evaluate(
      `window.__cullaZoomPlan(${JSON.stringify(selectors)}, ${maxScale}, ${anchorY})`,
    );
    const steps = Math.max(2, Math.round(seconds * FPS));
    for (let i = 1; i <= steps; i += 1) {
      const k = i / steps;
      const e = k < 0.5 ? 4 * k * k * k : 1 - (-2 * k + 2) ** 3 / 2;
      await this.page.evaluate(`window.__cullaZoomAt(${e})`);
      await this.frame();
    }
  }

  /** Seconds since the shot began at `beats` beats into it. */
  at(beats: number): number {
    return beats * BEAT;
  }

  /** Films until `beats` beats into the shot. */
  async untilBeat(beats: number) {
    await this.until(beats * BEAT);
  }

  /** Scrolls the page smoothly to `top`. */
  async scrollTo(top: number, seconds: number) {
    const from = Number(await this.page.evaluate("scrollY"));
    const steps = Math.max(2, Math.round(seconds * FPS));
    for (let i = 1; i <= steps; i += 1) {
      const k = i / steps;
      const e = k < 0.5 ? 4 * k * k * k : 1 - (-2 * k + 2) ** 3 / 2;
      await this.page.evaluate(`window.scrollTo(0, ${from + (top - from) * e})`);
      await this.frame();
    }
  }

  end() {
    const timeline = {
      fps: FPS,
      bpm: BPM,
      duration: this.now(),
      frames: this.frames,
      events: this.events,
    };
    writeFileSync(join(this.dir, "timeline.json"), `${JSON.stringify(timeline)}\n`);
  }
}


const KEEP_ROUTE = [".card-current .pill-photo", ".action-row"];
const REVIEW_KEEP_ROUTE = [".route-label-keep", ".keep-controls .destination"];

test.use({ viewport: VIEW, deviceScaleFactor: 1, reducedMotion: "no-preference" });

test("film the 30 s demo video (docs/videos.md (a))", async ({ page }) => {
  test.setTimeout(900_000);
  const files = photoFiles();
  if (PHOTOS !== null) {
    const dir = PHOTOS;
    await page.route("**/__video-photos/*", (route) => {
      const n = Number(route.request().url().split("/").pop());
      const file = files[n % files.length];
      return file === undefined
        ? route.abort()
        : route.fulfill({ contentType: "image/jpeg", body: readFileSync(join(dir, file)) });
    });
  }
  await page.addInitScript(pageHelpers(files.length > 0));
  await page.clock.install();
  await page.goto("/");
  await page.getByRole("button", { name: "Choose a folder" }).click();
  await expect(page.locator(".action-row")).toBeVisible();
  await expect
    .poll(() => page.evaluate("document.querySelector('.card-current img').complete"))
    .toBe(true);
  await page.clock.pauseAt(Date.now() + 1000);
  const visible = (selector: string) => () => page.locator(selector).first().isVisible();

  await test.step("swipe: keep, reject, keep, keep, then where the RAW goes", async () => {
    const s = await Shot.begin(page, "swipe");
    await s.untilBeat(1);
    await s.press("ArrowRight", ["→"], "Keep.");
    await s.untilBeat(3);
    await s.press("ArrowLeft", ["←"], "Reject.");
    await s.untilBeat(5);
    await s.press("ArrowRight", ["→"], "Keep.");
    await s.untilBeat(6);
    await s.press("ArrowRight", ["→"], "Keep.");
    await s.untilBeat(8);
    await s.zoom(KEEP_ROUTE, 2.6, 1.5 * BEAT, 0.62);
    await s.untilBeat(16);
    s.end();
  });

  await test.step("review: nothing moves until you say so", async () => {
    await page.evaluate("window.__cullaZoomPlan([], 1, 0.5); window.__cullaZoomAt(1)");
    await page.clock.runFor(500);
    await page.locator(".swipe-tally .button-primary").click();
    await page.clock.runFor(2000);
    await expect(page.locator(".apply-box .button-primary")).toHaveText(/^Move \d+ photos?$/);
    await page.mouse.move(-10, -10);
    const s = await Shot.begin(page, "review");
    await s.zoom(REVIEW_KEEP_ROUTE, 2.0, 1 * BEAT, 0.16);
    await s.untilBeat(8);
    s.end();
  });

  await test.step("apply: nothing deleted", async () => {
    await page.evaluate("window.__cullaZoomPlan([], 1, 0.5); window.__cullaZoomAt(1)");
    await page.locator(".apply-box .button-primary").click();
    await page.clock.runFor(1000);
    await expect(page.locator("dialog.confirm")).toBeVisible();
    await page.mouse.move(-10, -10);
    const s = await Shot.begin(page, "apply");
    await s.untilBeat(1);
    await page.locator("dialog.confirm .button-primary").click();
    await page.mouse.move(-10, -10);
    await s.filmUntil(visible("main.apply"), 3);
    // The simulated apply plays three times faster, so it finishes within the beats.
    await s.filmUntil(
      () => page.getByRole("heading", { level: 1, name: "Apply finished" }).isVisible(),
      8,
      3,
    );
    await s.zoom(["#apply-title", "main.apply .lead"], 2.2, 0.75 * BEAT, 0.35);
    await s.untilBeat(Math.max(8, Math.ceil(s.now() / BEAT)));
    s.end();
  });
});
