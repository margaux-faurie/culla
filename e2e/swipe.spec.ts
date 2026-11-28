import { expect, test } from "@playwright/test";
import { blockingViolations, keydown, startSorting, tally } from "./helpers";

test.describe("swipe screen (US-22 to US-32, US-42)", () => {
  test("keeps with → and rejects with ←, then shows the next photo", async ({ page }) => {
    await startSorting(page);
    const position = page.locator(".swipe-tally .strong");
    await expect(position).toHaveText("1 / 24");
    await page.keyboard.press("ArrowRight");
    await expect(tally(page, "kept")).toHaveText("1 kept");
    await expect(position).toHaveText("2 / 24");
    await page.keyboard.press("ArrowLeft");
    await expect(tally(page, "rejected")).toHaveText("1 rejected");
    await expect(tally(page, "undecided")).toHaveText("22 undecided");
    await expect(page.getByRole("status").filter({ hasText: "Rejected, 2 of 24" })).toBeAttached();
  });

  test("shows the photo through an opaque id and where it would go", async ({ page }) => {
    await startSorting(page);
    await expect(page.locator(".card-current img")).toHaveAttribute("alt", /.+/);
    await expect(page.locator(".card-current").getByText("Undecided")).toBeVisible();
    await expect(page.getByText("moved to E:\\2026\\_rejected", { exact: true })).toBeVisible();
    const keepRoute = page.getByText("moved to D:\\Photos\\Selects 2026", { exact: true });
    await expect(keepRoute).toBeVisible();
    await expect(page.getByText("No files have moved yet")).toBeVisible();
  });

  test("before move or copy is chosen, Keep says so; with no destination, where comes later", async ({
    page,
  }) => {
    await startSorting(page, { keepMode: null });
    await expect(
      page.getByText("moved or copied to D:\\Photos\\Selects 2026", { exact: true }),
    ).toBeVisible();
    await page.evaluate(
      `localStorage.setItem("culla-demo:destinations", '{"bySource":{},"lastUsed":null,"keepMode":null}')`,
    );
    await page.reload();
    await page.getByRole("button", { name: "Choose a folder" }).click();
    await expect(page.getByText("You choose where on Review", { exact: true })).toBeVisible();
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("Shift+ArrowLeft");
    await expect(page.getByText("Kept: you choose where on Review.")).toBeVisible();
    await page.keyboard.press("ArrowRight");
    await expect(page.locator(".toast")).toContainText(
      "Already kept. You choose where on Review. Press U to undo.",
    );
    expect(await blockingViolations(page)).toEqual([]);
  });

  test("an arrow on a decided photo shakes, explains and changes nothing (US-26)", async ({
    page,
  }) => {
    await startSorting(page);
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("Shift+ArrowLeft");
    await expect(page.locator(".card-current").getByText("Kept")).toBeVisible();
    await page.keyboard.press("ArrowLeft");
    await expect(page.locator(".toast")).toContainText("Already kept");
    await expect(page.locator(".toast")).toContainText("Press U to undo");
    await expect(page.locator(".card-current")).toHaveClass(/card-shake/);
    await expect(tally(page, "kept")).toHaveText("1 kept");
    await expect(tally(page, "rejected")).toHaveText("0 rejected");
    // The buttons follow the same rule (ADR 0006).
    await page.getByRole("button", { name: "Reject", exact: true }).click();
    await expect(tally(page, "rejected")).toHaveText("0 rejected");
    await expect(page.locator(".toast")).toContainText("Press U to undo");
  });

  test("U, Ctrl+Z and Ctrl+Shift+Z undo and redo, and jump to the photo (US-27)", async ({
    page,
  }) => {
    await startSorting(page);
    const position = page.locator(".swipe-tally .strong");
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("ArrowRight");
    await expect(position).toHaveText("4 / 24");
    await page.keyboard.press("Control+z");
    await expect(position).toHaveText("3 / 24");
    await expect(tally(page, "kept")).toHaveText("2 kept");
    await page.keyboard.press("Control+Shift+Z");
    await expect(tally(page, "kept")).toHaveText("3 kept");
    await expect(position).toHaveText("3 / 24");
    await page.keyboard.press("u");
    await expect(tally(page, "kept")).toHaveText("2 kept");
    await page.keyboard.press("ArrowLeft");
    await expect(tally(page, "rejected")).toHaveText("1 rejected");
  });

  test("Ctrl+Z stays on the key labelled Z on AZERTY (US-22)", async ({ page }) => {
    await startSorting(page);
    await page.keyboard.press("ArrowRight");
    await expect(tally(page, "kept")).toHaveText("1 kept");
    // On AZERTY the key labelled Z sits at the QWERTY W position.
    await keydown(page, { key: "z", code: "KeyW", ctrlKey: true });
    await expect(tally(page, "kept")).toHaveText("0 kept");
  });

  test("a held arrow decides one photo (US-23)", async ({ page }) => {
    await startSorting(page);
    await keydown(page, { key: "ArrowRight", code: "ArrowRight" });
    for (let i = 0; i < 10; i += 1) {
      await keydown(page, { key: "ArrowRight", code: "ArrowRight", repeat: true });
    }
    await expect(tally(page, "kept")).toHaveText("1 kept");
  });

  test("fast decisions are all recorded with at most 3 cards mounted (US-24)", async ({ page }) => {
    await startSorting(page);
    for (let i = 0; i < 5; i += 1) {
      await page.keyboard.press("ArrowRight");
      expect(await page.locator(".stage figure").count()).toBeLessThanOrEqual(3);
    }
    await expect(tally(page, "kept")).toHaveText("5 kept");
  });

  test("browses with Shift+arrows, N and F without deciding (US-28)", async ({ page }) => {
    await startSorting(page);
    const position = page.locator(".swipe-tally .strong");
    await page.keyboard.press("Shift+ArrowRight");
    await page.keyboard.press("Shift+ArrowRight");
    await expect(position).toHaveText("3 / 24");
    await page.keyboard.press("Shift+ArrowLeft");
    await expect(position).toHaveText("2 / 24");
    await expect(tally(page, "undecided")).toHaveText("24 undecided");
    await page.keyboard.press("Shift+ArrowLeft");
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("Shift+ArrowLeft");
    await page.keyboard.press("n");
    await expect(position).toHaveText("2 / 24");
    await page.keyboard.press("f");
    await expect(page.getByRole("button", { name: "Showing undecided" })).toBeVisible();
    await page.keyboard.press("f");
    await expect(page.getByRole("button", { name: "Showing kept" })).toBeVisible();
  });

  test("the end of the list offers the undecided photos and the review (US-29)", async ({
    page,
  }) => {
    await startSorting(page);
    await page.keyboard.press("ArrowRight");
    for (let i = 0; i < 23; i += 1) {
      await page.keyboard.press("Shift+ArrowRight");
    }
    await expect(page.getByRole("heading", { name: "End of the list" })).toBeVisible();
    await page.getByRole("button", { name: "Jump to undecided (23)" }).click();
    await expect(page.locator(".swipe-tally .strong")).toHaveText("2 / 24");
    // The clicked button is gone with the end card; the shortcuts still work.
    await expect(page.getByRole("main", { name: "Sort photos" })).toBeFocused();
    await page.keyboard.press("ArrowRight");
    await expect(tally(page, "kept")).toHaveText("2 kept");
  });

  test("the shortcuts still work after clicking Undo, which then goes away", async ({ page }) => {
    await startSorting(page);
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("Shift+ArrowLeft");
    await page.getByRole("button", { name: /^Undo/ }).click();
    await expect(tally(page, "kept")).toHaveText("0 kept");
    await expect(page.getByRole("button", { name: /^Undo/ })).toHaveCount(0);
    await page.keyboard.press("ArrowRight");
    await expect(tally(page, "kept")).toHaveText("1 kept");
  });

  test("a held Enter on the focused Keep button keeps one photo", async ({ page }) => {
    await startSorting(page);
    const keep = page.getByRole("button", { name: "Keep", exact: true });
    await keep.focus();
    // Repeated keyboard.down calls without keyboard.up are sent with repeat set (auto-repeat).
    await page.keyboard.down("Enter");
    for (let i = 0; i < 5; i += 1) {
      await page.keyboard.down("Enter");
    }
    await page.keyboard.up("Enter");
    await expect(tally(page, "kept")).toHaveText("1 kept");
    await expect(page.locator(".swipe-tally .strong")).toHaveText("2 / 24");
  });

  test("with RAW not following, the note says what stays, before and after deciding", async ({
    page,
  }) => {
    await startSorting(page);
    // RAW follows is set on the review screen, in its Options.
    await page.keyboard.press("Enter");
    await page.getByText("Options", { exact: true }).click();
    await page.getByRole("checkbox", { name: "RAW and edit files follow their JPEG" }).uncheck();
    await expect(page.getByRole("checkbox", { name: /RAW and edit files/ })).not.toBeChecked();
    await page.getByRole("button", { name: "Back to sorting" }).click();
    const note = page.getByText(
      "RAW and edit files do not follow: MGF_7001 .NEF stays in E:\\2026.",
    );
    await expect(note).toBeVisible();
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("Shift+ArrowLeft");
    await expect(page.getByText("Kept: will be moved to D:\\Photos\\Selects 2026.")).toBeVisible();
    await expect(note).toBeVisible();
  });

  test("decides 10 photos with clicks only (US-42, WCAG 2.5.7)", async ({ page }) => {
    await startSorting(page);
    const keep = page.getByRole("button", { name: "Keep", exact: true });
    const reject = page.getByRole("button", { name: "Reject", exact: true });
    for (let i = 0; i < 5; i += 1) {
      await keep.click();
      await reject.click();
    }
    await expect(tally(page, "kept")).toHaveText("5 kept");
    await expect(tally(page, "rejected")).toHaveText("5 rejected");
    for (const button of [keep, reject]) {
      const box = await button.boundingBox();
      expect(box?.width ?? 0).toBeGreaterThanOrEqual(24);
      expect(box?.height ?? 0).toBeGreaterThanOrEqual(24);
    }
  });

  test("? lists the shortcuts; Esc closes the list and focus goes back (US-31)", async ({
    page,
  }) => {
    await startSorting(page);
    await page.keyboard.press("?");
    const dialog = page.getByRole("dialog", { name: "Keyboard shortcuts" });
    await expect(dialog).toBeVisible();
    // Single-key shortcuts do nothing while focus is outside the swipe view (NFR-A11Y-10).
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("u");
    await expect(tally(page, "kept")).toHaveText("0 kept");
    expect(await blockingViolations(page)).toEqual([]);
    await page.keyboard.press("Escape");
    await expect(dialog).toBeHidden();
    await expect(page.getByRole("main", { name: "Sort photos" })).toBeFocused();
  });

  test("Enter opens the review and Esc comes back", async ({ page }) => {
    await startSorting(page);
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("Enter");
    const heading = page.getByRole("heading", {
      level: 1,
      name: "Check where your photos will go",
    });
    await expect(heading).toBeFocused();
    await expect(page.getByText("1 kept · moved")).toBeVisible();
    await expect(page.getByText("23 undecided · stay where they are")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("main", { name: "Sort photos" })).toBeFocused();
    await expect(tally(page, "kept")).toHaveText("1 kept");
  });

  test("works with reduced motion: no shake, the toast still shows (US-32)", async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    await startSorting(page);
    await page.keyboard.press("ArrowRight");
    await expect(page.locator(".card-exit")).toHaveCSS("animation-name", "fade-out");
    await page.keyboard.press("Shift+ArrowLeft");
    await page.keyboard.press("ArrowRight");
    await expect(page.locator(".toast")).toContainText("Press U to undo");
    await expect(page.locator(".card-current")).toHaveCSS("animation-name", "cue");
    await expect(tally(page, "kept")).toHaveText("1 kept");
  });

  test("has no serious or critical axe violations, undecided and decided", async ({ page }) => {
    await startSorting(page);
    expect(await blockingViolations(page)).toEqual([]);
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("Shift+ArrowLeft");
    await page.keyboard.press("ArrowRight");
    const toast = page.locator(".toast");
    await expect(toast).toBeVisible();
    // axe checks the toast at its final state, not mid fade-in (blockingViolations waits too).
    await expect(toast).toHaveCSS("opacity", "1");
    expect(await blockingViolations(page)).toEqual([]);
  });
});
