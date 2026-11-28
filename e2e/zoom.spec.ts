import { expect, type Page, test } from "@playwright/test";
import { blockingViolations, keydown, startSorting, tabUntil, tally } from "./helpers";

const full = (page: Page) => page.getByRole("img", { name: /, at 100 %$/ });
const position = (page: Page) => page.locator(".swipe-tally .strong");
const said = (page: Page, text: string) => page.locator("output", { hasText: text });
/** Where the full image sits: its CSS transform, `translate(x, y)`. */
async function offset(page: Page): Promise<string> {
  const style = (await full(page).getAttribute("style")) ?? "";
  return /translate\([^)]*\)/.exec(style)?.[0] ?? "";
}

test.describe("zoom to 100 % (US-39)", () => {
  test("Z zooms on the centre, the arrows pan without deciding, Z and Esc go back", async ({
    page,
  }) => {
    await startSorting(page);
    await page.keyboard.press("z");
    await expect(full(page)).toBeVisible();
    await expect(said(page, "Zoomed to 100 %")).toHaveCount(1);
    expect(await blockingViolations(page)).toEqual([]);
    const start = await offset(page);
    await page.keyboard.press("ArrowRight");
    const moved = await offset(page);
    expect(moved).not.toBe(start);
    await page.keyboard.press("ArrowLeft");
    expect(await offset(page)).toBe(start);
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowUp");
    expect(await offset(page)).toBe(start);
    // The arrows never decided anything.
    await expect(tally(page, "undecided")).toHaveText("24 undecided");
    await expect(position(page)).toHaveText("1 / 24");
    await page.keyboard.press("Escape");
    await expect(full(page)).toHaveCount(0);
    await expect(said(page, "Back to the whole photo")).toHaveCount(1);
    await page.keyboard.press("z");
    await expect(full(page)).toBeVisible();
    await page.keyboard.press("z");
    await expect(full(page)).toHaveCount(0);
    // Out of zoom, the arrows decide again.
    await page.keyboard.press("ArrowRight");
    await expect(tally(page, "kept")).toHaveText("1 kept");
  });

  test("zooms at the pointer: the corner under it stays in view", async ({ page }) => {
    await startSorting(page);
    const card = page.locator(".card-current img");
    await card.hover({ position: { x: 20, y: 20 } });
    await page.keyboard.press("z");
    await expect(full(page)).toBeVisible();
    // The left edge stays at the view's edge, and the top of the photo is in view: a zoom on the
    // centre would show the middle of the 4032 px high photo instead.
    const [x, y] = (await offset(page)).match(/-?[\d.]+/g)?.map(Number) ?? [];
    expect(x).toBe(0);
    expect(y ?? -9999).toBeGreaterThan(-400);
  });

  test("a held Space zooms until released; Space on a focused button presses it", async ({
    page,
  }) => {
    await startSorting(page);
    await page.keyboard.down("Space");
    await expect(full(page)).toBeVisible();
    // Key repeat while held changes nothing.
    await keydown(page, { key: " ", code: "Space", repeat: true });
    await expect(full(page)).toBeVisible();
    await page.keyboard.up("Space");
    await expect(full(page)).toHaveCount(0);
    await tabUntil(page, ".button-keep");
    await page.keyboard.press("Space");
    await expect(tally(page, "kept")).toHaveText("1 kept");
    await expect(full(page)).toHaveCount(0);
  });

  test("the key labelled Z zooms on AZERTY, and a held Z does not toggle again", async ({
    page,
  }) => {
    await startSorting(page);
    // AZERTY: the key labelled Z sits where QWERTY has W.
    await keydown(page, { key: "z", code: "KeyW" });
    await expect(full(page)).toBeVisible();
    await keydown(page, { key: "z", code: "KeyW", repeat: true });
    await expect(full(page)).toBeVisible();
    // The QWERTY Z position types W on AZERTY: not zoom.
    await keydown(page, { key: "w", code: "KeyZ" });
    await expect(full(page)).toBeVisible();
  });

  test("Shift+arrows browse and leave zoom; Keep while zoomed decides and leaves", async ({
    page,
  }) => {
    await startSorting(page);
    await page.keyboard.press("z");
    await expect(full(page)).toBeVisible();
    await page.keyboard.press("Shift+ArrowRight");
    await expect(full(page)).toHaveCount(0);
    await expect(position(page)).toHaveText("2 / 24");
    await page.keyboard.press("z");
    await expect(full(page)).toBeVisible();
    await page.getByRole("button", { name: "Keep" }).click();
    await expect(full(page)).toHaveCount(0);
    await expect(tally(page, "kept")).toHaveText("1 kept");
    await expect(position(page)).toHaveText("3 / 24");
  });

  test("a lone NEF says it shows the embedded preview, not the RAW", async ({ page }) => {
    await startSorting(page);
    await page.keyboard.press("Shift+ArrowRight");
    await page.keyboard.press("Shift+ArrowRight");
    await expect(page.getByText("MGF_7003 · NEF")).toBeVisible();
    await page.keyboard.press("z");
    await expect(page.getByText("Embedded preview, not the RAW")).toBeVisible();
    await page.keyboard.press("Escape");
    await page.keyboard.press("Shift+ArrowLeft");
    await page.keyboard.press("z");
    await expect(full(page)).toBeVisible();
    await expect(page.getByText("Embedded preview, not the RAW")).toHaveCount(0);
  });

  test("dragging pans the photo while zoomed", async ({ page }) => {
    await startSorting(page);
    await page.keyboard.press("z");
    await expect(full(page)).toBeVisible();
    const start = await offset(page);
    const box = await page.locator(".zoom").boundingBox();
    if (box === null) throw new Error("no zoom layer");
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width / 2 + 120, box.y + box.height / 2 + 60);
    await page.mouse.up();
    expect(await offset(page)).not.toBe(start);
    await expect(tally(page, "undecided")).toHaveText("24 undecided");
  });

  test("a full image that cannot be read leaves the preview and says so", async ({ page }) => {
    await startSorting(page, { scenario: "full-image-unreadable" });
    await page.keyboard.press("z");
    await expect(page.getByRole("alert")).toHaveText(
      "Could not read the full image. The preview stays.",
    );
    await expect(full(page)).toHaveCount(0);
    await expect(page.locator(".card-current img")).toBeVisible();
    // Sorting goes on as before.
    await page.keyboard.press("ArrowRight");
    await expect(tally(page, "kept")).toHaveText("1 kept");
  });

  test("with reduced motion nothing moves: no loading animation, an instant swap", async ({
    page,
  }) => {
    const animated = (selector: string) =>
      page.evaluate(
        `[...document.querySelectorAll(${JSON.stringify(selector)})].some((el) => { const s = getComputedStyle(el); return s.animationName !== "none" || parseFloat(s.transitionDuration) > 0; })`,
      );
    // Control: without reduced motion the loading dots do animate.
    await startSorting(page, { scenario: "full-image-slow" });
    await page.keyboard.press("z");
    await expect(page.getByText("Loading the full image")).toBeVisible();
    expect(await animated(".zoom-loading .dots span")).toBe(true);
    await page.keyboard.press("Escape");

    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.keyboard.press("z");
    await expect(page.getByText("Loading the full image")).toBeVisible();
    expect(await animated(".zoom-loading .dots span")).toBe(false);
    await expect(full(page)).toBeVisible();
    expect(await animated(".zoom, .zoom-image")).toBe(false);
  });

  test("while the full image loads the arrows decide nothing, and Esc cancels it", async ({
    page,
  }) => {
    await startSorting(page, { scenario: "full-image-slow" });
    await page.keyboard.press("z");
    await expect(page.getByText("Loading the full image")).toBeVisible();
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("ArrowLeft");
    await expect(tally(page, "undecided")).toHaveText("24 undecided");
    await expect(position(page)).toHaveText("1 / 24");
    await page.keyboard.press("Escape");
    await expect(page.getByText("Loading the full image")).toHaveCount(0);
    // The image that comes in afterwards is not shown.
    await page.waitForTimeout(1700);
    await expect(full(page)).toHaveCount(0);
  });

  test("U, F and Enter still work while zoomed; U keeps zoom on", async ({ page }) => {
    await startSorting(page);
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("Shift+ArrowLeft");
    await page.keyboard.press("z");
    await expect(full(page)).toBeVisible();
    await page.keyboard.press("u");
    await expect(tally(page, "kept")).toHaveText("0 kept");
    await expect(full(page)).toBeVisible();
    await page.keyboard.press("f");
    await expect(page.getByRole("button", { name: "Showing undecided" })).toBeVisible();
    await page.keyboard.press("Enter");
    await expect(
      page.getByRole("heading", { level: 1, name: "Check where your photos will go" }),
    ).toBeVisible();
  });

  test("Keep on a decided photo while zoomed is refused and zoom stays", async ({ page }) => {
    await startSorting(page);
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("Shift+ArrowLeft");
    await page.keyboard.press("z");
    await expect(full(page)).toBeVisible();
    await page.getByRole("button", { name: "Keep" }).click();
    await expect(page.locator(".toast")).toContainText("Press U to undo");
    await expect(full(page)).toBeVisible();
    await expect(position(page)).toHaveText("1 / 24");
  });

  test("a held zoom ends when the key list opens; zoom ends when the window is resized", async ({
    page,
  }) => {
    await startSorting(page);
    await page.keyboard.down("Space");
    await expect(full(page)).toBeVisible();
    await page.keyboard.press("?");
    await expect(page.getByRole("dialog", { name: "Keyboard shortcuts" })).toBeVisible();
    await expect(full(page)).toHaveCount(0);
    await page.keyboard.up("Space");
    await page.keyboard.press("Escape");
    await page.keyboard.press("z");
    await expect(full(page)).toBeVisible();
    await page.setViewportSize({ width: 1000, height: 700 });
    await expect(full(page)).toHaveCount(0);
  });

  test("only the main button drags", async ({ page }) => {
    await startSorting(page);
    await page.keyboard.press("z");
    await expect(full(page)).toBeVisible();
    const start = await offset(page);
    const box = await page.locator(".zoom").boundingBox();
    if (box === null) throw new Error("no zoom layer");
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.down({ button: "right" });
    await page.mouse.move(box.x + box.width / 2 + 120, box.y + box.height / 2 + 60);
    await page.mouse.up({ button: "right" });
    expect(await offset(page)).toBe(start);
  });

  test("the shortcut list names Z, Space and the arrows' pan", async ({ page }) => {
    await startSorting(page);
    await page.keyboard.press("?");
    const help = page.getByRole("dialog", { name: "Keyboard shortcuts" });
    await expect(help.getByText("Zoom to 100 % at the pointer, or back")).toBeVisible();
    await expect(help.getByText("Hold to zoom to 100 %")).toBeVisible();
    await expect(help.getByText("While zoomed: move around the photo")).toBeVisible();
    // Z in the open list does nothing behind it.
    await page.keyboard.press("z");
    await expect(full(page)).toHaveCount(0);
  });
});
