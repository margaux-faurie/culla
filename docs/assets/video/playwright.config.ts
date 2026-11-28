import { defineConfig, devices } from "@playwright/test";

// Films the demo video (docs/videos.md (a)). Kept out of the e2e run: the main config's testDir is
// ./e2e, and this config is only used when named with -c. Build the demo first: `pnpm build:demo`.
const port = 4173;
const baseURL = process.env.CULLA_VIDEO_BASE_URL ?? `http://127.0.0.1:${port}`;

export default defineConfig({
  testDir: ".",
  testMatch: "capture.spec.ts",
  outputDir: "../../../test-results/video",
  workers: 1,
  retries: 0,
  reporter: "list",
  use: {
    ...devices["Desktop Chrome"],
    baseURL,
    // No trace: it records with its own screencast, which would compete with the capture.
    trace: "off",
  },
  webServer: {
    command: "pnpm preview:demo",
    url: baseURL,
    reuseExistingServer: true,
    timeout: 30_000,
  },
});
