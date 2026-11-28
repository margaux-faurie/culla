import { expect, type Page, test } from "@playwright/test";
import {
  applyButton,
  blockingViolations,
  branch,
  openFold,
  rememberKeepMode,
  startSorting,
  tabUntil,
} from "./helpers";

/** Opens the demo with a scenario (`?scenario=`, demo only), move remembered, and starts sorting. */
const startWith = (page: Page, scenario: string | null) => startSorting(page, { scenario });

/** Keeps the first `keep` photos, rejects the next `reject`, reviews and confirms with the mouse. */
async function applyWith(page: Page, scenario: string | null, keep: number, reject = 0) {
  await startWith(page, scenario);
  for (let i = 0; i < keep; i += 1) await page.keyboard.press("ArrowRight");
  for (let i = 0; i < reject; i += 1) await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("Enter");
  await expect(page.getByText("Checking")).toBeHidden();
  const label = `Move ${keep + reject} ${keep + reject === 1 ? "photo" : "photos"}`;
  await page.locator(".apply-box").getByRole("button", { name: label }).click();
  await page.getByRole("dialog").getByRole("button", { name: label }).click();
  await expect(page.locator("main.apply")).toBeVisible();
}

const title = (page: Page) => page.locator("#apply-title");
const pauseButton = (page: Page) =>
  page.locator(".apply-controls").getByRole("button", { name: /^(Pause|Pausing|Continue)$/ });
const stopButton = (page: Page) =>
  page
    .locator(".apply-controls")
    .getByRole("button", { name: /^(Stop after this file|Stopping|Stop here)$/ });
const report = (page: Page) => page.locator(".report");

test.describe("apply screen (US-36)", () => {
  test("shows progress per file, then the report, the log and Done", async ({ page }) => {
    await applyWith(page, "slow", 2, 1);
    await expect(title(page)).toBeFocused();
    await expect(page.getByRole("heading", { level: 1, name: "Moving your photos" })).toBeVisible();
    await expect(
      page.getByText(/^\d of 3 photos done\. You can keep using your computer\.$/),
    ).toBeVisible();
    await expect(page.getByRole("progressbar", { name: "Apply progress" })).toBeVisible();
    const now = page.getByRole("region", { name: "Now" });
    await expect(now.getByText("MGF_7001 · JPEG and NEF")).toBeVisible();
    await expect(now.getByText("Reading the copy back and checking it matches")).toBeVisible();
    await expect(now.getByText("Then the original leaves E:\\2026")).toBeVisible();
    await expect(page.getByRole("list", { name: "Where your photos will go" })).toBeVisible();
    expect(await blockingViolations(page)).toEqual([]);

    await expect(page.getByRole("heading", { level: 1, name: "Apply finished" })).toBeFocused({
      timeout: 30_000,
    });
    await expect(page.locator(".lead")).toHaveText("3 of 3 photos done. Nothing was deleted.");
    await expect(report(page).getByRole("list", { name: "Summary" })).toHaveText("3 photos done");
    await page.getByRole("button", { name: "See them (3)" }).click();
    await expect(
      page.getByText("MGF_7001.NEF: moved to D:\\Photos\\Selects 2026\\MGF_7001.NEF, checked"),
    ).toBeVisible();
    await expect(page.getByRole("button", { name: /^Retry failed/ })).toHaveCount(0);
    expect(await blockingViolations(page)).toEqual([]);

    const openLog = page.getByRole("button", { name: "Open log" });
    await openLog.click();
    const dialog = page.getByRole("dialog", { name: "Apply log" });
    await expect(dialog.getByRole("button", { name: "Close" })).toBeFocused();
    await expect(dialog.getByLabel("Log text")).toContainText("Culla apply log");
    await expect(dialog.getByLabel("Log text")).toContainText("(apply, first run)");
    await expect(dialog.getByLabel("Log text")).toContainText("end          finished");
    expect(await blockingViolations(page)).toEqual([]);
    await page.keyboard.press("Escape");
    await expect(dialog).toBeHidden();
    await expect(openLog).toBeFocused();

    await page.getByRole("button", { name: "Done" }).click();
    await expect(page.getByRole("button", { name: "Choose a folder" })).toBeVisible();
  });

  test("pause is acknowledged at once, taken after the current file, and Continue carries on", async ({
    page,
  }) => {
    await applyWith(page, "slow", 2);
    await expect(page.getByRole("region", { name: "Now" })).toContainText("MGF_7001.JPG");
    await pauseButton(page).click();
    // At once, before the current file has finished.
    await expect(pauseButton(page)).toHaveText("Pausing");
    await expect(pauseButton(page)).toHaveAttribute("aria-disabled", "true");
    await expect(page.locator(".apply-halt")).toHaveText(/^Pausing after this file\./);
    await expect(page.getByRole("region", { name: "Now" })).toBeVisible();
    // Then paused, with nothing half-done: no file is being worked on.
    await expect(title(page)).toHaveText("Paused", { timeout: 15_000 });
    await expect(page.getByRole("region", { name: "Now" })).toBeHidden();
    await expect(page.locator(".apply-alert")).toHaveText(
      "Paused. Nothing is half-done. Continue when you are ready.",
    );
    await expect(pauseButton(page)).toHaveText("Continue");
    expect(await blockingViolations(page)).toEqual([]);
    await pauseButton(page).click();
    await expect(title(page)).toHaveText("Moving your photos");
    await expect(page.getByRole("heading", { level: 1, name: "Apply finished" })).toBeVisible({
      timeout: 30_000,
    });
  });

  test("stop after this file leaves a cut group copied, source kept; Finish moves and Apply remaining", async ({
    page,
  }) => {
    await applyWith(page, "slow", 2);
    await expect(page.getByRole("region", { name: "Now" })).toContainText("MGF_7001.JPG");
    await stopButton(page).click();
    await expect(stopButton(page)).toHaveText("Stopping");
    await expect(page.locator(".apply-halt")).toHaveText(/^Stopping after this file\./);
    await expect(page.getByRole("heading", { level: 1, name: "Apply stopped" })).toBeFocused({
      timeout: 15_000,
    });
    await expect(report(page).getByRole("list", { name: "Summary" })).toHaveText(
      "1 photo copied, source kept1 photo not started",
    );
    const kept = page.getByRole("region", { name: "Copied, source kept" });
    await expect(kept.getByText(/^MGF_7001\.JPG: copied, source kept\./)).toBeVisible();
    await expect(kept.getByText(/^MGF_7001\.NEF: the apply stopped before it\./)).toBeVisible();
    await expect(page.getByRole("region", { name: "Not started" })).toContainText("MGF_7002");
    expect(await blockingViolations(page)).toEqual([]);

    await page.getByRole("button", { name: "Finish moves (1)" }).click();
    await expect(page.getByRole("region", { name: "Now" })).toContainText(/again, from scratch/);
    await expect(page.getByRole("heading", { level: 1, name: "Apply finished" })).toBeFocused({
      timeout: 30_000,
    });
    await expect(report(page).getByRole("list", { name: "Summary" })).toHaveText(
      "1 photo not started1 photo done",
    );
    await page.getByRole("button", { name: "Apply remaining (1)" }).click();
    await expect(
      page.getByRole("heading", { level: 1, name: "Check where your photos will go" }),
    ).toBeFocused();
    await expect(branch(page, "2 kept")).toBeVisible();
    await expect(page.getByText(/^1 photo is held back\./)).toBeVisible();
    await openFold(page, /^What the check found/);
    await expect(
      page.getByText(/^MGF_7001 \(kept, in E:\\2026\): already applied\.$/),
    ).toBeVisible();
    await expect(applyButton(page)).toHaveText("Move 1 photo");
  });

  test("a stop in a reject leaves the photo split between folders", async ({ page }) => {
    await applyWith(page, "slow", 0, 1);
    await stopButton(page).click();
    await expect(page.getByRole("heading", { level: 1, name: "Apply stopped" })).toBeVisible({
      timeout: 15_000,
    });
    const split = page.getByRole("region", { name: "Split between folders" });
    await expect(
      split.getByText("MGF_7001.JPG: moved to E:\\2026\\_rejected\\MGF_7001.JPG"),
    ).toBeVisible();
    await expect(
      split.getByText("MGF_7001.NEF: not started, still at E:\\2026\\MGF_7001.NEF"),
    ).toBeVisible();
  });

  test("a card pulled mid-apply pauses until it is back, then continues on Continue", async ({
    page,
  }) => {
    await applyWith(page, "card-pulled", 1);
    await expect(title(page)).toHaveText("Paused");
    await expect(page.locator(".apply-alert")).toHaveText(
      /^Photos drive \(E:\) is not connected\. Connect it again/,
    );
    await expect(pauseButton(page)).toHaveAttribute("aria-disabled", "true");
    // The engine cannot stop while the card is out: Stop says why and does nothing.
    await expect(stopButton(page)).toHaveAttribute("aria-disabled", "true");
    await expect(stopButton(page)).toHaveAccessibleDescription(
      /^Connect Photos drive \(E:\) first/,
    );
    expect(await blockingViolations(page)).toEqual([]);
    await expect(page.locator(".apply-alert")).toHaveText(/^Photos drive \(E:\) is back\./);
    await expect(stopButton(page)).toHaveAttribute("aria-disabled", "false");
    // It does not carry on by itself.
    await expect(title(page)).toHaveText("Paused");
    await pauseButton(page).click();
    await expect(page.getByRole("heading", { level: 1, name: "Apply finished" })).toBeVisible({
      timeout: 15_000,
    });
    await page.getByRole("button", { name: "Open log" }).click();
    await expect(page.getByLabel("Log text")).toContainText("(apply, resume)");
    await expect(page.getByLabel("Log text")).toContainText("attempts 2");
  });

  test("a full destination pauses and says so", async ({ page }) => {
    await applyWith(page, "disk-full", 1);
    await expect(title(page)).toHaveText("Paused");
    await expect(page.locator(".apply-alert")).toHaveText(
      /^Backup drive \(D:\) is full\. Free some space/,
    );
    await pauseButton(page).click();
    await expect(page.getByRole("heading", { level: 1, name: "Apply finished" })).toBeVisible({
      timeout: 15_000,
    });
  });

  test("a lost file is an alarm, first, with the chkdsk advice and nothing offered for it", async ({
    page,
  }) => {
    await applyWith(page, "lost", 0, 1);
    await expect(title(page)).toHaveText("Paused");
    await expect(page.locator(".apply-alert")).toHaveText(/is back/);
    await pauseButton(page).click();
    await expect(
      page.getByRole("heading", { level: 1, name: "Apply finished, with photos to check" }),
    ).toBeVisible({ timeout: 15_000 });
    const first = report(page).locator(".report-section").first();
    await expect(first.getByRole("heading", { name: "Run chkdsk first" })).toBeVisible();
    await expect(first.getByText(/Run chkdsk on Photos drive \(E:\)/)).toBeVisible();
    await expect(
      first.getByText(/^MGF_7001\.JPG: it is in neither place Culla expected/),
    ).toBeVisible();
    await expect(page.getByRole("button", { name: /^Retry failed/ })).toHaveCount(0);
    expect(await blockingViolations(page)).toEqual([]);
  });

  test("a copy that does not match is shown first and can be retried", async ({ page }) => {
    await applyWith(page, "verify-mismatch", 1);
    await expect(page.getByRole("heading", { name: "A copy did not match" })).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.getByText(/card reader or the card/).first()).toBeVisible();
    await page.getByRole("button", { name: "Retry failed (1)" }).click();
    await expect(page.getByRole("heading", { level: 1, name: "Apply finished" })).toBeVisible({
      timeout: 15_000,
    });
    await expect(report(page).getByRole("list", { name: "Summary" })).toHaveText("1 photo done");
  });

  test("Esc does not stop or pause the apply", async ({ page }) => {
    await applyWith(page, "slow", 1);
    await page.keyboard.press("Escape");
    await page.keyboard.press("Escape");
    await expect(title(page)).toHaveText("Moving your photos");
    await expect(pauseButton(page)).toHaveText("Pause");
  });

  test("keyboard only: pause, continue, stop and close the report", async ({ page }) => {
    await rememberKeepMode(page, "move");
    await page.goto("/?scenario=slow");
    // Tab only once the screen is drawn, or it lands nowhere.
    await expect(page.getByRole("button", { name: "Choose a folder" })).toBeVisible();
    await page.keyboard.press("Tab");
    await expect(page.getByRole("button", { name: "Choose a folder" })).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(page.getByRole("main", { name: "Sort photos" })).toBeFocused();
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("Enter");
    await expect(page.getByText("Checking")).toBeHidden();
    await tabUntil(page, ".apply-box .button-primary");
    await page.keyboard.press("Enter");
    await page.keyboard.press("Tab");
    await page.keyboard.press("Enter");
    await expect(title(page)).toBeFocused();

    await tabUntil(page, ".apply-controls .button");
    await expect(pauseButton(page)).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(title(page)).toHaveText("Paused", { timeout: 15_000 });
    // Pause became Continue on the same button, which keeps the focus.
    await expect(pauseButton(page)).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(title(page)).toHaveText("Moving your photos");
    await page.keyboard.press("Tab");
    await expect(stopButton(page)).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(page.getByRole("heading", { level: 1, name: "Apply stopped" })).toBeFocused({
      timeout: 15_000,
    });
    await tabUntil(page, ".report-actions .button-primary");
    await expect(page.getByRole("button", { name: "Done" })).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(page.getByRole("button", { name: "Choose a folder" })).toBeVisible();
  });

  test("works with reduced motion: the bar jumps instead of easing", async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    await applyWith(page, "slow", 1);
    await expect(page.locator(".apply-bar-fill")).toHaveCSS("transition-duration", "0s");
    await expect(page.getByRole("heading", { level: 1, name: "Apply finished" })).toBeVisible({
      timeout: 30_000,
    });
  });

  test("reflows at 320 px without horizontal scrolling", async ({ page }) => {
    await page.setViewportSize({ width: 320, height: 800 });
    await applyWith(page, "slow", 1);
    await expect(page.getByRole("region", { name: "Now" })).toBeVisible();
    const overflow = await page.evaluate(
      "document.documentElement.scrollWidth - document.documentElement.clientWidth",
    );
    expect(overflow).toBeLessThanOrEqual(0);
  });
});

test.describe("an apply interrupted at launch (US-38)", () => {
  test("says so first, rolls forward on its own, reports, then shows the start", async ({
    page,
  }) => {
    await page.goto("/?scenario=interrupted");
    await expect(
      page.getByRole("heading", { level: 1, name: "An apply was interrupted" }),
    ).toBeFocused();
    await expect(
      page.getByText(
        "An apply was interrupted. Each photo is either in its original place or checked at its new one. Culla will tell you about anything that needs attention. Culla is finishing it.",
      ),
    ).toBeVisible();
    await expect(pauseButton(page)).toBeVisible();
    expect(await blockingViolations(page)).toEqual([]);
    await expect(page.getByRole("heading", { level: 1, name: "Apply finished" })).toBeFocused({
      timeout: 30_000,
    });
    await page.getByRole("button", { name: "Open log" }).click();
    await expect(page.getByLabel("Log text")).toContainText("(apply, resume)");
    await page.getByRole("button", { name: "Close" }).click();
    await expect(page.getByRole("button", { name: "Apply remaining" })).toHaveCount(0);
    await page.getByRole("button", { name: "Done" }).click();
    await expect(page.getByRole("button", { name: "Choose a folder" })).toBeVisible();
  });

  test("waits for the card and touches nothing until it is back", async ({ page }) => {
    await page.goto("/?scenario=interrupted-card-out");
    await expect(
      page.getByText(
        "Connect Photos drive (E:) to finish it. Culla touches nothing until it is back.",
      ),
    ).toBeVisible();
    await expect(page.getByRole("progressbar")).toHaveCount(0);
    await expect(pauseButton(page)).toHaveCount(0);
    await expect(page.getByRole("heading", { level: 1, name: "Apply finished" })).toBeVisible({
      timeout: 30_000,
    });
  });
});
