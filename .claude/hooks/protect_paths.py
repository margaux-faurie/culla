"""PreToolUse hook (Edit|Write|MultiEdit): block edits to protected paths.

Protected paths hold the rules that keep agents honest: lint and deny config, CI scripts and
workflows, the audited removal module, Tauri capabilities and the agent settings and hooks
themselves (design/file-engine.md §9.7). An agent may not change them on its own.

Override: Margaux starts Claude Code with CULLA_ALLOW_PROTECTED_EDITS=1 in its environment.
The agent cannot set that variable for the Claude Code process, so it stays a human decision.

Exit 2 blocks the call and stderr goes to Claude. Any crash exits non-zero, and the hook is
registered with onFailure "block", so a broken hook blocks rather than lets edits through.
"""
import json
import os
import sys

OVERRIDE_ENV = "CULLA_ALLOW_PROTECTED_EDITS"

# Matched against the path relative to a repo root, with "/" separators, case-folded.
# "dir/**" matches everything under dir; "**/name" matches name at any depth.
PROTECTED = [
    "**/clippy.toml",
    "**/.clippy.toml",
    "deny.toml",
    "scripts/ci/**",
    ".github/workflows/**",
    ".github/codeowners",
    "crates/adapters/src/winfs.rs",
    "crates/adapters/src/winfs/**",
    "src-tauri/capabilities/**",
    ".claude/settings.json",
    ".claude/settings.local.json",
    ".claude/hooks/**",
]


def norm(path: str) -> str:
    return os.path.normcase(os.path.realpath(path))


def matches(rel: str, pattern: str) -> bool:
    if pattern.startswith("**/"):
        name = pattern[3:]
        return rel == name or rel.endswith("/" + name)
    if pattern.endswith("/**"):
        return rel.startswith(pattern[:-2])
    return rel == pattern


def repo_roots(target: str, data: dict) -> list:
    """Every root the path could be relative to: the project, the session cwd, and the
    nearest enclosing git checkout (covers other worktrees and nested clones)."""
    roots = []
    for candidate in (os.environ.get("CLAUDE_PROJECT_DIR"), data.get("cwd")):
        if candidate:
            roots.append(norm(candidate))
    d = os.path.dirname(target)
    while True:
        if os.path.exists(os.path.join(d, ".git")):
            roots.append(norm(d))
            break
        parent = os.path.dirname(d)
        if parent == d:
            break
        d = parent
    return roots


def protected_match(file_path: str, data: dict):
    base = data.get("cwd") or os.environ.get("CLAUDE_PROJECT_DIR") or os.getcwd()
    target = norm(os.path.join(base, file_path))
    for root in repo_roots(target, data):
        try:
            rel = os.path.relpath(target, root)
        except ValueError:  # different drive on Windows
            continue
        rel = rel.replace("\\", "/").lower()
        if rel == ".." or rel.startswith("../"):
            continue
        for pattern in PROTECTED:
            if matches(rel, pattern):
                return rel, pattern
    return None


def main() -> int:
    data = json.load(sys.stdin)
    tool_input = data.get("tool_input") or {}
    file_path = tool_input.get("file_path") or tool_input.get("path")
    if not file_path:
        return 0
    hit = protected_match(file_path, data)
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
        "which file, what change and why. She makes the change herself or restarts Claude Code with "
        f"{OVERRIDE_ENV}=1.",
        file=sys.stderr,
    )
    return 2


if __name__ == "__main__":
    sys.exit(main())
