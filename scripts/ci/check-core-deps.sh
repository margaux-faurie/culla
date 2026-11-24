#!/usr/bin/env bash
# culla-core is pure: no I/O, no async runtime, no Tauri, no file-system crates (ADR 0002).
set -euo pipefail
banned='^(tauri|tokio|async-std|rusqlite|libsqlite3-sys|walkdir|ignore|tempfile|memmap2|windows|windows-sys|winapi|reqwest|hyper|turbojpeg|image)$'
deps=$(cargo tree -p culla-core --edges normal --prefix none --format '{p}' --locked | awk '{print $1}' | sort -u)
bad=$(echo "$deps" | grep -E "$banned" || true)
if [ -n "$bad" ]; then
  echo "culla-core depends on crates it must not use:" >&2
  echo "$bad" >&2
  exit 1
fi
echo "culla-core dependencies are clean: $(echo "$deps" | tr '\n' ' ')"
