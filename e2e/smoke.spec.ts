import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

test("demo start screen shows the Culla heading", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { level: 1, name: "Culla" })).toBeVisible();
});

test("demo start screen has no serious or critical axe violations", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { level: 1, name: "Culla" })).toBeVisible();
  const results = await new AxeBuilder({ page }).analyze();
  const blocking = results.violations
    .filter((v) => v.impact === "serious" || v.impact === "critical")
    .map((v) => ({ id: v.id, impact: v.impact, targets: v.nodes.map((n) => n.target) }));
  expect(blocking).toEqual([]);
});
