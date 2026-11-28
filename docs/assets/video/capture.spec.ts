/**
 * Films the web demo for the demo video (docs/videos.md (a), shots 3 to 8). Not part of the e2e
 * suite: it runs only with its own config, `playwright.config.ts` in this folder.
 *
 *   pnpm build:demo
 *   pnpm exec playwright test -c docs/assets/video/playwright.config.ts
 *   node docs/assets/video/compose.mjs --capture <dir> --out <dir> --fonts <dir>
 *
 * Each shot is filmed frame by frame on a stopped clock (Playwright's `page.clock` plus every CSS
 * animation paused and stepped), one JPEG screenshot (quality 92) per 1/30 s, into
 * `<CULLA_VIDEO_CAPTURE>/shot-NN/` with a `timeline.json` listing the frames and the key presses
 * to label on screen. So the swipe exit is filmed whole however slow the machine is (Chromium's
 * screencast drops frames during it). compose.mjs turns the shots into scenes.
 *
 * The page is 1600×900 at device scale 1, so it sits 1:1 in the 1920×1080 frame under the on-screen
 * text (proposal by the agent, docs/videos.md). Zooms are drawn by the browser (a CSS transform on
 * #root), so zoomed text stays sharp. The cursor is drawn by the page, because headless Chromium
 * draws none. Only the demo's own photos appear.
 */
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect, type Page, test } from "@playwright/test";

const OUT = process.env.CULLA_VIDEO_CAPTURE ?? join(tmpdir(), "culla-video-capture");
const VIEW = { width: 1600, height: 900 } as const;

/** The demo remembers move, as after a first apply (e2e/helpers.ts `rememberKeepMode`). */
const MEMORY = JSON.stringify({
  bySource: { "src-alps": "dst-archive" },
  lastUsed: "dst-selects",
  keepMode: "move",
});

/**
 * Page helpers, as a string so this file needs no DOM types: a drawn cursor that follows the mouse,
 * the zoom (a transform on #root that makes the union of some elements fill the view), and the
 * frame step that moves every CSS animation on by one frame.
 */
const PAGE_HELPERS = `
localStorage.setItem("culla-demo:destinations", ${JSON.stringify(MEMORY)});
window.addEventListener("DOMContentLoaded", () => {
  const c = document.createElement("div");
  c.id = "video-cursor";
  c.setAttribute("aria-hidden", "true");
  c.style.cssText = "position:fixed;left:0;top:0;width:30px;height:44px;z-index:2147483647;pointer-events:none;transform:translate(-100px,-100px)";
  c.innerHTML = '<svg width="30" height="44" viewBox="0 0 20 29"><path d="M1 1 L1 23 L6.5 18 L10.5 27.5 L14 26 L10 16.8 L17.5 16.8 Z" fill="#0c0b0a" stroke="#f1e9d8" stroke-width="1.6" stroke-linejoin="round"/></svg>';
  document.body.appendChild(c);
  window.addEventListener("mousemove", (e) => { c.style.transform = "translate(" + e.clientX + "px," + e.clientY + "px)"; }, true);
});
window.__cullaZoomPlan = (selectors, maxScale, anchorY) => {
  const root = document.getElementById("root");
  const from = root.__zoom || { s: 1, x: 0, y: 0 };
  let to = { s: 1, x: 0, y: 0 };
  if (selectors.length > 0) {
    const keep = root.style.transform;
    root.style.transform = "";
    const base = root.getBoundingClientRect();
    const rects = selectors.flatMap((s) => [...document.querySelectorAll(s)].map((e) => e.getBoundingClientRect()));
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

  /** Films one frame, then moves time on by one frame. */
  async frame() {
    const data = await this.page.screenshot({ type: "jpeg", quality: 92 });
    if (this.last === null || !data.equals(this.last)) {
      this.lastFile = `${String(this.frames.length).padStart(5, "0")}.jpg`;
      writeFileSync(join(this.dir, this.lastFile), data);
      this.last = data;
    }
    this.frames.push({ t: this.now(), file: this.lastFile });
    await this.page.evaluate(`window.__cullaStep(${FRAME_MS})`);
    await this.page.clock.runFor(FRAME_MS);
  }

  /** Films until `seconds` of this shot. */
  async until(seconds: number) {
    while (this.now() < seconds - 1e-6) {
      await this.frame();
    }
  }

  /** Films while a condition is not met yet, at most `seconds` more. */
  async filmUntil(done: () => Promise<boolean>, seconds: number) {
    const end = this.now() + seconds;
    while (!(await done())) {
      if (this.now() > end) {
        throw new Error("the page did not get there in time");
      }
      await this.frame();
    }
  }

  /** Presses a key (Playwright's name) and logs the keycaps to show and their label. */
  async press(key: string, keys: readonly string[], label: string) {
    this.events.push({ t: this.now(), keys, label });
    await this.page.keyboard.press(key);
  }

  /** Moves the drawn cursor slowly (ease in and out), one frame per step. */
  async glide(x: number, y: number, seconds: number) {
    const from = { ...mouse };
    const steps = Math.max(2, Math.round(seconds * FPS));
    for (let i = 1; i <= steps; i += 1) {
      const k = i / steps;
      const e = k < 0.5 ? 2 * k * k : 1 - (-2 * k + 2) ** 2 / 2;
      mouse.x = from.x + (x - from.x) * e;
      mouse.y = from.y + (y - from.y) * e;
      await this.page.mouse.move(mouse.x, mouse.y);
      await this.frame();
    }
  }

  /** Glides to the middle of `selector`, pauses, then clicks it. */
  async clickOn(selector: string, seconds = 0.9, pause = 0.4) {
    const box = await this.page.locator(selector).first().boundingBox();
    if (box === null) {
      throw new Error(`nothing to click: ${selector}`);
    }
    await this.glide(box.x + box.width / 2, box.y + box.height / 2, seconds);
    await this.until(this.now() + pause);
    await this.page.mouse.down();
    await this.frame();
    await this.frame();
    await this.page.mouse.up();
    await this.frame();
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
    const timeline = { fps: FPS, duration: this.now(), frames: this.frames, events: this.events };
    writeFileSync(join(this.dir, "timeline.json"), `${JSON.stringify(timeline)}\n`);
  }
}

/** Where the mouse is, so moves start from there. */
const mouse = { x: 1180, y: 640 };

const KEEP = ".action-row .button-keep";
const REJECT = ".action-row .button-reject";
const ROUTE_UNDER_BUTTONS = [".action-row"];
const REVIEW_KEEP_ROUTE = [".route-label-keep", ".keep-controls"];
/** Out of the way of zoomed text: the right edge, beside the photo or the plan. */
const CURSOR_REST = { x: 1585, y: 470 };

test.use({ viewport: VIEW, deviceScaleFactor: 1, reducedMotion: "no-preference" });

test("film the demo video (docs/videos.md (a))", async ({ page }) => {
  test.setTimeout(900_000);
  await page.addInitScript(PAGE_HELPERS);
  await page.clock.install();
  await page.goto("/");
  await expect(page.getByRole("button", { name: "Choose a folder" })).toBeVisible();
  await page.mouse.move(mouse.x, mouse.y);
  await page.clock.pauseAt(Date.now() + 1000);
  const visible = (selector: string) => () => page.locator(selector).first().isVisible();

  await test.step("shot 3: choose a folder, see where every photo will go", async () => {
    const s = await Shot.begin(page, "shot-03");
    await s.until(1.6);
    await s.clickOn(".start-pick .button-primary", 1.2, 0.45);
    await s.filmUntil(visible(".action-row"), 5);
    await s.glide(CURSOR_REST.x, CURSOR_REST.y, 0.9);
    await s.until(4.8);
    await s.zoom(ROUTE_UNDER_BUTTONS, 2.6, 1.6);
    await s.until(10.4);
    await s.zoom([], 1, 1.0);
    await s.until(12);
    s.end();
  });

  await test.step("shot 4: keep or reject each photo", async () => {
    const s = await Shot.begin(page, "shot-04");
    await s.until(0.5);
    await s.clickOn(KEEP, 0.9, 0.45);
    await s.until(3.0);
    await s.clickOn(REJECT, 0.8, 0.45);
    await s.until(5.4);
    await s.clickOn(KEEP, 0.8, 0.45);
    await s.until(7.4);
    await s.clickOn(KEEP, 0.2, 0.5);
    await s.until(8.6);
    await s.glide(CURSOR_REST.x, CURSOR_REST.y, 0.9);
    await s.until(10);
    s.end();
  });

  await test.step("shot 5: the arrow keys, and where this photo would go", async () => {
    const s = await Shot.begin(page, "shot-05");
    await s.until(0.8);
    await s.press("ArrowRight", ["→"], "Keep");
    await s.until(2.3);
    await s.press("ArrowLeft", ["←"], "Reject");
    await s.until(3.8);
    await s.press("ArrowRight", ["→"], "Keep");
    await s.until(5.0);
    await s.zoom(ROUTE_UNDER_BUTTONS, 2.6, 1.3);
    await s.until(9.0);
    await s.zoom([], 1, 0.9);
    await s.until(10);
    s.end();
  });

  await test.step("shot 6: Ctrl+Z walks back", async () => {
    const s = await Shot.begin(page, "shot-06");
    await s.until(1.4);
    await s.press("Control+z", ["Ctrl", "Z"], "Undo");
    await s.until(3.6);
    await s.press("Control+z", ["Ctrl", "Z"], "Undo");
    await s.until(8);
    s.end();
  });

  await test.step("shot 7: review the whole plan", async () => {
    const s = await Shot.begin(page, "shot-07");
    await s.until(0.3);
    await s.clickOn(".swipe-tally .button-primary", 1.0, 0.4);
    await s.filmUntil(async () => {
      const text = await page.locator(".apply-box .button-primary").textContent();
      return text !== null && /^Move \d+ photos?$/.test(text);
    }, 5);
    await s.glide(CURSOR_REST.x, 850, 0.6);
    await s.until(2.8);
    await s.zoom(REVIEW_KEEP_ROUTE, 2.0, 1.3, 0.4);
    await s.until(6.4);
    await s.zoom([], 1, 0.8);
    await s.clickOn(".apply-box .button-outline", 0.6, 0.3);
    await s.filmUntil(visible("#every-file"), 2);
    const top = Number(
      await page.evaluate(
        `document.getElementById("every-file").getBoundingClientRect().top + scrollY - 140`,
      ),
    );
    await s.scrollTo(top, 1.2);
    await s.until(10);
    s.end();
  });

  await test.step("shot 8: apply, simulated in the demo", async () => {
    await page.evaluate("window.scrollTo(0, 0)");
    const s = await Shot.begin(page, "shot-08");
    await s.until(0.3);
    await s.clickOn(".apply-box .button-primary", 0.8, 0.4);
    await s.filmUntil(visible("dialog.confirm"), 2);
    await page.evaluate(
      `(() => { const d = document.querySelector("dialog.confirm"); d.style.transition = "transform 600ms ease-in-out"; d.style.transform = "scale(1.45)"; })()`,
    );
    await s.until(s.now() + 3.0);
    await page.evaluate(
      `(() => { const d = document.querySelector("dialog.confirm"); d.style.transition = "transform 300ms ease-in-out"; d.style.transform = ""; })()`,
    );
    await s.until(s.now() + 0.4);
    await s.clickOn("dialog.confirm .button-primary", 0.6, 0.4);
    await s.glide(1500, 820, 0.5);
    await s.filmUntil(
      () => page.getByRole("heading", { level: 1, name: "Apply finished" }).isVisible(),
      30,
    );
    await s.until(Math.max(10, s.now() + 2));
    s.end();
  });
});
