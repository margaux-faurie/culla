#!/usr/bin/env bash
# culla-core is pure (ADR 0002): its dependencies, on every target, must be on this allowlist.
# Fails closed: a new dependency needs a deliberate edit here, reviewed with the PR.
set -euo pipefail
allowed='^(culla-core)$'
deps=$(cargo tree -p culla-core --target all --edges normal,build --prefix none --format '{p}' --locked | awk '{print $1}' | sort -u)
bad=$(echo "$deps" | grep -Ev "$allowed" || true)
if [ -n "$bad" ]; then
  echo "culla-core depends on crates not on its allowlist (scripts/ci/check-core-deps.sh):" >&2
  echo "$bad" >&2
  exit 1
fi
echo "culla-core dependencies are on the allowlist: $(echo "$deps" | tr '\n' ' ')"
