import { expect, type Page, test } from "@playwright/test";
import { blockingViolations, keydown, startSorting, tally } from "./helpers";

const details = (page: Page) => page.getByRole("region", { name: /^Photo details/ });
const position = (page: Page) => page.locator(".swipe-tally .strong");
const row = (page: Page, label: string) =>
  details(page)
    .locator(".exif-row", { has: page.locator("dt", { hasText: label }) })
    .locator("dd");

test.describe("EXIF overlay (US-40)", () => {
  test("I shows the date, camera, lens and exposure; I and Esc hide it", async ({ page }) => {
    await startSorting(page);
    await page.keyboard.press("i");
    await expect(details(page)).toBeVisible();
    await expect(details(page)).toHaveAccessibleName("Photo details · MGF_7001");
    await expect(row(page, "Taken")).toHaveText("14 Nov 2026, 09:00:00");
    await expect(row(page, "Camera")).toHaveText("NIKON Z 6_3");
    await expect(row(page, "Lens")).toHaveText("NIKKOR Z 24-70mm f/4 S");
    await expect(row(page, "Exposure")).toHaveText("1/250 s · f/4 · ISO 100 · 24 mm · 0 EV");
    await expect(page.locator("output", { hasText: "Photo details shown" })).toHaveCount(1);
    // No GPS position and no body serial, ever.
    await expect(details(page)).not.toContainText(/GPS|serial|latitude/i);
    expect(await blockingViolations(page)).toEqual([]);
    await page.keyboard.press("i");
    await expect(details(page)).toHaveCount(0);
    await page.keyboard.press("i");
    await expect(details(page)).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(details(page)).toHaveCount(0);
  });

  test("stays open from photo to photo, and decisions work as without it", async ({ page }) => {
    await startSorting(page);
    await page.keyboard.press("i");
    await expect(row(page, "Lens")).toHaveText("NIKKOR Z 24-70mm f/4 S");
    await page.keyboard.press("ArrowRight");
    await expect(tally(page, "kept")).toHaveText("1 kept");
    await expect(details(page)).toHaveAccessibleName("Photo details · MGF_7002");
    await expect(row(page, "Exposure")).toHaveText("1/500 s · f/2.8 · ISO 200 · 35 mm · −0.3 EV");
    await page.keyboard.press("Control+z");
    await expect(details(page)).toHaveAccessibleName("Photo details · MGF_7001");
    await page.keyboard.press("Shift+ArrowRight");
    await page.keyboard.press("Shift+ArrowRight");
    // MGF_7003, a lone NEF: its lens named from LensSpecification.
    await expect(row(page, "Lens")).toHaveText("24-200mm f/4-6.3");
    // The overlay never covers the Keep and Reject buttons.
    const box = await details(page).boundingBox();
    const keep = await page.getByRole("button", { name: "Keep" }).boundingBox();
    expect(box !== null && keep !== null && box.y + box.height <= keep.y).toBe(true);
  });

  test("a time from the file says so, and a missing lens says Not recorded", async ({ page }) => {
    await startSorting(page);
    await page.keyboard.press("i");
    for (let i = 0; i < 3; i += 1) await page.keyboard.press("Shift+ArrowRight");
    await expect(details(page)).toHaveAccessibleName("Photo details · MGF_7004");
    await expect(row(page, "Taken")).toHaveText("14 Nov 2026, 09:01:51, from the file time");
    await expect(row(page, "Lens")).toHaveText("Not recorded");
  });

  test("shows loading dots until the EXIF pass reaches the photo", async ({ page }) => {
    await startSorting(page, { scenario: "exif-pending" });
    await page.keyboard.press("i");
    await expect(details(page).getByText("Reading the photo details")).toBeVisible();
    await expect(row(page, "Camera")).toHaveText("NIKON Z 6_3");
  });

  test("Esc leaves zoom first, then hides the details; I in the review does nothing", async ({
    page,
  }) => {
    await startSorting(page);
    await keydown(page, { key: "i", code: "KeyI" });
    await page.keyboard.press("z");
    await expect(page.getByRole("img", { name: /, at 100 %$/ })).toBeVisible();
    await expect(details(page)).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("img", { name: /, at 100 %$/ })).toHaveCount(0);
    await expect(details(page)).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(details(page)).toHaveCount(0);
    await expect(position(page)).toHaveText("1 / 24");
    await page.keyboard.press("Enter");
    await page.keyboard.press("i");
    await expect(details(page)).toHaveCount(0);
  });

  test("the shortcut list and the hints name I", async ({ page }) => {
    await startSorting(page);
    await expect(page.locator(".hints")).toContainText("details");
    await page.keyboard.press("?");
    await expect(
      page.getByRole("dialog").getByText("Photo details: date, camera, lens, exposure"),
    ).toBeVisible();
  });
});
