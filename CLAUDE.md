# Culla — agent instructions

Culla is a Windows desktop app (Tauri 2 + Rust + React/TS) that culls Nikon JPEG+NEF photos with a swipe UI, then applies keep/reject as safe, journaled file operations. Product: `docs/PRD.md`. Architecture: `docs/architecture.md`. File engine: `docs/design/file-engine.md`. Decisions: `docs/adr/`.

## Status
Pre-code: documentation phase. Commands below are the target; they work once milestone M0 (`docs/roadmap.md`) lands. Update this file in the PR that makes them real.

## Commands (target)
- `pnpm install` · `pnpm check` (Biome + tsc) · `pnpm test` · `pnpm test <file>`
- `cargo test -p culla-core` · `cargo test -p culla-core <name>` · `cargo clippy --all-targets -- -D warnings` · `cargo deny check`
- `pnpm tauri dev` (desktop) · `pnpm dev:demo` (web demo) · `pnpm gen:bindings` (TS bindings from Rust)

## Layout
- `crates/core` — pure Rust domain + ports. No Tauri dependency, `#![forbid(unsafe_code)]`.
- `src-tauri` — adapters (fs, images, SQLite), commands, `culla://` scheme, capabilities.
- `src` — React UI, one Vite app with two build modes (desktop, demo); `Backend` interface with `TauriBackend` and `DemoBackend`.
- `docs` — specs, ADRs, research.

## Invariants (never break; tests and lints enforce them)
- Nothing touches the user's files before the user confirms Apply.
- Never overwrite a file. Never delete a file, except the source of a verified cross-volume move, and only in the single audited function described in `docs/design/file-engine.md`.
- Undecided photos are never touched.
- The webview gets no filesystem permissions; it only knows opaque photo IDs.

## How to add a feature
Slice along the hexagon, one slice per PR, test first: core function + tests → adapter → command + capability + regenerated bindings → `DemoBackend` → UI.

## Conventions
- Errors: `thiserror` in core; commands return typed `IpcError` (serde tagged). No `anyhow` in library code.
- Conventional Commits. A changed decision needs a new or superseding ADR (`docs/adr/`, MADR minimal).
- Docs and UI copy: plain, sentence case, no exclamation marks.

## Gotchas
- `std::fs::rename` overwrites on Windows. Use the engine's checked rename, never the raw call.
- Generated bindings are not edited by hand.

## Review and merge
Every PR gets an independent read-only reviewer agent (correctness, safety invariants, spec consistency; skip style) plus green CI. When both pass, the PR is squash-merged and work continues. Findings are fixed in the same PR and re-reviewed. Releases and anything irreversible outside the repo wait for Margaux.

## Keeping this file current
Update CLAUDE.md in the same PR whenever a command, the layout, an invariant or a convention changes, or when you hit a trap a future session would fall into too. Keep it short (under ~60 lines): move procedures to `.claude/skills/`, and rules that must always hold to hooks or lints. Remove lines that are no longer true.

## Done means
Tests, clippy, Biome and tsc green; new behaviour covered by a test; docs, ADR and CLAUDE.md updated if a decision or workflow changed. See `docs/testing.md`.
