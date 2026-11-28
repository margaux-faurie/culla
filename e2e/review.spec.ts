import { expect, type Page, test } from "@playwright/test";
import {
  applyButton,
  blockingViolations,
  chooseKeepMode,
  startSorting,
  tabUntil,
  tally,
} from "./helpers";

/** Opens the demo with a scenario (`?scenario=`, demo only), move remembered, and starts sorting. */
const startWith = (page: Page, scenario: string | null) => startSorting(page, { scenario });

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

/** The review's own status region (the dialog has its own). */
const reviewStatus = (page: Page) => page.locator("main.review > [role=status]");

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
    await startWith(page, null);
    // Barcelona: MGF_7270 then MGF_7271, whose name is taken in Selects 2026.
    for (let i = 0; i < 4; i += 1) await page.keyboard.press("Shift+ArrowRight");
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("Enter");
    await expect(page.getByText("Checking")).toBeHidden();
    // RAW follows is an option of the review, folded by default.
    await page.getByText("Options", { exact: true }).click();
    await page.getByRole("checkbox", { name: /RAW and edit files follow/ }).uncheck();
    await expect(page.getByRole("button", { name: "See every file (2)" })).toBeVisible();
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
    await expect(reviewStatus(page)).toHaveText(
      "Using Photos drive (E:). Checked again. Nothing stops the apply.",
    );
  });

  test("waits for the previews, then Check again clears it (P16)", async ({ page }) => {
    await startWith(page, "previews-busy");
    await decideAndReview(page, 1);
    await expect(page.getByText(/still reading previews/)).toBeVisible();
    await page.getByRole("button", { name: "Check again" }).click();
    await expect(page.getByText(/still reading previews/)).toBeHidden();
    await expect(applyButton(page)).toBeEnabled();
    await expect(reviewStatus(page)).toHaveText("Checked again. Nothing stops the apply.");
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
    // The file table may scroll on its own; it takes focus so the keyboard can scroll it.
    await page.getByRole("button", { name: /^See every file/ }).click();
    const files = page.getByRole("region", { name: /^Every file/ });
    await expect(files).toBeVisible();
    await files.focus();
    await expect(files).toBeFocused();
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

  test("a dialog the browser closed on its own opens again from Apply", async ({ page }) => {
    await startWith(page, null);
    await decideAndReview(page, 1);
    await applyButton(page).click();
    const dialog = page.getByRole("dialog", { name: "Move 1 photo?" });
    await expect(dialog).toBeVisible();
    // What Chromium does on a second Esc: it closes the modal even though cancel was prevented.
    await page.evaluate("document.querySelector('dialog.confirm').close()");
    await expect(dialog).toBeHidden();
    await expect(applyButton(page)).toBeFocused();
    await applyButton(page).click();
    await expect(dialog).toBeVisible();
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
    await expect(page.locator("main.apply")).toBeHidden();
  });

  test("keyboard only: reach Apply, confirm, and the apply screen takes over", async ({ page }) => {
    await startWith(page, null);
    await decideAndReview(page, 1, 1);
    await tabUntil(page, ".apply-box .button-primary");
    await expect(applyButton(page)).toBeFocused();
    await page.keyboard.press("Enter");
    const dialog = page.getByRole("dialog", { name: "Move 2 photos?" });
    await expect(dialog.getByRole("button", { name: "Go back" })).toBeFocused();
    await page.keyboard.press("Tab");
    await page.keyboard.press("Enter");
    // The apply screen takes over with its heading focused, and the simulated apply runs.
    await expect(page.locator("#apply-title")).toBeFocused();
    await expect(page.getByRole("heading", { level: 1, name: "Apply finished" })).toBeVisible();
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
    await expect(page.locator("main.apply")).toBeVisible();
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
    await expect(page.locator("main.apply")).toBeHidden();
  });
});

test.describe("move or copy and the destination on review (decided by Margaux 28 Nov 2026)", () => {
  test("the first time nothing is chosen: Apply waits for move or copy", async ({ page }) => {
    await startSorting(page, { keepMode: null });
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("ArrowLeft");
    await page.keyboard.press("Enter");
    await expect(heading(page)).toBeFocused();
    const move = page.getByRole("radio", { name: "Move", exact: true });
    const copy = page.getByRole("radio", { name: "Copy", exact: true });
    await expect(move).not.toBeChecked();
    await expect(copy).not.toBeChecked();
    // The destination used last is proposed, labelled as such.
    const route = page.getByRole("list", { name: "Where your photos will go" });
    await expect(route.getByText("D:\\Photos\\Selects 2026", { exact: true })).toBeVisible();
    await expect(route.getByText("same as last time")).toBeVisible();
    await expect(applyButton(page)).toBeDisabled();
    await expect(page.getByText("Choose move or copy for the kept photos first.")).toBeVisible();
    // No plan yet, but the decided photos are listed.
    await expect(page.getByRole("list", { name: "Rejected photos, 1" })).toBeVisible();
    await expect(
      page.getByText("Culla checks the plan once the kept photos have somewhere to go."),
    ).toBeVisible();
    expect(await blockingViolations(page)).toEqual([]);
    await chooseKeepMode(page, "Copy");
    await expect(copy).toBeChecked();
    await expect(applyButton(page)).toHaveText("Apply to 2 photos");
    await expect(applyButton(page)).toBeEnabled();
    await expect(page.getByText("1 kept · copied")).toBeVisible();
    await chooseKeepMode(page, "Move");
    await expect(applyButton(page)).toHaveText("Move 2 photos");
  });

  test("with no destination yet, the user chooses one on review", async ({ page }) => {
    await page.addInitScript(
      `localStorage.setItem("culla-demo:destinations", '{"bySource":{},"lastUsed":null,"keepMode":null}')`,
    );
    await startSorting(page, { keepMode: null });
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("Enter");
    await expect(
      page.getByText("Choose move or copy and a destination for the kept photos first."),
    ).toBeVisible();
    await expect(applyButton(page)).toBeDisabled();
    await page.getByRole("radio", { name: "Move", exact: true }).check();
    await expect(page.getByText("Choose a destination for the kept photos first.")).toBeVisible();
    await page.getByRole("button", { name: "Choose a destination" }).click();
    await expect(page.getByText("D:\\Photos\\Selects 2026", { exact: true })).toBeVisible();
    await expect(page.getByText("same as last time")).toBeHidden();
    await expect(applyButton(page)).toHaveText("Move 1 photo");
    await expect(applyButton(page)).toBeEnabled();
  });

  test("rejects alone apply with nothing chosen for kept photos", async ({ page }) => {
    await startSorting(page, { keepMode: null });
    await page.keyboard.press("ArrowLeft");
    await page.keyboard.press("Enter");
    await expect(applyButton(page)).toHaveText("Move 1 photo");
    await expect(applyButton(page)).toBeEnabled();
  });

  test("a remembered destination on a missing drive is not available until changed", async ({
    page,
  }) => {
    await page.goto("/");
    await page.getByRole("button", { name: /^Alps/ }).click();
    await expect(page.getByRole("main", { name: "Sort photos" })).toBeFocused();
    await expect(page.getByText("You choose where on Review")).toBeVisible();
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("Enter");
    await expect(page.getByText("not available")).toBeVisible();
    await page.getByRole("radio", { name: "Move", exact: true }).check();
    await expect(
      page.getByText(
        "The destination's drive is not connected. Connect it, or choose another destination.",
      ),
    ).toBeVisible();
    await expect(applyButton(page)).toBeDisabled();
    expect(await blockingViolations(page)).toEqual([]);
    await page.getByRole("button", { name: "Change" }).click();
    await expect(page.getByText("not available")).toBeHidden();
    await expect(applyButton(page)).toHaveText("Move 1 photo");
    await expect(applyButton(page)).toBeEnabled();
  });

  test("move or copy and the destination are kept for next time", async ({ page }) => {
    await startSorting(page, { keepMode: null });
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("Enter");
    await chooseKeepMode(page, "Copy");
    await page.reload();
    await page.getByRole("button", { name: "Choose a folder" }).click();
    await expect(page.getByRole("main", { name: "Sort photos" })).toBeFocused();
    await expect(
      page.getByText("copied to D:\\Photos\\Selects 2026", { exact: true }),
    ).toBeVisible();
    await page.keyboard.press("Enter");
    await expect(page.getByRole("radio", { name: "Copy", exact: true })).toBeChecked();
    await expect(page.getByText("same as last time")).toBeVisible();
  });

  test("move or copy is chosen from the keyboard alone", async ({ page }) => {
    await startSorting(page, { keepMode: null });
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("Enter");
    await expect(heading(page)).toBeFocused();
    await tabUntil(page, 'input[name="keep-mode"][value="move"]');
    await page.keyboard.press("Space");
    await expect(page.getByRole("radio", { name: "Move", exact: true })).toBeChecked();
    await expect(applyButton(page)).toHaveText("Move 1 photo");
    await tabUntil(page, ".apply-box .button-primary");
    await expect(applyButton(page)).toBeFocused();
  });
});
