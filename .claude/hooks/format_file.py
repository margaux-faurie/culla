"""PostToolUse hook (Edit|Write|MultiEdit): format the file that was just edited.

- .rs: rustfmt on that file (picks up rustfmt.toml from the repo; also formats its
  out-of-line child modules, which is harmless).
- ts, tsx, js, jsx, mjs, cjs, json, jsonc, css: Biome from node_modules/.bin, once installed.

A missing formatter, a file outside the project or a formatter error (for example a syntax
error mid-edit) is skipped quietly: the Stop check and CI catch real problems.
Never downloads anything (no npx).
"""
import json
import os
import shutil
import subprocess
import sys

BIOME_EXTS = {".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".json", ".jsonc", ".css"}
TIMEOUT_S = 30


def run(cmd: list, cwd: str) -> None:
    try:
        subprocess.run(cmd, cwd=cwd, stdin=subprocess.DEVNULL, capture_output=True, timeout=TIMEOUT_S)
    except (OSError, subprocess.TimeoutExpired):
        pass


def main() -> int:
    data = json.load(sys.stdin)
    tool_input = data.get("tool_input") or {}
    file_path = tool_input.get("file_path")
    root = os.environ.get("CLAUDE_PROJECT_DIR") or data.get("cwd")
    if not file_path or not root:
        return 0
    root = os.path.realpath(root)
    path = os.path.realpath(os.path.join(root, file_path))
    if not os.path.isfile(path):
        return 0
    try:
        if os.path.relpath(path, root).replace("\\", "/").startswith("../"):
            return 0
    except ValueError:
        return 0

    ext = os.path.splitext(path)[1].lower()
    if ext == ".rs":
        rustfmt = shutil.which("rustfmt")
        if rustfmt:
            run([rustfmt, "--quiet", path], cwd=root)
    elif ext in BIOME_EXTS:
        biome = shutil.which("biome", path=os.path.join(root, "node_modules", ".bin"))
        if biome:
            run([biome, "format", "--write", "--no-errors-on-unmatched", path], cwd=root)
    return 0


if __name__ == "__main__":
    sys.exit(main())
