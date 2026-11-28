import { tmpdir } from "node:os";
import { join } from "node:path";
import { defineConfig, devices } from "@playwright/test";

// Films the demo video (docs/videos.md (a)). Kept out of the e2e run: the main config's testDir is
// ./e2e, and this config is only used when named with -c. Build the demo first: `pnpm build:demo`
// (render-all.mjs does).
//
// It serves dist-demo/ itself and never reuses a server already running, so what it films is the
// tree that was just built; render-all.mjs then certifies that tree (`hash.mjs --write`). With
// CULLA_VIDEO_BASE_URL it films that address instead, and render-all.mjs certifies nothing.
const external = process.env.CULLA_VIDEO_BASE_URL;
const baseURL = external ?? "http://127.0.0.1:4173";

export default defineConfig({
  testDir: ".",
  testMatch: "capture.spec.ts",
  // Outside the repository: a failure's screenshots would show Margaux's photos.
  outputDir: join(tmpdir(), "culla-video-test-results"),
  workers: 1,
  retries: 0,
  reporter: "list",
  use: {
    ...devices["Desktop Chrome"],
    baseURL,
    // No trace: it records with its own screencast, which would compete with the capture.
    trace: "off",
    screenshot: "off",
    video: "off",
  },
  ...(external === undefined
    ? {
        webServer: {
          command: "pnpm preview:demo",
          url: baseURL,
          reuseExistingServer: false,
          timeout: 30_000,
        },
      }
    : {}),
});
