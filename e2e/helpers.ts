import AxeBuilder from "@axe-core/playwright";
import { expect, type Page } from "@playwright/test";

/**
 * Waits until every running CSS animation on the page has finished, so axe sees the final state
 * and not a frame mid-fade (the toast fades in over 160 ms; at partial opacity its text fails
 * color-contrast). An animation cancelled by an unmount counts as settled.
 */
export async function animationsSettled(page: Page) {
  // A string, so the e2e tsconfig needs no DOM types; Playwright awaits the promise it gives.
  await page.evaluate("Promise.allSettled(document.getAnimations().map((a) => a.finished))");
}

/** Serious and critical axe violations once animations have settled (NFR-A11Y, testing.md §2). */
export async function blockingViolations(page: Page) {
  await animationsSettled(page);
  const results = await new AxeBuilder({ page }).analyze();
  return results.violations
    .filter((v) => v.impact === "serious" || v.impact === "critical")
    .map((v) => ({ id: v.id, impact: v.impact, targets: v.nodes.map((n) => n.target) }));
}

/** Where the demo keeps its memory of move or copy and destinations (`backend/demo.ts`). */
const MEMORY_KEY = "culla-demo:destinations";

/**
 * Makes the demo remember move or copy as if it was chosen before (the destination memory stays
 * the demo's seed). Null leaves the seed: no move or copy yet, as the first time. Written only when
 * the page has no memory yet, so a reload keeps what the test changed.
 */
export async function rememberKeepMode(page: Page, keepMode: "move" | "copy") {
  const memory = JSON.stringify({
    bySource: { "src-alps": "dst-archive" },
    lastUsed: "dst-selects",
    keepMode,
  });
  // A string, so the e2e tsconfig needs no DOM types.
  await page.addInitScript(
    `if (localStorage.getItem(${JSON.stringify(MEMORY_KEY)}) === null) localStorage.setItem(${JSON.stringify(MEMORY_KEY)}, ${JSON.stringify(memory)});`,
  );
}

export interface StartOptions {
  /** A demo scenario (`?scenario=`, demo only). */
  readonly scenario?: string | null;
  /** Move or copy as last chosen; null for the first time (nothing chosen). Default "move". */
  readonly keepMode?: "move" | "copy" | null;
}

/** Opens the demo and picks its folder with the mouse: sorting starts at once. */
export async function startSorting(page: Page, options: StartOptions = {}) {
  const keepMode = options.keepMode === undefined ? "move" : options.keepMode;
  if (keepMode !== null) {
    await rememberKeepMode(page, keepMode);
  }
  const scenario = options.scenario ?? null;
  await page.goto(scenario === null ? "/" : `/?scenario=${scenario}`);
  await page.getByRole("button", { name: "Choose a folder" }).click();
  await expect(page.getByRole("main", { name: "Sort photos" })).toBeFocused();
}

/** The review's primary button: "Move 3 photos", "Apply to 2 photos", or "Apply" while checking. */
export const applyButton = (page: Page) =>
  page.locator(".apply-box").getByRole("button", { name: /^(Move|Apply to) \d+ photos?$|^Apply$/ });

/** Chooses move or copy on the review screen and waits for the plan built with it. */
export async function chooseKeepMode(page: Page, mode: "Move" | "Copy") {
  await page.getByRole("radio", { name: mode, exact: true }).check();
  await expect(applyButton(page)).toHaveText(/^(Move|Apply to) \d+ photos?$/);
}

/** The tally line of the swipe screen, e.g. "3 kept". */
export const tally = (page: Page, what: "kept" | "rejected" | "undecided") =>
  page.locator(".swipe-tally").getByText(new RegExp(`^\\d+ ${what}$`));

/** Sends a keydown with fields `page.keyboard` cannot set (another layout's code, auto-repeat). */
export async function keydown(
  page: Page,
  init: { key: string; code: string; ctrlKey?: boolean; shiftKey?: boolean; repeat?: boolean },
) {
  await page.locator(":focus").dispatchEvent("keydown", init);
}

/** Presses Tab until the element matching `selector` has focus, at most 30 times. */
export async function tabUntil(page: Page, selector: string) {
  for (let i = 0; i < 30; i += 1) {
    if ((await page.locator(`${selector}:focus`).count()) > 0) {
      return;
    }
    await page.keyboard.press("Tab");
  }
}
