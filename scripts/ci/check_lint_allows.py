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
REMOVAL_LINTS = re.compile(r"\b(?:clippy::)?disallowed_methods\b|\bclippy::(?:style|all)\b")
UNSAFE_LINT = re.compile(r"\bunsafe_code\b")


def tracked(*patterns: str) -> list[str]:
    out = subprocess.run(["git", "ls-files", "--", *patterns], check=True, capture_output=True, text=True)
    return [p for p in out.stdout.splitlines() if p]


def strip_comments(src: str) -> str:
    src = re.sub(r"/\*.*?\*/", "", src, flags=re.S)
    return re.sub(r"//[^\n]*", "", src)


def main() -> int:
    problems: list[str] = []
    for path in tracked("*.rs"):
        with open(path, encoding="utf-8") as f:
            src = strip_comments(f.read())
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
