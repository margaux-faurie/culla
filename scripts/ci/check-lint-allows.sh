#!/usr/bin/env bash
# The removal/rename lint and unsafe code may only be switched off in permitted places
# (design/file-engine.md §9.7, ADR 0020). Every member crate must inherit the workspace lints.
set -euo pipefail
status=0

lint_allowed='^(crates/bench/|crates/adapters/src/winfs/|crates/[a-z-]+/tests/)'
pattern='(allow|expect|warn)[[:space:]]*\([^)]*\b(clippy::)?(disallowed_methods|style|all)\b'
bad=$(git grep -l -E "$pattern" -- '*.rs' | grep -Ev "$lint_allowed" || true)
if [ -n "$bad" ]; then
  echo "clippy::disallowed_methods (or a group containing it) is switched off outside the permitted places:" >&2
  echo "$bad" >&2
  status=1
fi

unsafe_allowed='^crates/adapters/src/winfs/'
bad=$(git grep -l -E '(allow|expect|warn)[[:space:]]*\([^)]*\bunsafe_code\b' -- '*.rs' | grep -Ev "$unsafe_allowed" || true)
if [ -n "$bad" ]; then
  echo "unsafe_code is allowed outside crates/adapters/src/winfs/:" >&2
  echo "$bad" >&2
  status=1
fi

for manifest in $(git ls-files 'crates/*/Cargo.toml' 'src-tauri/Cargo.toml'); do
  if ! awk '/^\[lints\]/{f=1;next} /^\[/{f=0} f&&/^workspace[[:space:]]*=[[:space:]]*true/{ok=1} END{exit !ok}' "$manifest"; then
    echo "$manifest does not inherit the workspace lints ([lints] workspace = true)." >&2
    status=1
  fi
  if grep -Eq '^\[lints\.|disallowed' "$manifest"; then
    echo "$manifest overrides lints; only [lints] workspace = true is allowed." >&2
    status=1
  fi
done

[ "$status" -eq 0 ] && echo "Lint allows and crate lint settings are within the permitted places."
exit "$status"
