import { expect, test } from "@playwright/test";
import { blockingViolations } from "./helpers";

test.describe("download and install page (SPEC §2, docs/install.md)", () => {
  test("the demo links to it", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("link", { name: "Download and install" }).click();
    await expect(
      page.getByRole("heading", { level: 1, name: "Install Culla on Windows 11" }),
    ).toBeVisible();
  });

  test("says plainly that there is no release and no video yet", async ({ page }) => {
    await page.goto("/install");
    await expect(page.getByText("There is no release yet.")).toBeVisible();
    await expect(
      page.getByText("The demo video and the install video are not recorded yet."),
    ).toBeVisible();
    await expect(page.locator("video")).toHaveCount(0);
    await expect(page.locator("iframe")).toHaveCount(0);
    await expect(page.locator('a[href$=".exe"], a[href$=".msi"]')).toHaveCount(0);
  });

  test("shows the guide from docs/install.md", async ({ page }) => {
    await page.goto("/install");
    await expect(
      page.getByRole("heading", { level: 2, name: '3. The "Windows protected your PC" screen' }),
    ).toBeVisible();
    await expect(page.getByText("Get-FileHash .\\Culla_1.0.0_x64-setup.exe")).toBeVisible();
    await expect(page.getByText("screenshot:")).toHaveCount(0);
    await page.getByRole("link", { name: "step 3" }).click();
    await expect(page).toHaveURL(/#3-the-windows-protected-your-pc-screen$/);
  });

  test("has no serious or critical axe violations", async ({ page }) => {
    await page.goto("/install");
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
    expect(await blockingViolations(page)).toEqual([]);
  });
});
