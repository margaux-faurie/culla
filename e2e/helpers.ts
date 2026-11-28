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

/** Opens the demo, picks its folder, chooses "Moved" and starts sorting, with the mouse. */
export async function startSorting(page: Page, mode: "Moved" | "Copied" = "Moved") {
  await page.goto("/");
  await page.getByRole("button", { name: "Choose a folder" }).click();
  await page.getByRole("radio", { name: mode }).check();
  await page.getByRole("button", { name: "Start sorting" }).click();
  await expect(page.getByRole("main", { name: "Sort photos" })).toBeFocused();
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
