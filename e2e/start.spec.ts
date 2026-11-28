import { expect, type Page, test } from "@playwright/test";
import { blockingViolations, tally } from "./helpers";

const swipe = (page: Page) => page.getByRole("main", { name: "Sort photos" });

test.describe("start screen (US-21, US-44; decided by Margaux 28 Nov 2026)", () => {
  test("asks only what to sort: no route, no options shown, no Start button", async ({ page }) => {
    await page.goto("/");
    await expect(
      page.getByRole("heading", { level: 1, name: "What do you want to sort?" }),
    ).toBeVisible();
    await expect(page.getByRole("button", { name: "Choose a folder" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Recent folders" })).toBeVisible();
    await expect(page.getByRole("list", { name: "Where your photos will go" })).toBeHidden();
    await expect(page.getByRole("button", { name: "Start sorting" })).toHaveCount(0);
    await expect(page.getByRole("radio")).toHaveCount(0);
    // Include subfolders is folded away, on by default.
    await expect(page.getByRole("checkbox", { name: "Include subfolders" })).toBeHidden();
    expect(await blockingViolations(page)).toEqual([]);
  });

  test("choosing a folder starts sorting at once, with its subfolders", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Choose a folder" }).click();
    await expect(swipe(page)).toBeFocused();
    await expect(page.locator(".swipe-tally .strong")).toHaveText("1 / 24");
    await expect(tally(page, "undecided")).toHaveText("24 undecided");
  });

  test("Options leaves subfolders out before sorting starts", async ({ page }) => {
    await page.goto("/");
    await page.getByText("Options", { exact: true }).click();
    const subfolders = page.getByRole("checkbox", { name: "Include subfolders" });
    await expect(subfolders).toBeChecked();
    await subfolders.uncheck();
    expect(await blockingViolations(page)).toEqual([]);
    await page.getByRole("button", { name: "Choose a folder" }).click();
    await expect(swipe(page)).toBeFocused();
    await expect(page.locator(".swipe-tally .strong")).toHaveText("1 / 4");
  });

  test("a connected recent folder starts sorting at once", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: /^Alps/ }).click();
    await expect(swipe(page)).toBeFocused();
  });

  test("a recent folder whose drive is not connected says so and stays", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: /^100NZ6_3/ }).click();
    await expect(page.getByRole("alert")).toHaveText("Connect this folder's drive to sort it.");
    await expect(swipe(page)).toHaveCount(0);
    expect(await blockingViolations(page)).toEqual([]);
  });

  test("works from the keyboard alone", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByRole("button", { name: "Choose a folder" })).toBeVisible();
    await page.keyboard.press("Tab");
    await expect(page.getByRole("button", { name: "Choose a folder" })).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(swipe(page)).toBeFocused();
  });

  test("reflows at 320 px without horizontal scrolling", async ({ page }) => {
    await page.setViewportSize({ width: 320, height: 800 });
    await page.goto("/");
    await page.getByText("Options", { exact: true }).click();
    await expect(page.getByRole("checkbox", { name: "Include subfolders" })).toBeVisible();
    const overflow = await page.evaluate<number>(
      "document.documentElement.scrollWidth - document.documentElement.clientWidth",
    );
    expect(overflow).toBeLessThanOrEqual(0);
  });
});
