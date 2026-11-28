// Films and edits the demo video in one go (docs/videos.md (a)): capture.spec.ts, then
// compose.mjs. Run it again whenever a filmed screen changes.
//
//   node docs/assets/video/render-all.mjs --photos <dir> --wall <dir> --out <dir> [--fonts <dir>]
//        [--order <file,file,...>] [--bpm 120] [--offset 0] [--name culla-demo-30s]
//        [--audio <file>] [--wall-keep-out <files>] [--wall-centre <files>] [--check] [--build]
//
// --photos  the JPEGs shown in the app (CULLA_VIDEO_PHOTOS); --order their session order
// --wall    the thumbnails of the opening wall
// --fonts   Bricolage Grotesque (.ttf, .woff or .woff2); by default the app's own
// --out     where the video goes; the frames are filmed into <out>/capture. Keep it outside the
//           repository: everything there is made from Margaux's photos.
// --build   runs `pnpm build:demo` first (the capture serves dist-demo/ with `pnpm preview:demo`,
//           or reuses a server already on port 4173)
// Last, `hash.mjs --write` records the screens it filmed (docs/videos.md, "Keeping the video
// current"): commit screens.json and pending.md with the published video. Not with --check.
// Everything else is passed on to compose.mjs. The Playwright command is `pnpm exec playwright`
// unless CULLA_VIDEO_PLAYWRIGHT names another (for example `node node_modules/playwright/cli.js`).
import { spawn } from "node:child_process";
import { mkdirSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, "../../..");
const args = process.argv.slice(2);
const arg = (name) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : undefined;
};
const need = (name) => {
  const value = arg(name);
  if (value === undefined) {
    console.error(
      "usage: node docs/assets/video/render-all.mjs --photos <dir> --wall <dir> --out <dir> [--fonts <dir>] [--order <files>] [--bpm 120] [--offset 0] [--audio <file>] [--check] [--build]",
    );
    process.exit(2);
  }
  return resolve(value);
};
const photos = need("photos");
const wall = need("wall");
const out = need("out");
if (!relative(repo, out).startsWith("..")) {
  console.error(`--out must be outside the repository (${repo}): it holds frames of real photos`);
  process.exit(2);
}
const capture = join(out, "capture");
mkdirSync(capture, { recursive: true });
const bpm = arg("bpm") ?? "120";

function run(command, argv, env = {}) {
  return new Promise((ok, fail) => {
    const p = spawn(command, argv, { cwd: repo, stdio: "inherit", env: { ...process.env, ...env } });
    p.on("exit", (code) =>
      code === 0 ? ok() : fail(new Error(`${command} ${argv.join(" ")} exited ${code}`)),
    );
  });
}

if (args.includes("--build")) {
  await run("pnpm", ["build:demo"]);
}
const [pw, ...pwArgs] = (process.env.CULLA_VIDEO_PLAYWRIGHT ?? "pnpm exec playwright").split(" ");
await run(pw, [...pwArgs, "test", "-c", "docs/assets/video/playwright.config.ts"], {
  CULLA_VIDEO_CAPTURE: capture,
  CULLA_VIDEO_PHOTOS: photos,
  CULLA_VIDEO_PHOTO_ORDER: arg("order") ?? "",
  CULLA_VIDEO_BPM: bpm,
});

// Passed on as given: --offset, --name, --audio, --check, --still.
const passOn = [];
for (let i = 0; i < args.length; i += 1) {
  const a = args[i];
  if (["--offset", "--name", "--audio", "--still", "--wall-keep-out", "--wall-centre", "--fonts"].includes(a)) {
    passOn.push(a, ["--audio", "--fonts"].includes(a) ? resolve(args[i + 1]) : args[i + 1]);
    i += 1;
  } else if (a === "--check") {
    passOn.push(a);
  }
}
await run(process.execPath, [
  join(here, "compose.mjs"),
  ...["--capture", capture, "--wall", wall, "--out", out, "--bpm", bpm],
  ...passOn,
]);
if (!args.includes("--check")) {
  await run(process.execPath, [join(here, "hash.mjs"), "--write"]);
}
