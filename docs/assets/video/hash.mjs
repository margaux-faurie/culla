// Video freshness (docs/videos.md, "Keeping the video current"). Node only, no dependencies.
//
// The demo video films the screens listed in screens.json. `renderedFrom` is a SHA-256 over
// those files as they were at the last render. When the screens change, the author either
// renders the video again and records the new hash here, or adds a line to pending.md, so a
// change never slips past a release unnoticed (tests/video-freshness.test.ts).
//
//   node docs/assets/video/hash.mjs           prints the hash of the screens now
//   node docs/assets/video/hash.mjs --check   exits 1, saying what to do, if the video is stale
//                                             and pending.md has no entry since the render
//   node docs/assets/video/hash.mjs --write [--date YYYY-MM-DD]
//                                             after a render: records the hash and the date in
//                                             screens.json and empties pending.md's entries
import { createHash } from "node:crypto";
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

/** The repository root. */
export const ROOT = fileURLToPath(new URL("../../../", import.meta.url));
const SCREENS = fileURLToPath(new URL("screens.json", import.meta.url));
const PENDING = fileURLToPath(new URL("pending.md", import.meta.url));

/** A pattern of screens.json as a regular expression: `**` any depth, `*` within one name. */
export function globToRegExp(glob) {
  let re = "";
  for (let i = 0; i < glob.length; i += 1) {
    const c = glob.charAt(i);
    if (c === "*" && glob.charAt(i + 1) === "*") {
      re += ".*";
      i += 1;
    } else if (c === "*") {
      re += "[^/]*";
    } else {
      re += c.replace(/[.+?^${}()|[\]\\]/g, "\\$&");
    }
  }
  return new RegExp(`^${re}$`);
}

/** Every file under `dir`, as paths relative to `root` with `/` between names. */
function walk(root, dir) {
  const out = [];
  let entries;
  try {
    entries = readdirSync(join(root, dir), { withFileTypes: true });
  } catch {
    return out;
  }
  for (const entry of entries) {
    const path = dir === "" ? entry.name : `${dir}/${entry.name}`;
    if (entry.isDirectory()) {
      out.push(...walk(root, path));
    } else if (entry.isFile()) {
      out.push(path);
    }
  }
  return out;
}

/** The folder a pattern starts in: everything before its first `*`, up to the last `/`. */
const baseOf = (glob) => {
  const head = glob.split("*")[0];
  return head.includes("/") ? head.slice(0, head.lastIndexOf("/")) : "";
};

/**
 * The files the video films: matching one of `paths` and none of `exclude`, sorted by path
 * (code-unit order, the same on every OS).
 */
export function screenFiles(root, paths, exclude = []) {
  const skip = exclude.map(globToRegExp);
  const found = new Set();
  for (const glob of paths) {
    const re = globToRegExp(glob);
    for (const file of walk(root, baseOf(glob))) {
      if (re.test(file) && !skip.some((s) => s.test(file))) {
        found.add(file);
      }
    }
  }
  return [...found].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
}

/**
 * SHA-256 over each file's path and contents, in order. Line endings are read as LF, so a
 * Windows checkout gives the same hash as Linux.
 */
export function screensHash(root, files) {
  const hash = createHash("sha256");
  for (const file of files) {
    const text = readFileSync(join(root, file), "utf8").replace(/\r\n/g, "\n");
    hash.update(`${file}\0${text}\0`);
  }
  return hash.digest("hex");
}

const ENTRY = /^- (\d{4}-\d{2}-\d{2}) #([^\s:]+): (.+) changed — re-render before release$/;

/** The entries of pending.md: `- <date> #<PR or branch>: <screen> changed — re-render before release`. */
export function pendingEntries(text) {
  const entries = [];
  for (const line of text.split(/\r?\n/)) {
    const m = ENTRY.exec(line.trim());
    if (m !== null) {
      entries.push({ date: m[1], ref: m[2], screen: m[3] });
    }
  }
  return entries;
}

/**
 * Whether the video is fresh: the screens hash as at the render, or at least one pending entry
 * dated on or after the render. `message` says what to do when it is not.
 */
export function freshness({ current, renderedFrom, renderedAt, pendingText }) {
  if (current === renderedFrom) {
    return { fresh: true, pending: [], message: "The video was rendered from these screens." };
  }
  const pending = pendingEntries(pendingText).filter((e) => e.date >= renderedAt);
  if (pending.length > 0) {
    return {
      fresh: true,
      pending,
      message: `The screens changed since the render of ${renderedAt}; pending.md lists ${pending.length} change(s) to film before the release.`,
    };
  }
  return {
    fresh: false,
    pending,
    message: [
      `The screens the demo video films changed since its render of ${renderedAt}`,
      `(screens.json renderedFrom ${renderedFrom.slice(0, 12)}…, now ${current.slice(0, 12)}…).`,
      "Do one of these:",
      "  - re-render the video (render-all, docs/videos.md), then run `node docs/assets/video/hash.mjs --write`;",
      "  - or add a line to docs/assets/video/pending.md, for example:",
      `    - ${today()} #<PR or branch>: <screen> changed — re-render before release`,
    ].join("\n"),
  };
}

const today = () => new Date().toISOString().slice(0, 10);

/** screens.json, the hash of the screens now and pending.md. */
export function load(root = ROOT) {
  const screens = JSON.parse(readFileSync(SCREENS, "utf8"));
  const files = screenFiles(root, screens.paths, screens.exclude);
  return {
    screens,
    files,
    current: screensHash(root, files),
    pendingText: readFileSync(PENDING, "utf8"),
  };
}

function main(args) {
  const { screens, current, pendingText } = load();
  if (args[0] === "--check") {
    const result = freshness({ current, ...screens, pendingText });
    console.log(result.message);
    return result.fresh ? 0 : 1;
  }
  if (args[0] === "--write") {
    const at = args[1] === "--date" && args[2] !== undefined ? args[2] : today();
    if (!/^\d{4}-\d{2}-\d{2}$/.test(at)) {
      console.error("--date takes YYYY-MM-DD");
      return 2;
    }
    const next = { ...screens, renderedFrom: current, renderedAt: at };
    writeFileSync(SCREENS, `${JSON.stringify(next, null, 2)}\n`);
    const kept = pendingText.split(/\r?\n/).filter((line) => !ENTRY.test(line.trim()));
    writeFileSync(PENDING, `${kept.join("\n").trimEnd()}\n`);
    console.log(`Recorded ${current} (${at}); pending.md emptied.`);
    return 0;
  }
  if (args.length > 0) {
    console.error("usage: node docs/assets/video/hash.mjs [--check | --write [--date YYYY-MM-DD]]");
    return 2;
  }
  console.log(current);
  return 0;
}

if (process.argv[1] !== undefined && fileURLToPath(import.meta.url) === process.argv[1]) {
  process.exitCode = main(process.argv.slice(2));
}
