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

  test("says plainly that there is no release, and shows the demo video", async ({ page }) => {
    await page.goto("/install");
    await expect(page.getByText("There is no release yet.")).toBeVisible();
    await expect(page.getByText("The install video is not recorded yet.")).toBeVisible();
    // One self-hosted player, no autoplay, with English and French caption tracks.
    const video = page.locator("video");
    await expect(video).toHaveCount(1);
    await expect(video).not.toHaveAttribute("autoplay", /.*/);
    await expect(video.locator("source")).toHaveAttribute("src", "/videos/culla-demo.mp4");
    await expect(video.locator("track")).toHaveCount(2);
    await expect(page.locator("iframe")).toHaveCount(0);
    // The video has no sound: a transcript of its on-screen text sits under it, closed.
    const transcript = page.locator("details", { has: page.getByText("Transcript") });
    await expect(transcript).toHaveCount(1);
    await expect(transcript).not.toHaveAttribute("open", /.*/);
    await transcript.getByText("Transcript").click();
    await expect(transcript.getByRole("listitem").first()).toHaveText("600 photos. One evening.");
    await expect(transcript.getByRole("listitem")).toHaveCount(9);
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
