#!/usr/bin/env python3
"""Lint switches for the removal rule and unsafe code may only appear in permitted places.

See design/file-engine.md §9.7 and ADR 0020. Attributes are matched whole, across lines,
so the multi-line form rustfmt produces for long `reason = "..."` values is caught too.
Every member crate must inherit the workspace lints and must not override them.
"""
import re
import subprocess
import sys

LINT_ALLOWED = re.compile(r"^(crates/bench/|crates/adapters/src/winfs/|crates/[a-z-]+/tests/)")
UNSAFE_ALLOWED = re.compile(r"^crates/adapters/src/winfs/")
# #[allow(...)], #![expect(...)], #[cfg_attr(cond, warn(...))], across lines.
ATTR = re.compile(
    r"#!?\[\s*(?:cfg_attr\s*\((?:[^()]|\([^()]*\))*?,\s*)?(allow|expect|warn)\s*\(((?:[^()]|\([^()]*\))*)\)",
    re.S,
)
REMOVAL_LINTS = re.compile(
    r"\b(?:clippy::)?disallowed_methods?\b|\bclippy::(?:style|all)\b|\brenamed_and_removed_lints\b"
)
# Product code must not pull in source files from elsewhere (it would escape the path rules).
FOREIGN_SOURCE = re.compile(r"#!?\[[^\]]*\bpath\s*=|\binclude(?:_str)?\b")
UNSAFE_LINT = re.compile(r"\bunsafe_code\b")


def tracked(*patterns: str) -> list[str]:
    out = subprocess.run(["git", "ls-files", "--", *patterns], check=True, capture_output=True, text=True)
    return [p for p in out.stdout.splitlines() if p]


def strip_comments(src: str) -> str:
    """Blank out string/char literals, then comments, keeping line numbers.

    Literals are replaced by empty quotes first, so a "*/" or "//" inside a string can't
    hide or truncate real code.
    """
    out = []
    i, n = 0, len(src)
    while i < n:
        c = src[i]
        if src.startswith("//", i):
            j = src.find("\n", i)
            i = n if j == -1 else j
        elif src.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if src.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif src.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            out.append("\n" * src.count("\n", i, j))
            i = j
        elif c == "r" and re.match(r'r#*"', src[i:]):
            m = re.match(r'r(#*)"', src[i:])
            end = src.find('"' + m.group(1), i + len(m.group(0)))
            end = n if end == -1 else end + 1 + len(m.group(1))
            out.append('""' + "\n" * src.count("\n", i, end))
            i = end
        elif c == '"':
            j = i + 1
            while j < n and src[j] != '"':
                j += 2 if src[j] == "\\" else 1
            out.append('""' + "\n" * src.count("\n", i, j))
            i = j + 1
        elif c == "'" and re.match(r"'(?:\\.|[^\\'\n])'", src[i:]):
            m = re.match(r"'(?:\\.|[^\\'\n])'", src[i:])
            out.append("' '")
            i += len(m.group(0))
        else:
            out.append(c)
            i += 1
    return "".join(out)


def main() -> int:
    problems: list[str] = []
    for path in tracked("*.rs"):
        with open(path, encoding="utf-8") as f:
            src = strip_comments(f.read())
        if not re.match(r"^crates/[a-z-]+/tests/", path):
            for m in FOREIGN_SOURCE.finditer(src):
                line = src.count("\n", 0, m.start()) + 1
                problems.append(f"{path}:{line}: #[path] (also inside cfg_attr) / include or include_str is not allowed outside crates/*/tests/")
        for m in ATTR.finditer(src):
            lints = m.group(2)
            line = src.count("\n", 0, m.start()) + 1
            if REMOVAL_LINTS.search(lints) and not LINT_ALLOWED.match(path):
                problems.append(f"{path}:{line}: removal lint switched off outside winfs/, bench/ or crates/*/tests/")
            if UNSAFE_LINT.search(lints) and not UNSAFE_ALLOWED.match(path):
                problems.append(f"{path}:{line}: unsafe_code allowed outside crates/adapters/src/winfs/")

    for manifest in tracked("crates/*/Cargo.toml", "src-tauri/Cargo.toml"):
        with open(manifest, encoding="utf-8") as f:
            text = f.read()
        section = re.search(r"^\[lints\]\s*\n(.*?)(?=^\[|\Z)", text, re.S | re.M)
        if not section or not re.search(r"^workspace\s*=\s*true\s*$", section.group(1), re.M):
            problems.append(f"{manifest}: missing [lints] workspace = true")
        if re.search(r"^\[lints\.", text, re.M) or "disallowed" in text:
            problems.append(f"{manifest}: overrides lints; only [lints] workspace = true is allowed")

    # crates/core/clippy.toml replaces the root file for culla-core; it must keep every root rule.
    def paths(f: str) -> set[str]:
        with open(f, encoding="utf-8") as fh:
            body = fh.read().split("disallowed-types")[0]
        return set(re.findall(r'path\s*=\s*"([^"]+)"', body))
    missing = paths("clippy.toml") - paths("crates/core/clippy.toml")
    if missing:
        problems.append("crates/core/clippy.toml is missing root rules: " + ", ".join(sorted(missing)))

    for p in problems:
        print(p, file=sys.stderr)
    if not problems:
        print("Lint switches and crate lint settings are within the permitted places.")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
