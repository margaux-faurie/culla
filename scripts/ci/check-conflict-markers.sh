#!/usr/bin/env bash
# Fails if any tracked text file contains merge-conflict markers.
set -euo pipefail
if git grep -nI -E '^(<<<<<<<( |$)|=======$|>>>>>>>( |$))' -- . ':!scripts/ci/check-conflict-markers.sh'; then
  echo "Merge-conflict markers found (see above)." >&2
  exit 1
fi
echo "No merge-conflict markers."
