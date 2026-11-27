import { expect, test } from "@playwright/test";
import { blockingViolations, tabUntil } from "./helpers";

test.describe("start screen (US-21, US-44)", () => {
  test("nothing is preselected and Start stays disabled until moved or copied is chosen", async ({
    page,
  }) => {
    await page.goto("/");
    const start = page.getByRole("button", { name: "Start sorting" });
    await expect(start).toBeDisabled();
    await page.getByRole("button", { name: "Choose a folder" }).click();

    await expect(page.getByRole("radio", { name: "Moved" })).not.toBeChecked();
    await expect(page.getByRole("radio", { name: "Copied" })).not.toBeChecked();
    await expect(start).toBeDisabled();
    await expect(page.getByText("Choose moved or copied first")).toBeVisible();

    await page.getByRole("radio", { name: "Copied" }).check();
    await expect(start).toBeEnabled();
  });

  test("the route shows where kept, rejected and undecided photos go", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Choose a folder" }).click();
    const route = page.getByRole("list", { name: "Where your photos will go" });
    await expect(route.getByText("E:\\2026", { exact: true })).toBeVisible();
    await expect(
      route.locator(".route-place").filter({ hasText: "Moved to E:\\2026\\_rejected" }),
    ).toBeVisible();
    await expect(route.getByText("new folder")).toBeVisible();
    await expect(route.getByText("Nothing is deleted.", { exact: false })).toBeVisible();
    await expect(route.getByText("Stay exactly where they are.")).toBeVisible();
    // The proposed destination is the last one used, labelled as such.
    await expect(route.getByText("D:\\Photos\\Selects 2026", { exact: true })).toBeVisible();
    await expect(route.getByText("same as last time")).toBeVisible();
    // One real example per branch, with its JPEG, NEF and edit file.
    const examples = route.locator(".route-example");
    await expect(examples).toHaveCount(2);
    await expect(examples.first()).toContainText("E:\\2026\\Barcelona\\MGF_7273 .JPG .NEF .xmp");
    await expect(examples.nth(1)).toContainText("moved to E:\\2026\\_rejected\\Barcelona");

    await page.getByRole("radio", { name: "Copied" }).check();
    await expect(examples.first()).toContainText("copied to D:\\Photos\\Selects 2026\\Barcelona");
  });

  test("with RAW not following, the example shows the NEF and its sidecar staying", async ({
    page,
  }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Choose a folder" }).click();
    await page.getByRole("checkbox", { name: "RAW and edit files follow their JPEG" }).uncheck();
    const example = page.locator(".route-example").first();
    await expect(example).toContainText("MGF_7273 .JPG →");
    await expect(example).toContainText("MGF_7273 .NEF .xmp stay in E:\\2026\\Barcelona");
  });

  test("leaving subfolders out changes the count and the example", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Choose a folder" }).click();
    await expect(page.getByText("24 photos, also in Barcelona and Lyon")).toBeVisible();
    await page.getByRole("checkbox", { name: "Include subfolders" }).uncheck();
    await expect(page.getByText("4 photos · subfolders left out")).toBeVisible();
    await expect(page.locator(".route-example").first()).toContainText("E:\\2026\\MGF_7002");
  });

  test("a remembered destination on a missing drive is not available", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: /^Alps/ }).click();
    await expect(page.getByText("not available")).toBeVisible();
    await page.getByRole("radio", { name: "Moved" }).check();
    await expect(page.getByRole("button", { name: "Start sorting" })).toBeDisabled();
    await page.getByRole("button", { name: "Change" }).click();
    await expect(page.getByText("D:\\Photos\\Selects 2026", { exact: true })).toBeVisible();
    await expect(page.getByRole("button", { name: "Start sorting" })).toBeEnabled();
  });

  test("a folder whose drive is not connected cannot be started", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: /^100NZ6_3/ }).click();
    await expect(page.getByText("Connect this folder's drive to sort it")).toBeVisible();
    await expect(page.getByRole("button", { name: "Start sorting" })).toBeDisabled();
  });

  test("the destination is remembered for the folder next time", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Choose a folder" }).click();
    await page.getByRole("radio", { name: "Moved" }).check();
    await page.getByRole("button", { name: "Start sorting" }).click();
    await expect(page.getByRole("main", { name: "Sort photos" })).toBeVisible();
    await page.reload();
    await page.getByRole("button", { name: "Choose a folder" }).click();
    await expect(page.getByText("same as last time")).toBeVisible();
    // Move or copy is chosen fresh each session.
    await expect(page.getByRole("radio", { name: "Moved" })).not.toBeChecked();
  });

  test("works from the keyboard alone", async ({ page }) => {
    await page.goto("/");
    await page.keyboard.press("Tab");
    await expect(page.getByRole("button", { name: "Choose a folder" })).toBeFocused();
    await page.keyboard.press("Enter");
    const moved = page.getByRole("radio", { name: "Moved" });
    await expect(moved).toBeVisible();
    await tabUntil(page, 'input[name="keep-mode"][value="move"]');
    await expect(moved).toBeFocused();
    await page.keyboard.press("Space");
    await expect(moved).toBeChecked();
    const start = page.getByRole("button", { name: "Start sorting" });
    await tabUntil(page, ".button-large");
    await expect(start).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(page.getByRole("main", { name: "Sort photos" })).toBeFocused();
  });

  test("reflows at 320 px without horizontal scrolling", async ({ page }) => {
    await page.setViewportSize({ width: 320, height: 800 });
    await page.goto("/");
    await page.getByRole("button", { name: "Choose a folder" }).click();
    await expect(page.getByText("same as last time")).toBeVisible();
    const overflow = await page.evaluate<number>(
      "document.documentElement.scrollWidth - document.documentElement.clientWidth",
    );
    expect(overflow).toBeLessThanOrEqual(0);
  });

  test("has no serious or critical axe violations once a folder is chosen", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Choose a folder" }).click();
    await expect(page.getByText("same as last time")).toBeVisible();
    expect(await blockingViolations(page)).toEqual([]);
    await page.getByRole("button", { name: /^Alps/ }).click();
    await expect(page.getByText("not available")).toBeVisible();
    expect(await blockingViolations(page)).toEqual([]);
  });
});
