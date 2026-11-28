import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

test("demo start screen asks what to sort", async ({ page }) => {
  await page.goto("/");
  const heading = page.getByRole("heading", { level: 1, name: "What do you want to sort?" });
  await expect(heading).toBeVisible();
  await expect(page.getByText("Culla", { exact: true })).toBeVisible();
});

test("demo start screen has no serious or critical axe violations", async ({ page }) => {
  await page.goto("/");
  const heading = page.getByRole("heading", { level: 1, name: "What do you want to sort?" });
  await expect(heading).toBeVisible();
  const results = await new AxeBuilder({ page }).analyze();
  const blocking = results.violations
    .filter((v) => v.impact === "serious" || v.impact === "critical")
    .map((v) => ({ id: v.id, impact: v.impact, targets: v.nodes.map((n) => n.target) }));
  expect(blocking).toEqual([]);
});
