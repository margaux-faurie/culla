#!/usr/bin/env bash
# Runs a hook script from this folder with the first working Python 3.
# Usage: bash .claude/hooks/py.sh <script.py>   (hook JSON arrives on stdin and is passed through)
# On Windows (Git Bash), `python3` is often the Microsoft Store stub, so each candidate is probed.
# If no Python is found the script exits 1: PreToolUse hooks set onFailure "block", so the
# protected-path check fails closed.
set -u
here="$(cd "$(dirname "$0")" && pwd)"
script="$here/$1"
shift

ok='import sys; sys.exit(0 if sys.version_info >= (3, 8) else 1)'
for py in python3 python; do
  if command -v "$py" >/dev/null 2>&1 && "$py" -c "$ok" </dev/null >/dev/null 2>&1; then
    exec "$py" -I -B "$script" "$@"
  fi
done
if command -v py >/dev/null 2>&1 && py -3 -c "$ok" </dev/null >/dev/null 2>&1; then
  exec py -3 -I -B "$script" "$@"
fi

echo "Culla hook: no Python 3 found (tried python3, python, py -3); $1 did not run." >&2
exit 1
