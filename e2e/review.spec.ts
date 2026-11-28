import { expect, type Page, test } from "@playwright/test";
import { blockingViolations, startSorting, tabUntil, tally } from "./helpers";

/** Opens the demo with a scenario (`?scenario=`, demo only) and starts sorting with the mouse. */
async function startWith(page: Page, scenario: string | null, mode: "Moved" | "Copied" = "Moved") {
  if (scenario === null) {
    await startSorting(page, mode);
    return;
  }
  await page.goto(`/?scenario=${scenario}`);
  await page.getByRole("button", { name: "Choose a folder" }).click();
  await page.getByRole("radio", { name: mode }).check();
  await page.getByRole("button", { name: "Start sorting" }).click();
  await expect(page.getByRole("main", { name: "Sort photos" })).toBeFocused();
}

const heading = (page: Page) =>
  page.getByRole("heading", { level: 1, name: "Check where your photos will go" });

/** Keeps the first `keep` photos, rejects the next `reject`, then opens the review with Enter. */
async function decideAndReview(page: Page, keep: number, reject = 0) {
  for (let i = 0; i < keep; i += 1) await page.keyboard.press("ArrowRight");
  for (let i = 0; i < reject; i += 1) await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("Enter");
  await expect(heading(page)).toBeFocused();
  await expect(page.getByText("Checking")).toBeHidden();
}

const applyButton = (page: Page) =>
  page.locator(".apply-box").getByRole("button", { name: /^(Move|Apply to) \d+ photos?$|^Apply$/ });

test.describe("review screen (US-34)", () => {
  test("opens with Enter on its heading, with counts, the plan and nothing moved", async ({
    page,
  }) => {
    await startWith(page, null);
    await decideAndReview(page, 2, 1);
    await expect(
      page.getByText("Nothing has moved yet. Check the plan below, then apply."),
    ).toBeVisible();
    await expect(page.getByText("2 kept · moved")).toBeVisible();
    await expect(page.getByText("1 rejected · moved")).toBeVisible();
    await expect(page.getByText("21 undecided · stay where they are")).toBeVisible();
    await expect(page.getByText("3 other files (.mov, .txt) are left alone.")).toBeVisible();
    await expect(
      page.getByText(/^Kept photos change drive, so each copy is read back/),
    ).toBeVisible();
    await expect(applyButton(page)).toHaveText("Move 3 photos");
    await expect(applyButton(page)).toBeEnabled();
    expect(await blockingViolations(page)).toEqual([]);
  });

  test("Esc and Back to sorting return to the photo I left; single keys do nothing", async ({
    page,
  }) => {
    await startWith(page, null);
    await decideAndReview(page, 3);
    for (const key of ["u", "n", "z", "i", "f", "?", "ArrowRight", "ArrowLeft"]) {
      await page.keyboard.press(key);
    }
    await expect(heading(page)).toBeVisible();
    await expect(page.getByText("3 kept · moved")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("main", { name: "Sort photos" })).toBeFocused();
    await expect(page.locator(".swipe-tally .strong")).toHaveText("4 / 24");
    await expect(tally(page, "kept")).toHaveText("3 kept");
    await page.getByRole("button", { name: "Review", exact: true }).click();
    await page.getByRole("button", { name: "Back to sorting" }).click();
    await expect(page.locator(".swipe-tally .strong")).toHaveText("4 / 24");
  });

  test("a thumbnail goes back to its photo, and Sort them now to the first undecided", async ({
    page,
  }) => {
    await startWith(page, null);
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("ArrowLeft");
    await page.keyboard.press("f");
    await page.keyboard.press("Enter");
    await expect(heading(page)).toBeFocused();
    await page.getByRole("list", { name: "Rejected photos, 1" }).getByRole("button").click();
    await expect(page.locator(".swipe-tally .strong")).toHaveText("2 / 24");
    await expect(page.getByRole("button", { name: "Showing all" })).toBeVisible();
    await expect(page.locator(".card-current").getByText("Rejected")).toBeVisible();
    await expect(tally(page, "rejected")).toHaveText("1 rejected");
    await page.keyboard.press("Enter");
    await page.getByRole("button", { name: "Sort them now" }).click();
    await expect(page.locator(".swipe-tally .strong")).toHaveText("3 / 24");
  });

  test("lists every file with its target and suffix, and shows RAW staying when it does not follow", async ({
    page,
  }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Choose a folder" }).click();
    await page.getByRole("checkbox", { name: /RAW and edit files follow/ }).uncheck();
    await page.getByRole("radio", { name: "Moved" }).check();
    await page.getByRole("button", { name: "Start sorting" }).click();
    // Barcelona: MGF_7270 then MGF_7271, whose name is taken in Selects 2026.
    for (let i = 0; i < 4; i += 1) await page.keyboard.press("Shift+ArrowRight");
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("Enter");
    await expect(page.getByText("Checking")).toBeHidden();
    await expect(
      page.getByText(
        "1 name is taken in D:\\Photos\\Selects 2026. That photo gets _1 (MGF_7271_1). Nothing is replaced.",
      ),
    ).toBeVisible();
    const toggle = page.getByRole("button", { name: "See every file (2)" });
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-expanded", "true");
    const table = page.getByRole("table");
    await expect(
      table.getByText("D:\\Photos\\Selects 2026\\Barcelona\\MGF_7271_1.JPG"),
    ).toBeVisible();
    await expect(table.getByText("E:\\2026\\Barcelona\\MGF_7271.NEF")).toBeVisible();
    await expect(table.getByText("Stays where it is").first()).toBeVisible();
    expect(await blockingViolations(page)).toEqual([]);
  });

  test("names a write-protected card and disables Apply (P3)", async ({ page }) => {
    await startWith(page, "write-protected");
    await decideAndReview(page, 0, 1);
    await expect(page.getByRole("heading", { name: "Needs fixing first" })).toBeVisible();
    await expect(
      page.getByText(/is write-protected\. If it is a memory card, slide the lock switch/),
    ).toBeVisible();
    await expect(applyButton(page)).toBeDisabled();
    await expect(page.getByText("Apply waits until the problem above is solved.")).toBeVisible();
    expect(await blockingViolations(page)).toEqual([]);
  });

  test("names the free space missing (P5)", async ({ page }) => {
    await startWith(page, "no-space");
    await decideAndReview(page, 1);
    await expect(page.getByText(/^Not enough space on Backup drive \(D:\)/)).toBeVisible();
    await expect(applyButton(page)).toBeDisabled();
  });

  test("lists held-back photos with their reason and applies the rest", async ({ page }) => {
    await startWith(page, "held");
    await decideAndReview(page, 4);
    await expect(page.getByRole("heading", { name: "Held back" })).toBeVisible();
    await expect(
      page.getByText("MGF_7001 (kept, in E:\\2026): changed since scan, rescan."),
    ).toBeVisible();
    await expect(page.getByText(/MGF_7002 .*: already applied\./)).toBeVisible();
    await expect(applyButton(page)).toHaveText("Move 1 photo");
    expect(await blockingViolations(page)).toEqual([]);
  });

  test("shows warnings and still allows apply (P12, P15, P17)", async ({ page }) => {
    await startWith(page, "warnings");
    await decideAndReview(page, 2);
    await expect(page.getByText(/is inside a cloud-synced folder/)).toBeVisible();
    await expect(page.getByText(/open in another app/)).toBeVisible();
    await expect(page.getByText(/over 260 characters/)).toBeVisible();
    await expect(applyButton(page)).toBeEnabled();
  });

  test("asks which drive when two share the card's serial", async ({ page }) => {
    await startWith(page, "two-volumes");
    await decideAndReview(page, 1);
    await expect(applyButton(page)).toBeDisabled();
    await page.getByRole("button", { name: "Use Photos drive (E:)" }).click();
    await expect(page.getByRole("heading", { name: "Needs fixing first" })).toBeHidden();
    await expect(applyButton(page)).toBeEnabled();
  });

  test("waits for the previews, then Check again clears it (P16)", async ({ page }) => {
    await startWith(page, "previews-busy");
    await decideAndReview(page, 1);
    await expect(page.getByText(/still reading previews/)).toBeVisible();
    await page.getByRole("button", { name: "Check again" }).click();
    await expect(page.getByText(/still reading previews/)).toBeHidden();
    await expect(applyButton(page)).toBeEnabled();
  });

  test("says why Apply is disabled with nothing decided", async ({ page }) => {
    await startWith(page, null);
    await page.keyboard.press("Enter");
    await expect(
      page.getByText("Nothing to apply yet. Keep or reject some photos first."),
    ).toBeVisible();
    await expect(applyButton(page)).toBeDisabled();
  });

  test("reflows at 320 px without horizontal scrolling", async ({ page }) => {
    await page.setViewportSize({ width: 320, height: 800 });
    await startWith(page, null);
    await decideAndReview(page, 2, 1);
    const overflow = await page.evaluate(
      "document.documentElement.scrollWidth > document.documentElement.clientWidth",
    );
    expect(overflow).toBe(false);
    expect(await blockingViolations(page)).toEqual([]);
  });
});

test.describe("confirm dialog (US-35)", () => {
  test("states the counts, starts on Go back, traps focus, and Esc changes nothing", async ({
    page,
  }) => {
    await startWith(page, null);
    await decideAndReview(page, 2, 1);
    await applyButton(page).click();
    const dialog = page.getByRole("dialog", { name: "Move 3 photos?" });
    await expect(dialog).toBeVisible();
    await expect(
      dialog.getByText("2 kept photos will be moved to D:\\Photos\\Selects 2026."),
    ).toBeVisible();
    await expect(
      dialog.getByText("1 rejected photo will be moved to E:\\2026\\_rejected."),
    ).toBeVisible();
    await expect(dialog.getByText("Nothing is deleted.")).toBeVisible();
    await expect(dialog.getByText("21 undecided photos stay where they are.")).toBeVisible();
    const back = dialog.getByRole("button", { name: "Go back" });
    await expect(back).toBeFocused();
    expect(await blockingViolations(page)).toEqual([]);
    await page.keyboard.press("Tab");
    await expect(dialog.getByRole("button", { name: "Move 3 photos" })).toBeFocused();
    await page.keyboard.press("Tab");
    await expect(back).toBeFocused();
    await page.keyboard.press("Shift+Tab");
    await expect(dialog.getByRole("button", { name: "Move 3 photos" })).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(dialog).toBeHidden();
    await expect(applyButton(page)).toBeFocused();
    await expect(heading(page)).toBeVisible();
    // A stray Enter on the opened dialog closes it.
    await page.keyboard.press("Enter");
    await expect(dialog).toBeVisible();
    await page.keyboard.press("Enter");
    await expect(dialog).toBeHidden();
    await expect(heading(page)).toBeVisible();
  });

  test("Enter pressed 5 times or held on the swipe screen never starts an apply", async ({
    page,
  }) => {
    await startWith(page, null);
    await page.keyboard.press("ArrowRight");
    for (let i = 0; i < 5; i += 1) await page.keyboard.press("Enter");
    await expect(heading(page)).toBeFocused();
    await expect(page.getByRole("dialog")).toBeHidden();
    await page.keyboard.press("Escape");
    await page.keyboard.down("Enter");
    for (let i = 0; i < 10; i += 1) {
      await page
        .locator(":focus")
        .dispatchEvent("keydown", { key: "Enter", code: "Enter", repeat: true });
    }
    await page.keyboard.up("Enter");
    await expect(heading(page)).toBeFocused();
    await expect(page.getByRole("dialog")).toBeHidden();
    await expect(page.getByRole("heading", { name: "Plan confirmed" })).toBeHidden();
  });

  test("keyboard only: reach Apply, confirm, and the plan is confirmed without touching a file", async ({
    page,
  }) => {
    await startWith(page, null);
    await decideAndReview(page, 1, 1);
    await tabUntil(page, ".apply-box .button-primary");
    await expect(applyButton(page)).toBeFocused();
    await page.keyboard.press("Enter");
    const dialog = page.getByRole("dialog", { name: "Move 2 photos?" });
    await expect(dialog.getByRole("button", { name: "Go back" })).toBeFocused();
    await page.keyboard.press("Tab");
    await page.keyboard.press("Enter");
    await expect(page.getByRole("heading", { name: "Plan confirmed" })).toBeFocused();
    await expect(page.getByText(/the web demo never touches a file/)).toBeVisible();
  });

  test("a name taken since the review rebuilds the plan to confirm again (P11)", async ({
    page,
  }) => {
    await startWith(page, "taken-at-confirm");
    await decideAndReview(page, 1);
    await applyButton(page).click();
    await page.getByRole("dialog").getByRole("button", { name: "Move 1 photo" }).click();
    await expect(page.getByRole("dialog")).toBeHidden();
    await expect(
      page.getByText(
        /^MGF_7001 was taken in D:\\Photos\\Selects 2026 since the plan was made\. It now gets _1 \(MGF_7001_1\)\./,
      ),
    ).toBeVisible();
    await applyButton(page).click();
    await page.getByRole("dialog").getByRole("button", { name: "Move 1 photo" }).click();
    await expect(page.getByRole("heading", { name: "Plan confirmed" })).toBeVisible();
  });

  test("a card locked since the review stops the apply and shows why", async ({ page }) => {
    await startWith(page, "locked-at-confirm");
    await decideAndReview(page, 0, 1);
    await applyButton(page).click();
    await page.getByRole("dialog").getByRole("button", { name: "Move 1 photo" }).click();
    await expect(page.getByRole("dialog")).toBeHidden();
    await expect(page.getByRole("heading", { name: "Needs fixing first" })).toBeVisible();
    await expect(page.getByText(/lock switch/)).toBeVisible();
    await expect(applyButton(page)).toBeDisabled();
    await expect(page.getByRole("heading", { name: "Plan confirmed" })).toBeHidden();
  });
});
