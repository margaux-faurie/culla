# Culla — agent instructions

Culla is a Windows desktop app (Tauri 2 + Rust + React/TS) that culls Nikon JPEG+NEF photos with a swipe UI, then applies keep/reject as safe, journaled file operations. Start at `docs/SPEC.md`: it records the author's decisions and maps each topic to its owning doc (PRD, nfr, architecture, ADRs, file-engine design, threat model, testing, roadmap). `docs/research/` is historical input: where it disagrees with the spec or an ADR, the spec and ADRs win.

## Status
Pre-code: design docs drafted (24 Nov); open decisions in SPEC §10 and file-engine §13. Next is milestone M0. M0 is landing in slices; move a command from Target to Work now in the PR that makes it real.

## Commands
Work now (M0, Rust side):
- `cargo test --workspace` · `cargo test -p culla-core <name>` · `cargo test -p culla-adapters` (Windows-specific tests from M2)
- `cargo clippy --workspace --all-targets -- -D warnings` · `cargo fmt --all` · `cargo deny check`
- `scripts/ci/check-conflict-markers.sh` · `scripts/ci/check-lint-allows.sh`

Target (rest of M0, JS and app side):
- `pnpm install` · `pnpm check` (Biome + tsc) · `pnpm test` · `pnpm test <file>`
- `pnpm tauri dev` (desktop) · `pnpm dev:demo` (web demo) · `pnpm gen:bindings` (TS bindings from Rust)

## Layout
- `crates/core` (package `culla-core`) — domain, ports, pure logic. No I/O, no Tauri, `#![forbid(unsafe_code)]`.
- `crates/adapters` (package `culla-adapters`) — Windows file ops, EXIF/NEF parsing, decode, SQLite. No Tauri.
- `crates/cli` (package `culla-cli`, binary `culla`) — headless scan, preview, plan, apply, resume. Wiring only.
- `crates/bench` (package `culla-bench`) — benchmark harness. See ADR 0020.
- `src-tauri` — wiring only: commands, `culla://` image scheme, capabilities.
- `src` — React UI, one Vite app with two build modes (desktop, demo); `Backend` interface with `TauriBackend` and `DemoBackend`.
- `docs` — specs, ADRs, research.

## Invariants (never break; tests and lints will enforce them from M0)
- Nothing touches the user's files before the user confirms Apply.
- Never overwrite a file: renames are no-replace (an existing target is a hard error, never replaced; mechanism in design/file-engine.md §9.3, ADR 0022 proposed), copies use `create_new`.
- Never remove a file, except in the audited module `culla-adapters::winfs::remove_verified` (`remove_verified_source`, `remove_own_partial`; design/file-engine.md §9.7, §11), which removes only the source of a cross-volume move whose destination and source were re-read and hash-verified, and Culla's own `*.culla-partial` files. The lint covers `culla-core`, `culla-adapters`, `culla-cli` and `src-tauri`; the lint can be switched off only in `winfs/`, `crates/bench/` and `crates/*/tests/` (temp dirs only); inline `#[cfg(test)]` code is not exempt.
- Undecided photos are never touched.
- The webview gets no filesystem permissions; it only knows opaque photo IDs.

## How to add a feature
Slice along the hexagon, one slice per PR, test first: core function + tests → adapter → command + capability + regenerated bindings → `DemoBackend` → UI.

## Conventions
- Errors: `thiserror` in core; commands return typed `IpcError` (serde tagged). No `anyhow` in library code.
- Conventional Commits. A changed decision needs a new or superseding ADR (`docs/adr/`, MADR minimal).
- Docs and UI copy: plain, sentence case, no exclamation marks.

## Gotchas
- `std::fs::rename` overwrites on Windows. Use the adapters' no-replace rename, never the raw call.
- Generated bindings are not edited by hand.

## Review and merge
Every PR gets an independent read-only reviewer agent (correctness, safety invariants, spec consistency; skip style) plus green CI. When both pass, the PR is squash-merged and work continues. Findings are fixed in the same PR and re-reviewed. Releases and anything irreversible outside the repo wait for Margaux.

## Keeping this file current
Update CLAUDE.md in the same PR whenever a command, the layout, an invariant or a convention changes, or when you hit a trap a future session would fall into too. Keep it short (under ~60 lines): move procedures to `.claude/skills/`, and rules that must always hold to hooks or lints. Remove lines that are no longer true.

## Done means
Tests, clippy, Biome and tsc green; new behaviour covered by a test; docs, ADR and CLAUDE.md updated if a decision or workflow changed. See `docs/testing.md`.
