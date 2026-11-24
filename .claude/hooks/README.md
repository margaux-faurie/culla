# Claude Code hooks

Wired in `../settings.json`. Each hook runs through `py.sh`, which picks a working Python 3.

| Hook | Script | What it does |
|---|---|---|
| PreToolUse on Edit, Write, MultiEdit, NotebookEdit | `protect_paths.py` | Blocks edits to protected paths (list in the script) |
| PostToolUse on the same tools | `format_file.py` | rustfmt on `.rs` (that file only), Biome on TS, JS, JSON, CSS once installed |
| Stop | `stop_check.py` | Fast check from `docs/testing.md` §11; exit 2 sends Claude back to work, at most 3 times in a row |

## Requirements

- Python 3.8 or later (`python3`, `python` or `py -3`). The CI script `check_lint_allows.py` needs 3.11.
- Windows: Git for Windows, with Git Bash where Claude Code finds it (on `PATH`, or set `CLAUDE_CODE_GIT_BASH_PATH`). Without it hooks run in PowerShell and these commands fail.
- Claude Code v2.1.295 or later for `onFailure: "block"`, which makes the protected-path hook fail closed. On older versions a crashing hook lets the edit through.
- Hooks start only after the folder is trusted in an interactive session.

## Override

To change a protected path, Margaux makes the change herself or starts a session with `CULLA_ALLOW_PROTECTED_EDITS=1` in the environment of the Claude Code process.

## Limits

These hooks are guard rails for well-meaning agents, not a security boundary. A determined agent can still change a protected file through Bash (`sed -i`, redirection, a script), and Bash deny rules match command text only. The backstop is CI (`check_lint_allows.py`, clippy, `cargo deny`), branch protection with CODEOWNERS review on protected paths, and the read-only reviewer agents.
