// Edits the 30 s demo video (docs/videos.md (a)) from the shots capture.spec.ts filmed.
//
//   node docs/assets/video/compose.mjs --capture <dir> --wall <dir> --fonts <dir> --out <dir>
//        [--bpm 120] [--offset 0] [--name culla-demo-30s] [--check] [--still <scene>:<seconds>]...
//        [--audio <file>]
//   node docs/assets/video/compose.mjs --master <mp4> --audio <file> --out <dir>
//
// --capture  the folder capture.spec.ts wrote (CULLA_VIDEO_CAPTURE): swipe, review, apply
// --wall     a folder of small landscape thumbnails (e.g. 180×120 JPEGs, no people, no metadata)
//            for the opening wall; repeated and shuffled to fill it. Kept outside the repository.
// --fonts    a folder holding Bricolage Grotesque (OFL) as .ttf or .woff2. Not in the repository.
// --bpm, --offset  the music's tempo and its first downbeat in seconds: every cut and every word
//            lands on a beat. Film again with the same CULLA_VIDEO_BPM so the decisions do too.
// --check    only draws the check frames
// --audio    also writes culla-demo-30s.music.mp4: the silent master with the track trimmed to
//            the video, -16 LUFS (loudnorm, two passes), faded in over 1 s and out over 2.5 s,
//            AAC stereo, video copied. With --master, only that, on an existing master.
//
// Nothing here or in its output may be committed when it holds Margaux's photos, the wall's
// thumbnails or the music
// (docs/videos.md): keep --capture and --out outside the repository.
//
// Every picture is drawn by Chromium at 1920×1080 and sent to ffmpeg as JPEG (quality 92); one
// H.264 file per scene (30 fps, -preset veryfast, CRF 18, no audio), written to a .tmp.mp4 name
// and renamed when ffmpeg exits 0, then joined with the concat demuxer and -c copy. Hard cuts on
// beats; words come in with 150 ms eased scale or slide.
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { chromium } from "@playwright/test";

const args = process.argv.slice(2);
const arg = (name) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : undefined;
};
const path = (name) => (arg(name) ? resolve(arg(name)) : null);
const CAPTURE = path("capture");
const WALL = path("wall");
const FONTS = path("fonts");
const OUT = path("out");
const MASTER = path("master");
const AUDIO = path("audio");
const CHECK = args.includes("--check");
const BPM = Number(arg("bpm") ?? "120");
const OFFSET = Number(arg("offset") ?? "0");
const FFMPEG = process.env.FFMPEG ?? "ffmpeg";
const FPS = 30;
const W = 1920;
const H = 1080;
const BEAT = 60 / BPM;
const NAME = arg("name") ?? "culla-demo-30s";

/** Seconds from the start of the video to beat `b` (beat 0 is the first downbeat). */
const T = (b) => OFFSET + b * BEAT;

const shot = (name) => {
  const dir = join(CAPTURE, name);
  const timeline = JSON.parse(readFileSync(join(dir, "timeline.json"), "utf8"));
  if (Math.abs((timeline.bpm ?? 120) - BPM) > 1e-6) {
    console.warn(`${name} was filmed at ${timeline.bpm} BPM, the edit is at ${BPM}: film it again`);
  }
  return { dir, ...timeline };
};

/** The wall: COLS × ROWS tiles of 180×120 with 8 px gaps, thumbnails shuffled (seeded). */
const COLS = 34;
const ROWS = 24;
function wallTiles() {
  const files = readdirSync(WALL).filter((f) => /\.(jpe?g|png|webp)$/i.test(f)).sort();
  if (files.length === 0) {
    throw new Error(`no thumbnails in ${WALL}`);
  }
  let seed = 7;
  const rand = () => {
    seed = (seed * 1103515245 + 12345) % 2147483648;
    return seed / 2147483648;
  };
  const tiles = [];
  while (tiles.length < COLS * ROWS) {
    const round = [...files];
    for (let i = round.length - 1; i > 0; i -= 1) {
      const j = Math.floor(rand() * (i + 1));
      [round[i], round[j]] = [round[j], round[i]];
    }
    tiles.push(...round);
  }
  return tiles.slice(0, COLS * ROWS).map((f) => pathToFileURL(join(WALL, f)).href);
}

/**
 * The cut list, in beats (Margaux's storyboard of 28 Nov 2026, opener changed by her the same
 * day): the thumbnail wall, the problem, the swipe with real photos, review, apply, end card.
 * 60 beats: 30 s at 120 BPM.
 */
function scenes() {
  const swipe = shot("swipe");
  const review = shot("review");
  const apply = shot("apply");
  // Each word stays until the next one, at most a beat and a half.
  const words = swipe.events.map((e, i) => {
    const next = swipe.events[i + 1];
    return {
      at: e.t,
      until: Math.min(e.t + 1.5 * BEAT, next === undefined ? 99 : next.t),
      text: e.label,
      side: e.keys[0] === "←" ? "left" : "right",
      style: "word",
    };
  });
  return [
    {
      // Tight on a few tiles, a fast pull-back to hundreds of photos, then a slow drift.
      id: "1-wall",
      from: 0,
      to: T(10),
      wall: { tiles: wallTiles(), hold: 1 * BEAT, pull: 3 * BEAT, from: 1.5, to: 0.44 },
      words: [
        { at: T(4), text: "600 photos.", style: "slam-center", line: 0 },
        { at: T(6), text: "One evening.", style: "slam-center", line: 1 },
      ],
    },
    {
      id: "3-painful",
      from: T(10),
      to: T(16),
      words: [
        { at: 0, text: "Sorting them?", style: "big", line: 0 },
        { at: 2 * BEAT, text: "Painful.", style: "big", line: 1 },
      ],
    },
    {
      id: "4-swipe",
      from: T(16),
      to: T(32),
      shot: swipe,
      words: [...words, { at: 9.5 * BEAT, until: 99, text: "The RAW follows.", style: "panel", top: true }],
    },
    {
      id: "5-review",
      from: T(32),
      to: T(40),
      shot: review,
      words: [{ at: 0.5 * BEAT, until: 99, text: "Nothing moves until you say so.", style: "panel" }],
    },
    {
      id: "6-apply",
      from: T(40),
      to: T(48),
      shot: apply,
      words: [
        { at: 2 * BEAT, until: 99, text: "Nothing deleted. Ever.", style: "panel", orange: true },
        // Honesty rule (docs/videos.md): the demo's apply is a simulation.
        { at: 0, until: 99, text: "Web demo: the apply is simulated", style: "note", settled: true },
      ],
    },
    {
      id: "7-end",
      from: T(48),
      to: T(60),
      end: true,
      words: [
        { at: 0, text: "Culla", style: "wordmark" },
        { at: 2 * BEAT, text: "Free. Open source. Windows 11.", style: "end-line" },
        { at: 4 * BEAT, text: "github.com/margaux-faurie/culla", style: "end-url" },
      ],
    },
  ];
}

/** Frames of a scene: from its first to its last frame on the 30 fps grid of the whole video. */
const sceneFrames = (scene) => Math.round(scene.to * FPS) - Math.round(scene.from * FPS);

/** The frame of a shot on screen at `t` (the last one filmed at or before it). */
function frameAt(sh, t) {
  let file = sh.frames[0].file;
  for (const f of sh.frames) {
    if (f.t > t + 1e-6) break;
    file = f.file;
  }
  return pathToFileURL(join(sh.dir, file)).href;
}

/** What to draw at `t` (seconds into the scene): plain data, also the key for reusing a frame. */
function stateAt(scene, t) {
  const r3 = (x) => Math.round(x * 1000) / 1000;
  const st = { id: scene.id };
  if (scene.wall) {
    const w = scene.wall;
    const k = Math.min(1, Math.max(0, (t - w.hold) / w.pull));
    const e = k < 0.5 ? 4 * k * k * k : 1 - (-2 * k + 2) ** 3 / 2;
    // Scale eases from tight to the whole wall; a slow drift and turn run all along.
    st.wall = {
      s: r3(w.from * (w.to / w.from) ** e),
      x: r3(-14 * t),
      y: r3(-6 * t),
      r: r3(-2.5 + 0.25 * t),
    };
  }
  if (scene.shot) {
    st.src = frameAt(scene.shot, t);
  }
  st.end = scene.end === true;
  st.words = (scene.words ?? [])
    .filter((w) => t >= w.at - 1e-6 && t < (w.until ?? 99))
    .map((w) => ({ ...w, k: w.settled ? 1 : r3(Math.min(1, (t - w.at) / 0.15)) }));
  return st;
}

function fontFaces() {
  const files = readdirSync(FONTS).filter((f) => /bricolage.*\.(ttf|woff2)$/i.test(f));
  if (files.length === 0) {
    throw new Error(`no Bricolage Grotesque .ttf or .woff2 in ${FONTS}`);
  }
  return files
    .map((f) => {
      const weight = /bold/i.test(f) ? "700" : /\[|variable/i.test(f) ? "200 800" : "400";
      const url = pathToFileURL(join(FONTS, f)).href;
      return `@font-face { font-family: "Bricolage"; src: url("${url}"); font-weight: ${weight}; }`;
    })
    .join("\n");
}

/** The drawing page: Night, ink, one orange line; no gradients or shadows. */
function composeHtml() {
  return `<!doctype html><html><head><meta charset="utf-8"><style>
${fontFaces()}
:root { --paper: #0c0b0a; --ink: #f1e9d8; --muted: #a8a092; --orange: #ffb38a; }
* { box-sizing: border-box; margin: 0; }
html, body { width: ${W}px; height: ${H}px; background: var(--paper); color: var(--ink); overflow: hidden;
  font-family: "Bricolage", "DejaVu Sans", sans-serif; }
.full { position: absolute; inset: 0; width: ${W}px; height: ${H}px; object-fit: cover; }
.layer { position: absolute; inset: 0; }
.slam, .big, .word, .panel { position: absolute; font-weight: 700; letter-spacing: -0.03em; line-height: 1; white-space: nowrap; }
.slam { left: 120px; font-size: 150px; padding: 18px 36px 26px; background: var(--paper); border-radius: 32px; transform-origin: 0 50%; }
.big { left: 0; right: 0; text-align: center; font-size: 180px; transform-origin: 50% 50%; }
.word { top: 400px; font-size: 150px; padding: 18px 40px 28px; background: var(--paper); border-radius: 32px; }
.word.right { right: 96px; transform-origin: 100% 50%; }
.word.left { left: 96px; transform-origin: 0 50%; }
.panel { left: 96px; bottom: 96px; font-size: 96px; padding: 20px 40px 30px; background: var(--paper); border-radius: 32px; }
.panel.top { top: 96px; bottom: auto; }
.note { position: absolute; right: 96px; top: 96px; font-size: 48px; line-height: 1.1; padding: 12px 28px 16px;
  background: var(--paper); border: 2px solid var(--muted); border-radius: 999px; white-space: nowrap; }
.orange { color: var(--orange); }
.wordmark { position: absolute; left: 0; right: 0; top: 300px; text-align: center; font-size: 240px; font-weight: 700; letter-spacing: -0.04em; line-height: 1; }
.end-line, .end-url { position: absolute; left: 0; right: 0; text-align: center; font-size: 64px; line-height: 1.1; }
.end-line { top: 600px; }
.end-url { top: 700px; color: var(--muted); }
#wall { position: absolute; inset: 0; overflow: hidden; display: none; }
.wall { position: absolute; left: 50%; top: 50%; display: grid; gap: 8px; transform-origin: 50% 50%; }
.wall img { width: 180px; height: 120px; border-radius: 6px; display: block; }
.slam-center { position: absolute; left: 0; right: 0; text-align: center; }
.slam-center span { display: inline-block; font-weight: 700; letter-spacing: -0.03em; line-height: 1; font-size: 150px;
  padding: 18px 40px 28px; background: var(--paper); border-radius: 32px; white-space: nowrap; }
</style></head><body><div id="wall"></div><div id="root"></div>
<script>
const ease = (k) => 1 - Math.pow(1 - Math.max(0, Math.min(1, k)), 3);
const esc = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;");
function word(w) {
  const e = ease(w.k);
  const o = w.orange ? " orange" : "";
  switch (w.style) {
    case "slam": {
      const top = 640 + w.line * 200;
      return '<div class="slam" style="top:' + top + 'px;opacity:' + e + ';transform:scale(' + (1.35 - 0.35 * e) + ')">' + esc(w.text) + "</div>";
    }
    case "slam-center": {
      const top = 300 + w.line * 230;
      return '<div class="slam-center" style="top:' + top + 'px;opacity:' + e + ';transform:scale(' + (1.35 - 0.35 * e) + ')"><span>' + esc(w.text) + "</span></div>";
    }
    case "big": {
      const top = 300 + w.line * 260;
      return '<div class="big" style="top:' + top + 'px;opacity:' + e + ';transform:scale(' + (1.3 - 0.3 * e) + ')">' + esc(w.text) + "</div>";
    }
    case "word":
      return '<div class="word ' + w.side + '" style="opacity:' + e + ';transform:scale(' + (1.3 - 0.3 * e) + ')">' + esc(w.text) + "</div>";
    case "panel":
      return '<div class="panel' + o + (w.top ? " top" : "") + '" style="opacity:' + e + ';transform:translateY(' + (60 * (1 - e)) + 'px)">' + esc(w.text) + "</div>";
    case "note":
      return '<div class="note">' + esc(w.text) + "</div>";
    case "wordmark":
      return '<div class="wordmark" style="opacity:' + e + ';transform:scale(' + (1.2 - 0.2 * e) + ')">' + esc(w.text) + "</div>";
    default:
      return '<div class="' + w.style + '" style="opacity:' + e + ';transform:translateY(' + (40 * (1 - e)) + 'px)">' + esc(w.text) + "</div>";
  }
}
const loaded = new Map();
async function img(src) {
  if (!loaded.has(src)) {
    const i = new Image();
    i.src = src;
    await i.decode();
    loaded.set(src, { w: i.naturalWidth, h: i.naturalHeight });
  }
  return loaded.get(src);
}
let wallBuilt = false;
function wall(st) {
  const layer = document.getElementById("wall");
  if (!st.wall) {
    layer.style.display = "none";
    return;
  }
  layer.style.display = "block";
  const box = layer.firstElementChild;
  box.style.transform = "translate(-50%, -50%) translate(" + st.wall.x + "px," + st.wall.y + "px) rotate(" + st.wall.r + "deg) scale(" + st.wall.s + ")";
}
window.buildWall = async (tiles, cols) => {
  const layer = document.getElementById("wall");
  layer.innerHTML = '<div class="wall" style="grid-template-columns:repeat(' + cols + ',180px)">' +
    tiles.map((t) => '<img src="' + t + '">').join("") + "</div>";
  await Promise.all([...layer.querySelectorAll("img")].map((i) => i.decode().catch(() => undefined)));
  wallBuilt = true;
};
window.render = async (st) => {
  wall(st);
  const parts = [];
  if (st.src) {
    await img(st.src);
    parts.push('<img class="full" src="' + st.src + '">');
  }
  parts.push('<div class="layer">' + st.words.map(word).join("") + "</div>");
  const root = document.getElementById("root");
  root.innerHTML = parts.join("");
  await Promise.all([...root.querySelectorAll("img")].map((i) => i.decode().catch(() => undefined)));
  await document.fonts.ready;
};
</script></body></html>`;
}

async function encodeScene(page, scene, work) {
  const final = join(work, `${scene.id}.mp4`);
  const tmp = join(work, `${scene.id}.tmp.mp4`);
  const ff = spawn(
    FFMPEG,
    [
      ...["-v", "error", "-y", "-f", "image2pipe", "-framerate", String(FPS), "-c:v", "mjpeg", "-i", "-"],
      ...["-vf", "format=yuv420p", "-c:v", "libx264", "-preset", "veryfast", "-crf", "18"],
      ...["-pix_fmt", "yuv420p", "-r", String(FPS), "-an", "-movflags", "+faststart", tmp],
    ],
    { stdio: ["pipe", "inherit", "inherit"] },
  );
  const done = new Promise((ok, fail) =>
    ff.on("exit", (code) => (code === 0 ? ok() : fail(new Error(`ffmpeg exited ${code} on ${scene.id}`)))),
  );
  const frames = sceneFrames(scene);
  let lastKey = "";
  let last = null;
  for (let n = 0; n < frames; n += 1) {
    const st = stateAt(scene, n / FPS);
    const key = JSON.stringify(st);
    if (key !== lastKey) {
      await page.evaluate((s) => window.render(s), st);
      last = await page.screenshot({ type: "jpeg", quality: 92 });
      lastKey = key;
    }
    if (!ff.stdin.write(last)) {
      await new Promise((ok) => ff.stdin.once("drain", ok));
    }
  }
  ff.stdin.end();
  await done;
  renameSync(tmp, final);
  console.log(`${scene.id}: ${frames} frames`);
  return final;
}

async function still(page, scene, t, file) {
  await page.evaluate((s) => window.render(s), stateAt(scene, t));
  writeFileSync(file, await page.screenshot({ type: "jpeg", quality: 92 }));
  console.log(`${file}: ${scene.id} at ${t}s`);
}

function run(argv, capture = false) {
  return new Promise((ok, fail) => {
    const p = spawn(FFMPEG, argv, { stdio: ["ignore", "inherit", capture ? "pipe" : "inherit"] });
    let err = "";
    if (capture) p.stderr.on("data", (d) => (err += d));
    p.on("exit", (code) => (code === 0 ? ok(err) : fail(new Error(`ffmpeg exited ${code}\n${err}`))));
  });
}

/**
 * Lays the music under the finished master (video copied, not encoded again): trimmed to the
 * video, loudness measured, then normalised to -16 LUFS in a second, linear pass, then the fades.
 */
async function addMusic(master, duration, out) {
  const target = "I=-16:TP=-1.5:LRA=11";
  const trim = `atrim=0:${duration},asetpts=PTS-STARTPTS`;
  const log = await run(
    ["-hide_banner", "-nostats", "-i", AUDIO, "-af", `${trim},loudnorm=${target}:print_format=json`, "-f", "null", "-"],
    true,
  );
  const m = JSON.parse(log.slice(log.lastIndexOf("{"), log.lastIndexOf("}") + 1));
  const measured = `measured_I=${m.input_i}:measured_TP=${m.input_tp}:measured_LRA=${m.input_lra}:measured_thresh=${m.input_thresh}:offset=${m.target_offset}:linear=true`;
  const fadeOut = Math.max(0, duration - 2.5).toFixed(3);
  const chain = `[1:a]${trim},loudnorm=${target}:${measured},aresample=48000,afade=t=in:st=0:d=1,afade=t=out:st=${fadeOut}:d=2.5[a]`;
  const tmp = out.replace(/\.mp4$/, ".tmp.mp4");
  await run([
    ...["-v", "error", "-y", "-i", master, "-i", AUDIO, "-filter_complex", chain],
    ...["-map", "0:v", "-map", "[a]", "-c:v", "copy", "-c:a", "aac", "-b:a", "192k", "-ac", "2"],
    ...["-t", String(duration), "-movflags", "+faststart", tmp],
  ]);
  renameSync(tmp, out);
  console.log(`wrote ${out} (music at -16 LUFS: not for the repository)`);
}

async function main() {
  mkdirSync(OUT, { recursive: true });
  const work = join(OUT, "work");
  mkdirSync(work, { recursive: true });
  const html = join(work, "compose.html");
  writeFileSync(html, composeHtml());
  const all = scenes();
  const byId = (id) => all.find((s) => s.id === id);
  const browser = await chromium.launch({ args: ["--allow-file-access-from-files"] });
  const page = await browser.newPage({ viewport: { width: W, height: H }, deviceScaleFactor: 1 });
  await page.goto(pathToFileURL(html).href);
  await page.evaluate(() => document.fonts.load('700 100px "Bricolage"'));
  const wallScene = all.find((s) => s.wall);
  await page.evaluate(([tiles, cols]) => window.buildWall(tiles, cols), [wallScene.wall.tiles, COLS]);

  // The busiest moments, to look at before the full render.
  const checks = [
    ["1-wall", 0.5 * BEAT],
    ["1-wall", 2.5 * BEAT],
    ["1-wall", 7 * BEAT],
    ["4-swipe", 3.2 * BEAT],
    ["4-swipe", 12 * BEAT],
    ["6-apply", 6 * BEAT],
    ["7-end", 8 * BEAT],
  ];
  for (const [i, [id, t]] of checks.entries()) {
    await still(page, byId(id), t, join(OUT, `frame-${i + 1}-${id}.jpg`));
  }
  // More on request: --still <scene id>:<seconds into the scene>, repeatable.
  for (let i = 0; i < args.length; i += 1) {
    if (args[i] === "--still") {
      const [id, t] = args[i + 1].split(":");
      await still(page, byId(id), Number(t), join(OUT, `still-${id}-${t}.jpg`));
    }
  }
  if (CHECK) {
    await browser.close();
    return;
  }

  const files = [];
  for (const scene of all) {
    files.push(await encodeScene(page, scene, work));
  }
  await still(page, all[0], 7 * BEAT, join(OUT, `${NAME}-poster.jpg`));
  await browser.close();

  const list = join(work, "concat.txt");
  writeFileSync(list, `${files.map((f) => `file '${f.replaceAll("'", "'\\''")}'`).join("\n")}\n`);
  const tmp = join(OUT, `${NAME}.tmp.mp4`);
  await run(["-v", "error", "-y", "-f", "concat", "-safe", "0", "-i", list, "-c", "copy", "-an", "-movflags", "+faststart", tmp]);
  renameSync(tmp, join(OUT, `${NAME}.mp4`));
  if (AUDIO !== null) {
    const duration = all.reduce((sum, scene) => sum + sceneFrames(scene), 0) / FPS;
    await addMusic(join(OUT, `${NAME}.mp4`), duration, join(OUT, `${NAME}.music.mp4`));
  }
  if (!process.env.CULLA_VIDEO_KEEP_WORK) {
    rmSync(work, { recursive: true, force: true });
  }
  console.log(`wrote ${join(OUT, `${NAME}.mp4`)}`);
}

if (MASTER !== null) {
  if (AUDIO === null || OUT === null) {
    console.error("usage: node compose.mjs --master <mp4> --audio <file> --out <dir>");
    process.exit(2);
  }
  // The master's length, from ffmpeg's own report (no ffprobe needed).
  const info = await run(["-hide_banner", "-i", MASTER, "-f", "null", "-"], true);
  const [, h, mi, se] = info.match(/Duration: (\d+):(\d+):([\d.]+)/);
  mkdirSync(OUT, { recursive: true });
  await addMusic(MASTER, Number(h) * 3600 + Number(mi) * 60 + Number(se), join(OUT, `${NAME}.music.mp4`));
} else if ([CAPTURE, WALL, FONTS, OUT].includes(null)) {
  console.error("usage: node compose.mjs --capture <dir> --wall <dir> --fonts <dir> --out <dir> [--bpm 120] [--offset 0]");
  process.exit(2);
} else if (!existsSync(join(CAPTURE, "swipe", "timeline.json"))) {
  console.error(`no capture at ${CAPTURE}`);
  process.exit(2);
} else {
  await main();
}
