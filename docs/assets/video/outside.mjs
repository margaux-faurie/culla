// The video tooling's one guard (docs/videos.md, Photos): everything it reads from Margaux's
// photos or writes from them stays outside the repository. Node only, no dependencies.
import { existsSync, realpathSync } from "node:fs";
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

/** The repository root, symlinks resolved. */
export const REPO = realpathSync(fileURLToPath(new URL("../../../", import.meta.url)));

/**
 * `path` made absolute with symlinks resolved, also when it does not exist yet: the nearest
 * existing folder above it is resolved and the rest appended.
 */
export function realPath(path) {
  let head = resolve(path);
  const tail = [];
  while (!existsSync(head)) {
    const up = dirname(head);
    if (up === head) {
      break;
    }
    tail.unshift(basename(head));
    head = up;
  }
  return join(existsSync(head) ? realpathSync(head) : head, ...tail);
}

/**
 * True when `path` is the repository or inside it. Another drive (Windows) is outside, and so is
 * a sibling whose name only starts with "..": `relative` gives "..out" for `<repo>/..out`, which
 * is inside.
 */
export function isInside(path, root = REPO) {
  const rel = relative(root, realPath(path));
  return rel === "" || (rel !== ".." && !rel.startsWith(`..${sep}`) && !isAbsolute(rel));
}

/** Throws when `path` (named `what` in the message) is the repository or inside it. */
export function assertOutside(path, what) {
  if (isInside(path)) {
    throw new Error(
      `${what} must be outside the repository (${REPO}): it holds Margaux's photos or what is made from them`,
    );
  }
  return realPath(path);
}

/** Throws unless `name` is a plain file name: no folder, no "..", not empty. */
export function assertPlainName(name, what) {
  if (name === "" || name === "." || name === ".." || /[/\\]/.test(name) || name.includes("\0")) {
    throw new Error(`${what} must be a plain name, without a folder: ${JSON.stringify(name)}`);
  }
  return name;
}
