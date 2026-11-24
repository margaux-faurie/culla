"""PreToolUse hook (Edit|Write|MultiEdit|NotebookEdit): block edits to protected paths.

Protected paths hold the rules that keep agents honest: lint and deny config, cargo config,
CI scripts and workflows, the audited removal module, Tauri capabilities, and the Claude Code
settings, hooks and agents themselves (design/file-engine.md §9.7). See README.md here.

Override: Margaux starts Claude Code with CULLA_ALLOW_PROTECTED_EDITS=1 in its environment.

Exit 2 blocks the call and stderr goes to Claude. Any crash exits non-zero, and the hook is
registered with onFailure "block", so a broken hook blocks rather than lets edits through.
"""
import json
import os
import re
import sys

OVERRIDE_ENV = "CULLA_ALLOW_PROTECTED_EDITS"

# Globs against the path relative to a repo root, "/" separators, case-folded.
# "**/" = any leading folders (or none), "/**" = everything below, "*" = within one segment.
PROTECTED = [
    "**/clippy.toml",
    "**/.clippy.toml",
    "deny.toml",
    "**/.cargo/**",
    "scripts/ci/**",
    ".github/workflows/**",
    ".github/codeowners",
    "crates/adapters/src/winfs.rs",
    "crates/adapters/src/winfs/**",
    "src-tauri/capabilities/**",
    "**/.claude/settings*.json",
    ".claude/hooks/**",
    ".claude/agents/**",
]


def glob_re(pattern: str):
    out, i = "", 0
    while i < len(pattern):
        if pattern.startswith("**/", i):
            out, i = out + "(?:.*/)?", i + 3
        elif pattern.startswith("/**", i):
            out, i = out + "/.*", i + 3
        elif pattern[i] == "*":
            out, i = out + "[^/]*", i + 1
        else:
            out, i = out + re.escape(pattern[i]), i + 1
    return re.compile(out + r"\Z")


PROTECTED_RE = [(p, glob_re(p)) for p in PROTECTED]


def norm(path: str) -> str:
    return os.path.normcase(os.path.realpath(os.path.expanduser(os.path.expandvars(path))))


def git_root(start: str):
    d = start
    while True:
        if os.path.exists(os.path.join(d, ".git")):
            return d
        parent = os.path.dirname(d)
        if parent == d:
            return None
        d = parent


def user_settings() -> set:
    """Claude Code user settings files, which can disable hooks for every project."""
    homes = {os.path.expanduser("~")}
    for var in ("USERPROFILE", "HOME"):
        if os.environ.get(var):
            homes.add(os.environ[var])
    dirs = {os.path.join(h, ".claude") for h in homes}
    if os.environ.get("CLAUDE_CONFIG_DIR"):
        dirs.add(os.environ["CLAUDE_CONFIG_DIR"])
    return {norm(os.path.join(d, n)) for d in dirs for n in ("settings.json", "settings.local.json")}


def roots(target: str, cwd) -> list:
    """Git root of the session cwd first, then the project dir, then the target's own checkout
    (other worktrees, nested clones). A path is protected if it matches under any of them."""
    out = []
    if cwd:
        out.append(git_root(norm(cwd)) or norm(cwd))
    if os.environ.get("CLAUDE_PROJECT_DIR"):
        out.append(norm(os.environ["CLAUDE_PROJECT_DIR"]))
    own = git_root(os.path.dirname(target))
    if own:
        out.append(own)
    return out


def protected_match(file_path: str, cwd):
    base = cwd or os.environ.get("CLAUDE_PROJECT_DIR") or os.getcwd()
    target = norm(os.path.join(base, os.path.expanduser(file_path)))
    if target in user_settings():
        return target, "user settings (~/.claude/settings*.json)"
    for root in roots(target, cwd):
        try:
            rel = os.path.relpath(target, root)
        except ValueError:  # different drive on Windows
            continue
        rel = rel.replace("\\", "/").lower()
        if rel == ".." or rel.startswith("../"):
            continue
        for pattern, rx in PROTECTED_RE:
            if rx.match(rel):
                return rel, pattern
    return None


def main() -> int:
    data = json.load(sys.stdin)
    tool_input = data.get("tool_input") or {}
    file_path = tool_input.get("file_path") or tool_input.get("notebook_path") or tool_input.get("path")
    if not file_path:
        return 0
    hit = protected_match(file_path, data.get("cwd"))
    if hit is None:
        return 0
    rel, pattern = hit
    if os.environ.get(OVERRIDE_ENV) == "1":
        print(f"Culla hook: {rel} is protected; allowed because {OVERRIDE_ENV}=1.", file=sys.stderr)
        return 0
    print(
        f"Blocked: {rel} is a protected path (rule {pattern!r} in .claude/hooks/protect_paths.py). "
        "It holds lint, CI, capability, removal or agent-safety rules that an agent may not change "
        "on its own. Do not work around this with Bash or another tool. Stop and ask Margaux: say "
        "which file, what change and why. She makes the change herself or starts a session with "
        f"{OVERRIDE_ENV}=1.",
        file=sys.stderr,
    )
    return 2


if __name__ == "__main__":
    sys.exit(main())
