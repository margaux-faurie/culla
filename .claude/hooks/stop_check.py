"""Stop hook: a fast check before Claude ends its turn (docs/testing.md §11).

Runs `cargo check --workspace --quiet`, `scripts/ci/check_lint_allows.py` and, once the JS side
exists, `tsc --noEmit`. On failure it exits 2 with a short report on stderr, so Claude keeps
working. Full tests, clippy and Windows jobs are left to CI.

No endless loops:
- `stop_hook_active` false (a fresh stop) resets the block counter for this session.
- `stop_hook_active` true (Claude is already continuing because of this hook) counts up; after
  MAX_BLOCKS consecutive blocks the hook lets Claude stop and warns the user instead.
- A check that times out or whose tool is missing never blocks.
Claude Code also caps stop-hook continuations at 8 in a row.
"""
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time

MAX_BLOCKS = 3
BUDGET_S = 180
TAIL_LINES = 40


def state_file(session: str, root: str) -> str:
    key = hashlib.sha256(f"{session}|{root}".encode()).hexdigest()[:16]
    return os.path.join(tempfile.gettempdir(), f"culla-stop-hook-{key}.json")


def read_blocks(path: str) -> int:
    try:
        with open(path, encoding="utf-8") as f:
            return int(json.load(f).get("blocks", 0))
    except (OSError, ValueError, AttributeError):
        return 0


def write_blocks(path: str, blocks: int) -> None:
    try:
        with open(path, "w", encoding="utf-8") as f:
            json.dump({"blocks": blocks}, f)
    except OSError:
        pass


def checks(root: str) -> list:
    out = []
    if os.path.isfile(os.path.join(root, "Cargo.toml")) and shutil.which("cargo"):
        out.append(("cargo check", [shutil.which("cargo"), "check", "--workspace", "--quiet"]))
    lint = os.path.join(root, "scripts", "ci", "check_lint_allows.py")
    if os.path.isfile(lint) and shutil.which("git"):
        out.append(("check_lint_allows.py", [sys.executable, lint]))
    tsc = shutil.which("tsc", path=os.path.join(root, "node_modules", ".bin"))
    if tsc and os.path.isfile(os.path.join(root, "tsconfig.json")):
        out.append(("tsc", [tsc, "--noEmit", "-p", root]))
    return out


def main() -> int:
    raw = sys.stdin.read()
    data = json.loads(raw) if raw.strip() else {}
    root = os.path.realpath(os.environ.get("CLAUDE_PROJECT_DIR") or data.get("cwd") or os.getcwd())
    state = state_file(str(data.get("session_id", "")), root)
    blocks = read_blocks(state) if data.get("stop_hook_active") else 0

    env = dict(os.environ, CARGO_TERM_COLOR="never", NO_COLOR="1")
    deadline = time.monotonic() + BUDGET_S
    failures, skipped = [], []
    for name, cmd in checks(root):
        left = deadline - time.monotonic()
        if left <= 5:
            skipped.append(f"{name} (time budget used up)")
            continue
        try:
            r = subprocess.run(
                cmd, cwd=root, env=env, stdin=subprocess.DEVNULL,
                capture_output=True, text=True, errors="replace", timeout=left,
            )
        except subprocess.TimeoutExpired:
            skipped.append(f"{name} (timed out)")
            continue
        except OSError as e:
            skipped.append(f"{name} ({e})")
            continue
        if r.returncode != 0:
            lines = (r.stdout + r.stderr).strip().splitlines()
            failures.append((name, "\n".join(lines[-TAIL_LINES:])))

    if not failures:
        write_blocks(state, 0)
        if skipped:
            print(json.dumps({"systemMessage": "Culla stop check skipped: " + "; ".join(skipped)}))
        return 0

    blocks += 1
    write_blocks(state, blocks)
    names = ", ".join(n for n, _ in failures)
    if blocks > MAX_BLOCKS:
        write_blocks(state, 0)
        print(json.dumps({"systemMessage": f"Culla stop check still failing after {MAX_BLOCKS} attempts ({names}); letting Claude stop."}))
        return 0

    report = [f"Fast check failed: {names}. Fix it before you finish. If it cannot be fixed in this task, "
              "say so plainly in your reply with the failing command and output."]
    for name, tail in failures:
        report.append(f"--- {name} (last {TAIL_LINES} lines) ---\n{tail}")
    print("\n".join(report), file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
