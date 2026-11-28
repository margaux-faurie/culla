import { expect, type Page, test } from "@playwright/test";
import { blockingViolations, rememberKeepMode, tabUntil, tally } from "./helpers";

const swipe = (page: Page) => page.getByRole("main", { name: "Sort photos" });
const position = (page: Page) => page.locator(".swipe-tally .strong");
const recent2026 = (page: Page) => page.getByRole("button", { name: /^2026/ });

/** Sorts the first photos of the demo folder (keep, keep, reject), then reloads the page. */
async function sortThenReload(page: Page) {
  await rememberKeepMode(page, "move");
  await page.goto("/");
  await page.getByRole("button", { name: "Choose a folder" }).click();
  await expect(swipe(page)).toBeFocused();
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("ArrowLeft");
  await expect(position(page)).toHaveText("4 / 24");
  await page.reload();
  await expect(page.getByRole("heading", { name: "Recent folders" })).toBeVisible();
}

test.describe("resume a session (US-37, proposed in SPEC §10)", () => {
  test("a recent folder in progress shows its count and resumes on the first undecided", async ({
    page,
  }) => {
    await sortThenReload(page);
    await expect(recent2026(page)).toContainText("In progress · 3 decided");
    expect(await blockingViolations(page)).toEqual([]);
    await recent2026(page).click();
    await expect(swipe(page)).toBeFocused();
    await expect(position(page)).toHaveText("4 / 24");
    await expect(tally(page, "kept")).toHaveText("2 kept");
    await expect(tally(page, "rejected")).toHaveText("1 rejected");
    await expect(page.locator(".toast")).toHaveText(
      "Picked up where you left off: 3 of 24 photos decided.",
    );
    expect(await blockingViolations(page)).toEqual([]);
    // The undo stack came back too: Ctrl+Z undoes the reject and jumps to it.
    await page.keyboard.press("Control+z");
    await expect(position(page)).toHaveText("3 / 24");
    await expect(tally(page, "rejected")).toHaveText("0 rejected");
  });

  test("Choose a folder on a folder in progress resumes it too", async ({ page }) => {
    await sortThenReload(page);
    await page.getByRole("button", { name: "Choose a folder" }).click();
    await expect(swipe(page)).toBeFocused();
    await expect(tally(page, "kept")).toHaveText("2 kept");
    // A new decision is saved on top of the resumed ones.
    await page.keyboard.press("ArrowRight");
    await page.reload();
    await expect(recent2026(page)).toContainText("In progress · 4 decided");
  });

  test("Start over, from the keyboard alone: asks first, Esc keeps everything", async ({
    page,
  }) => {
    await sortThenReload(page);
    await tabUntil(page, ".recent");
    await expect(recent2026(page)).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(swipe(page)).toBeFocused();
    await tabUntil(page, ".swipe-where button");
    const startOver = page.getByRole("button", { name: "Start over" });
    await expect(startOver).toBeFocused();
    await page.keyboard.press("Enter");
    const dialog = page.getByRole("dialog", { name: "Start this folder over?" });
    await expect(dialog).toBeVisible();
    await expect(
      dialog.getByText(
        "Your decisions on 3 photos are put aside, and every photo starts undecided. No file changes.",
      ),
    ).toBeVisible();
    await expect(dialog.getByRole("button", { name: "Keep my decisions" })).toBeFocused();
    expect(await blockingViolations(page)).toEqual([]);
    await page.keyboard.press("Escape");
    await expect(dialog).toBeHidden();
    await expect(tally(page, "kept")).toHaveText("2 kept");
    // Single keys pressed in the dialog never reach the swipe view.
    await startOver.click();
    await page.keyboard.press("ArrowRight");
    await expect(tally(page, "kept")).toHaveText("2 kept");
    await page.keyboard.press("Tab");
    await expect(dialog.getByRole("button", { name: "Start over" })).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(dialog).toBeHidden();
    await expect(swipe(page)).toBeFocused();
    await expect(position(page)).toHaveText("1 / 24");
    await expect(tally(page, "undecided")).toHaveText("24 undecided");
    await expect(page.getByRole("button", { name: "Start over" })).toHaveCount(0);
    await page.reload();
    await expect(page.getByRole("heading", { name: "Recent folders" })).toBeVisible();
    await expect(recent2026(page)).toHaveCount(0);
  });

  test("a new session has no Start over", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Choose a folder" }).click();
    await expect(swipe(page)).toBeFocused();
    await expect(page.getByRole("button", { name: "Start over" })).toHaveCount(0);
  });
});
