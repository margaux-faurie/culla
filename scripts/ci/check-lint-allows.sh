#!/usr/bin/env bash
# The removal/rename lint may only be switched off in the bench crate and, from M2,
# in the audited winfs module (design/file-engine.md §9.7, ADR 0020).
set -euo pipefail
allowed='^(crates/bench/|crates/adapters/src/winfs/|crates/[a-z]+/tests/)'
bad=$(git grep -l -E 'allow\((clippy::)?disallowed_methods\)' -- '*.rs' | grep -Ev "$allowed" || true)
if [ -n "$bad" ]; then
  echo "clippy::disallowed_methods is allowed outside the permitted places:" >&2
  echo "$bad" >&2
  exit 1
fi
echo "disallowed_methods allows are confined to permitted places."
