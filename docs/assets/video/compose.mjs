// Edits the demo video (docs/videos.md (a)) from the shots capture.spec.ts filmed.
//
//   node docs/assets/video/compose.mjs --capture <dir> --out <dir> --fonts <dir> [--check]
//
// --capture  the folder capture.spec.ts wrote (CULLA_VIDEO_CAPTURE), with shot-03 to shot-08
// --out      where culla-demo.mp4, culla-demo-poster.jpg and the check frames go
// --fonts    a folder holding Bricolage Grotesque (OFL) as .ttf or .woff2: Regular and Bold, or the
//            variable font. Not in the repository (SPEC §5.6: self-hosted from M3).
// --check    only draws the four check frames (title card, keep route, review route, end card)
// --audio    optional music file: also writes culla-demo.music.mp4, the master with the track
//            trimmed to the video, normalised to -16 LUFS (loudnorm, two passes), faded in over
//            1 s and out over the last 2.5 s, AAC stereo. The master stays silent. The music copy
//            and the track are never committed (docs/videos.md, Sound).
//            With --master <mp4> (and --out), only lays the music under that finished master.
//
// Every picture is drawn by Chromium at 1920×1080 (cards, on-screen text, key labels, lower
// third; the filmed page sits 1:1 under the text), sent to ffmpeg as JPEG (quality 92), one H.264
// file per scene (30 fps, -preset veryfast, no audio), then joined with the concat demuxer and
// -c copy. Each scene is written to a .tmp.mp4 name and renamed once ffmpeg exits 0. Plain cuts
// between scenes; only the text fades in. Needs ffmpeg on PATH (or FFMPEG) and Playwright's
// Chromium.
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { chromium } from "playwright";

const args = process.argv.slice(2);
const arg = (name) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : undefined;
};
const CAPTURE = resolve(arg("capture") ?? "");
const OUT = resolve(arg("out") ?? "");
const FONTS = resolve(arg("fonts") ?? "");
const CHECK = args.includes("--check");
const AUDIO = arg("audio") ? resolve(arg("audio")) : null;
const FFMPEG = process.env.FFMPEG ?? "ffmpeg";
const FPS = 30;
const W = 1920;
const H = 1080;

const MASTER = arg("master") ? resolve(arg("master")) : null;
if (MASTER === null && (!arg("capture") || !arg("out") || !arg("fonts"))) {
  console.error("usage: node compose.mjs --capture <dir> --out <dir> --fonts <dir> [--check]");
  process.exit(2);
}

// The filmed page (1600×900) sits under a 150 px band for the on-screen text.
const APP = { x: 160, y: 150, w: 1600, h: 900 };

const shot = (n) => {
  const dir = join(CAPTURE, `shot-0${n}`);
  const timeline = JSON.parse(readFileSync(join(dir, "timeline.json"), "utf8"));
  return { dir, ...timeline };
};

/**
 * The scenes, from the shot list (docs/videos.md (a)). `text` is the on-screen text, with the
 * times it shows; key labels come from the key presses capture.spec.ts logged.
 */
function scenes() {
  const s = (n) => shot(n);
  const s8 = s(8);
  return [
    { id: "01-title", duration: 6, card: "title" },
    { id: "02-raw", duration: 6, card: "raw" },
    {
      id: "03-route",
      duration: 12,
      shot: s(3),
      text: [{ from: 0.2, to: 12, text: "As you sort, see where every photo will go" }],
    },
    {
      id: "04-decide",
      duration: 10,
      shot: s(4),
      text: [{ from: 0, to: 10, text: "Keep or reject each photo" }],
    },
    {
      id: "05-keys",
      duration: 10,
      shot: s(5),
      text: [
        { from: 0, to: 5.0, text: "Or use the arrow keys" },
        { from: 5.0, to: 10, text: "Under each button: where this photo would go" },
      ],
    },
    {
      id: "06-undo",
      duration: 8,
      shot: s(6),
      text: [{ from: 0, to: 8, text: "Changed your mind? Ctrl+Z" }],
    },
    {
      id: "07-review",
      duration: 10,
      shot: s(7),
      text: [{ from: 0, to: 10, text: "Review the whole plan first" }],
    },
    {
      id: "08-apply",
      duration: Math.round(s8.duration * FPS) / FPS,
      shot: s8,
      text: [{ from: 0, to: s8.duration, text: "Nothing is deleted" }],
      lowerThird: { from: 0.3, text: "Web demo: the apply is simulated" },
    },
    { id: "09-end", duration: 8, card: "end" },
  ];
}

/** The frame of a shot on screen at `t` (the last one filmed at or before it). */
function frameAt(sh, t) {
  let file = sh.frames[0].file;
  for (const f of sh.frames) {
    if (f.t > t + 1e-6) break;
    file = f.file;
  }
  return pathToFileURL(join(sh.dir, file)).href;
}

/** What to draw at `t` in a scene: plain data, also the key that says if a frame can be reused. */
function stateAt(scene, t) {
  const st = { card: scene.card ?? null, t: scene.card ? t : 0 };
  if (scene.shot) {
    st.src = frameAt(scene.shot, t);
    const line = scene.text.find((x) => t >= x.from && t < x.to) ?? null;
    st.text = line?.text ?? null;
    st.textOpacity = line === null ? 0 : fade(t - line.from, line.to - t, scene.text.indexOf(line) === 0 ? 0.35 : 0.25);
    const key = [...scene.shot.events].reverse().find((e) => t >= e.t && t < e.t + 1.4) ?? null;
    st.keys = key?.keys ?? null;
    st.keyLabel = key?.label ?? null;
    st.keyOpacity = key === null ? 0 : fade(t - key.t, key.t + 1.4 - t, 0.12);
    if (scene.lowerThird && t >= scene.lowerThird.from) {
      st.lower = scene.lowerThird.text;
      st.lowerOpacity = fade(t - scene.lowerThird.from, 99, 0.3);
    }
  }
  return st;
}

/** Fades in over `d` seconds from the start and out over `d` before the end (never a flash). */
function fade(sinceStart, untilEnd, d) {
  const k = Math.min(1, sinceStart / d, untilEnd / d);
  return Math.round(Math.max(0, k) * 100) / 100;
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

/** The drawing page: Night, the Portfolio Design System's colours, no gradients or shadows. */
function composeHtml() {
  return `<!doctype html><html><head><meta charset="utf-8"><style>
${fontFaces()}
:root { --paper: #0c0b0a; --raised: #171513; --ink: #f1e9d8; --muted: #a8a092; --faint: #6f695f; --rule: #2c2925; --orange: #ffb38a; }
* { box-sizing: border-box; margin: 0; }
html, body { width: ${W}px; height: ${H}px; background: var(--paper); color: var(--ink); overflow: hidden;
  font-family: "Bricolage", "DejaVu Sans", sans-serif; }
#app { position: absolute; left: ${APP.x}px; top: ${APP.y}px; width: ${APP.w}px; height: ${APP.h}px;
  border-radius: 18px 18px 0 0; overflow: hidden; }
#app img { display: block; width: ${APP.w}px; height: ${APP.h}px; }
#band { position: absolute; left: ${APP.x}px; right: ${W - APP.x - APP.w}px; top: 0; height: ${APP.y}px;
  display: flex; align-items: center; justify-content: space-between; gap: 32px; }
#text { font-size: 56px; line-height: 1.1; letter-spacing: -0.5px; white-space: nowrap; }
#keys { display: flex; align-items: center; gap: 14px; font-size: 48px; }
.cap { display: inline-flex; align-items: center; justify-content: center; min-width: 80px; height: 80px;
  padding: 0 20px; border: 3px solid var(--ink); border-radius: 16px; font-size: 48px; line-height: 1; }
.plus { color: var(--muted); }
.key-label { margin-left: 10px; }
#lower { position: absolute; right: ${W - APP.x - APP.w + 32}px; bottom: ${H - APP.y - APP.h + 32}px; padding: 14px 32px;
  background: var(--paper); border: 2px solid var(--muted); border-radius: 999px; font-size: 48px; line-height: 1.1; }
.wordmark { font-weight: 700; letter-spacing: -0.035em; line-height: 1; }
.headline { font-size: 64px; line-height: 1.12; letter-spacing: -0.5px; }
.card { position: absolute; inset: 0; }
.file-name { font-size: 48px; text-align: center; margin-top: 24px; white-space: nowrap; }
</style></head><body><div id="root"></div>
<script>
const photo = (fill, stroke, sun) => \`<svg viewBox="-75 -105 150 210" width="300" height="420">
  <rect x="-74" y="-104" width="148" height="208" rx="18" fill="\${fill}" stroke="\${stroke}" stroke-width="2.5"/>
  <rect x="-61" y="-91" width="122" height="146" rx="8" fill="#2c2925"/>
  <circle cx="26" cy="-56" r="12" fill="\${sun}"/>
  <path d="M-61 34 L-24 -10 L2 18 L22 0 L61 40 L61 47 Q61 55 53 55 L-53 55 Q-61 55 -61 47 Z" fill="#6f695f"/>
  <rect x="-61" y="70" width="70" height="6" rx="3" fill="#6f695f"/>
  <rect x="-61" y="84" width="44" height="6" rx="3" fill="#2c2925"/></svg>\`;
const fileIcon = (stroke, label) => \`<svg viewBox="0 0 160 200" width="220" height="275">
  <path d="M8 8 H108 L152 52 V192 H8 Z" fill="#171513" stroke="\${stroke}" stroke-width="6" stroke-linejoin="round"/>
  <path d="M108 8 V52 H152" fill="none" stroke="\${stroke}" stroke-width="6" stroke-linejoin="round"/>
  <text x="80" y="140" text-anchor="middle" font-family="Bricolage, sans-serif" font-weight="700" font-size="40" fill="\${stroke}">\${label}</text></svg>\`;
const folderIcon = \`<svg viewBox="0 0 200 160" width="250" height="200">
  <path d="M8 30 Q8 16 22 16 H78 L96 36 H178 Q192 36 192 50 V138 Q192 152 178 152 H22 Q8 152 8 138 Z" fill="#171513" stroke="#a8a092" stroke-width="6" stroke-linejoin="round"/></svg>\`;
const ease = (k) => (k <= 0 ? 0 : k >= 1 ? 1 : k < 0.5 ? 4 * k * k * k : 1 - Math.pow(-2 * k + 2, 3) / 2);
const clamp = (k) => Math.max(0, Math.min(1, k));

function title(t) {
  const cards = [
    ["#171513", "#6f695f", "#a8a092", -14, -70, 30],
    ["#171513", "#6f695f", "#a8a092", 9, 60, 20],
    ["#171513", "#a8a092", "#a8a092", -5, -20, 10],
    ["#171513", "#f1e9d8", "#ffb38a", 3, 20, 0],
  ];
  const stack = cards.map(([fill, stroke, sun, rot, dx, dy], i) => {
    const k = ease(clamp((t - 0.2 - i * 0.25) / 0.6));
    return \`<div style="position:absolute;left:\${1240 + dx}px;top:\${300 + dy + (1 - k) * 30}px;opacity:\${k};transform:rotate(\${rot}deg)">\${photo(fill, stroke, sun)}</div>\`;
  }).join("");
  const w = clamp((t - 0.1) / 0.5), h = clamp((t - 0.9) / 0.5);
  return \`<div class="card">\${stack}
    <div class="wordmark" style="position:absolute;left:160px;top:300px;font-size:150px;opacity:\${w}">Culla</div>
    <div class="headline" style="position:absolute;left:160px;top:500px;width:880px;opacity:\${h}">After a shoot: hundreds of photos to sort</div></div>\`;
}

function raw(t) {
  const h = clamp((t - 0.1) / 0.4);
  const slide = ease(clamp((t - 1.6) / 1.4));
  const gone = clamp((t - 2.8) / 0.4);
  const behind = clamp((t - 3.1) / 0.4);
  const jx = 700 + slide * 760;
  const nefStroke = behind > 0.5 ? "#ffb38a" : "#f1e9d8";
  return \`<div class="card">
    <div class="headline" style="position:absolute;left:160px;top:170px;width:1600px;opacity:\${h}">Sorting by hand, the RAW gets left behind</div>
    <div style="position:absolute;left:1440px;top:470px">\${folderIcon}<div class="file-name" style="color:#a8a092">Selects</div></div>
    <div style="position:absolute;left:\${jx}px;top:430px;opacity:\${1 - gone}">\${fileIcon("#f1e9d8", "JPG")}<div class="file-name">DSC_0412.JPG</div></div>
    <div style="position:absolute;left:400px;top:430px">\${fileIcon(nefStroke, "NEF")}<div class="file-name" style="color:\${nefStroke}">DSC_0412.NEF</div></div>
  </div>\`;
}

function end(t) {
  const a = clamp((t - 0.1) / 0.5), b = clamp((t - 0.6) / 0.5), c = clamp((t - 1.1) / 0.5);
  return \`<div class="card" style="display:flex;flex-direction:column;align-items:center;justify-content:center;text-align:center">
    <div class="wordmark" style="font-size:180px;opacity:\${a}">Culla</div>
    <div class="headline" style="margin-top:48px;color:var(--orange);opacity:\${b}">Nothing overwritten, nothing deleted</div>
    <div style="margin-top:56px;font-size:52px;color:var(--muted);opacity:\${c}">Free and open source, for Windows 11</div>
    <div style="margin-top:20px;font-size:52px;opacity:\${c}">github.com/margaux-faurie/culla</div></div>\`;
}

let built = null;
window.render = async (st) => {
  const root = document.getElementById("root");
  if (st.card) {
    built = null;
    root.innerHTML = st.card === "title" ? title(st.t) : st.card === "raw" ? raw(st.t) : end(st.t);
    await document.fonts.ready;
    return;
  }
  if (built !== "app") {
    root.innerHTML = '<div id="band"><div id="text"></div><div id="keys"></div></div><div id="app"><img alt=""></div><div id="lower"></div>';
    built = "app";
  }
  const img = root.querySelector("#app img");
  if (img.getAttribute("src") !== st.src) {
    img.src = st.src;
    await img.decode();
  }
  const text = root.querySelector("#text");
  text.textContent = st.text ?? "";
  text.style.opacity = st.textOpacity;
  const keys = root.querySelector("#keys");
  keys.innerHTML = st.keys
    ? st.keys.map((k) => '<span class="cap">' + k + "</span>").join('<span class="plus">+</span>') +
      '<span class="key-label">' + st.keyLabel + "</span>"
    : "";
  keys.style.opacity = st.keyOpacity;
  const lower = root.querySelector("#lower");
  lower.textContent = st.lower ?? "";
  lower.style.display = st.lower ? "block" : "none";
  lower.style.opacity = st.lowerOpacity ?? 0;
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
  const frames = Math.round(scene.duration * FPS);
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

  // The busiest moments, to look at before the full render.
  await still(page, byId("01-title"), 3.5, join(OUT, "frame-1-title.jpg"));
  await still(page, byId("05-keys"), 7.5, join(OUT, "frame-2-swipe-route.jpg"));
  await still(page, byId("07-review"), 5.0, join(OUT, "frame-3-review-route.jpg"));
  await still(page, byId("09-end"), 5.0, join(OUT, "frame-4-end.jpg"));
  // More stills on request: --still <scene id>:<seconds>, repeatable.
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
  await still(page, byId("01-title"), 4.0, join(OUT, "culla-demo-poster.jpg"));
  await browser.close();

  const list = join(work, "concat.txt");
  writeFileSync(list, files.map((f) => `file '${f.replaceAll("'", "'\\''")}'`).join("\n") + "\n");
  const tmp = join(OUT, "culla-demo.tmp.mp4");
  await run(["-v", "error", "-y", "-f", "concat", "-safe", "0", "-i", list, "-c", "copy", "-an", "-movflags", "+faststart", tmp]);
  renameSync(tmp, join(OUT, "culla-demo.mp4"));
  if (AUDIO !== null) {
    const duration = all.reduce((sum, scene) => sum + Math.round(scene.duration * FPS), 0) / FPS;
    await addMusic(join(OUT, "culla-demo.mp4"), duration, join(OUT, "culla-demo.music.mp4"));
  }
  if (!process.env.CULLA_VIDEO_KEEP_WORK) {
    rmSync(work, { recursive: true, force: true });
  }
  console.log(`wrote ${join(OUT, "culla-demo.mp4")}`);
}

if (MASTER !== null) {
  if (AUDIO === null || !arg("out")) {
    console.error("usage: node compose.mjs --master <mp4> --audio <file> --out <dir>");
    process.exit(2);
  }
  // The master's length, from ffmpeg's own report (no ffprobe needed).
  const info = await run(["-hide_banner", "-i", MASTER, "-f", "null", "-"], true);
  const [, h, mi, se] = info.match(/Duration: (\d+):(\d+):([\d.]+)/);
  mkdirSync(OUT, { recursive: true });
  await addMusic(MASTER, Number(h) * 3600 + Number(mi) * 60 + Number(se), join(OUT, "culla-demo.music.mp4"));
} else if (!existsSync(CAPTURE)) {
  console.error(`no capture at ${CAPTURE}`);
  process.exit(2);
} else {
  await main();
}
