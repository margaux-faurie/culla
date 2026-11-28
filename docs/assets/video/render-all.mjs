// Films and edits the demo video in one go (docs/videos.md (a)): capture.spec.ts, then
// compose.mjs. Run it again whenever a filmed screen changes.
//
//   node docs/assets/video/render-all.mjs --photos <dir> --wall <dir> --out <dir> [--fonts <dir>]
//        [--order <file,file,...>] [--bpm 120] [--offset 0] [--name culla-demo-30s]
//        [--audio <file>] [--wall-keep-out <files>] [--wall-centre <files>] [--check] [--no-build]
//
// --photos  the JPEGs shown in the app (CULLA_VIDEO_PHOTOS); --order their session order
// --wall    the thumbnails of the opening wall
// --fonts   Bricolage Grotesque (.ttf, .woff or .woff2); by default the app's own
// --out     where the video goes; the frames are filmed into <out>/capture. --photos, --wall and
//           --out must be outside the repository (symlinks resolved; another drive is fine), or it
//           stops: everything there is Margaux's or made from her photos (outside.mjs).
// It first builds the demo (`pnpm build:demo`; --no-build skips it), and the capture serves that
// build itself (`pnpm preview:demo`, never a server already running).
// Last, `hash.mjs --write` certifies that the screens of the working tree that was just built
// and filmed are in the video (docs/videos.md, "Keeping the video current"): render from a clean
// tree at the commit you publish, publish the files, then commit them with screens.json and
// pending.md. It is skipped with --check, with --no-build (the build may be older than the tree)
// and with CULLA_VIDEO_BASE_URL (another server was filmed); run it by hand once you are sure.
// Everything else is passed on to compose.mjs. The Playwright command is `pnpm exec playwright`
// unless CULLA_VIDEO_PLAYWRIGHT names another (for example `node node_modules/playwright/cli.js`).
import { spawn } from "node:child_process";
import { mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { assertOutside, assertPlainName, REPO } from "./outside.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const repo = REPO;
const args = process.argv.slice(2);
const arg = (name) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : undefined;
};
const need = (name) => {
  const value = arg(name);
  if (value === undefined) {
    console.error(
      "usage: node docs/assets/video/render-all.mjs --photos <dir> --wall <dir> --out <dir> [--fonts <dir>] [--order <files>] [--bpm 120] [--offset 0] [--audio <file>] [--check] [--no-build]",
    );
    process.exit(2);
  }
  return resolve(value);
};
let photos;
let wall;
let out;
try {
  photos = assertOutside(need("photos"), "--photos");
  wall = assertOutside(need("wall"), "--wall");
  out = assertOutside(need("out"), "--out");
  if (arg("name") !== undefined) {
    assertPlainName(arg("name"), "--name");
  }
  if (arg("audio") !== undefined) {
    assertOutside(arg("audio"), "--audio");
  }
} catch (error) {
  console.error(error.message);
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

const built = !args.includes("--no-build");
if (built) {
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
if (args.includes("--check")) {
  // Check frames only: nothing to certify.
} else if (!built || process.env.CULLA_VIDEO_BASE_URL !== undefined) {
  console.log(
    "Not recorded in screens.json: the demo filmed was not built from this tree here. Once sure it was, run node docs/assets/video/hash.mjs --write",
  );
} else {
  await run(process.execPath, [join(here, "hash.mjs"), "--write"]);
}
