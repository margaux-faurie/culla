import { defineConfig, devices } from "@playwright/test";

// End to end on the web demo build (docs/testing.md §2). Build it first: `pnpm build:demo`.
// The web server serves dist-demo/ on a fixed port (package.json `preview:demo`).
const port = 4173;
const baseURL = `http://127.0.0.1:${port}`;
const ci = process.env.CI !== undefined;

export default defineConfig({
  testDir: "./e2e",
  forbidOnly: ci,
  retries: ci ? 1 : 0,
  reporter: ci ? [["github"], ["html", { open: "never" }]] : "list",
  use: {
    baseURL,
    trace: "retain-on-failure",
  },
  // Chromium first; Firefox and WebKit join when the swipe flows land (M3).
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: {
    command: "pnpm preview:demo",
    url: baseURL,
    reuseExistingServer: !ci,
    timeout: 30_000,
  },
});
