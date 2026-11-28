# Culla — agent instructions

Culla is a Windows desktop app (Tauri 2 + Rust + React/TS) that culls Nikon JPEG+NEF photos with a swipe UI, then applies keep/reject as safe, journaled file operations. Start at `docs/SPEC.md`: it records the author's decisions and maps each topic to its owning doc (PRD, nfr, architecture, ADRs, file-engine design, threat model, testing, roadmap). `docs/research/` is historical input: where it disagrees with the spec or an ADR, the spec and ADRs win.

## Status
M0 (repo and tooling) is done; M1 (core engine headless) is in progress: NEF preview reader, scan and grouping, EXIF, volume identity, the preview decode and the preview pipeline (threads, prefetch, in-memory cache) are in, with the headless `culla scan` and `culla preview` and the `culla-bench` harness for the M1 campaign. M2 (file engine) has started: the pure decision log, `plan()`, preflight, the journal executor and the apply log's rendering are in, with the dry-run `culla plan`. M3 has started on the UI, demo backend only: start screen with the route view, swipe screen, TS decision reducer (`src/decisions`). M4 has started on the same demo backend: review screen and confirm dialog (`src/review`, the demo's plan and preflight in `backend/demo-plan.ts`, scenarios by `?scenario=` in `backend/demo-scenarios.ts`), apply screen and report (`src/apply`, the simulated run in `backend/demo-apply.ts`, its log in `backend/demo-log.ts`). Author decisions recorded 27 Nov; what is still open is in SPEC §10 and file-engine §13.

## Commands
Work now:
- `cargo test --workspace` · `cargo test -p culla-core <name>` · `cargo test -p culla-adapters [--features windows-it]` (Windows integration tests from M2) · `CULLA_REAL_SAMPLES=<dir> cargo test -p culla-adapters --test real_samples -- --ignored` (the author's NEFs stay outside the repo: they hold GPS; same gate for `-p culla-cli --test scan`, `--test preview` and `--test plan`) · `cargo run -p culla-cli -- scan <folder> [--no-subfolders] [--json]` (read-only; `culla help` lists exit codes) · `cargo run -p culla-cli -- preview <folder> [--no-subfolders]` (per-step timings, read-only; to eyeball previews use `CULLA_PREVIEW_OUT` with the adapters' `real_samples` test) · `cargo run -p culla-cli -- plan <folder> --decisions <log-file> [--keep move|copy --dest <folder>] [--raw-follows yes|no] [--json]` (dry run: plan and preflight, read-only; group ids are `culla scan` positions)
- `cargo clippy --workspace --all-targets -- -D warnings` · `cargo fmt --all` · `cargo deny check` (`culla-app` needs WebView2 on Windows or libwebkit2gtk-4.1-dev on Linux)
- `scripts/ci/check-conflict-markers.sh` · `scripts/ci/check_lint_allows.py` · `pwsh scripts/ci/vhdx.ps1 -Action mount|attach|detach|readonly|unmount` (Windows, admin; see `scripts/ci/README.md`)
- `pnpm install` · `pnpm check` (Biome + tsc) · `pnpm test` · `pnpm test <file>`
- `pnpm dev:demo` · `pnpm build:demo` (web demo, `dist-demo/`) · `pnpm tauri dev` · `pnpm tauri build` (Windows, NSIS)
- `pnpm build:demo && pnpm e2e` (Playwright + axe on `dist-demo/`; first time `pnpm exec playwright install chromium`)
- `pnpm gen:bindings` (placeholder until the first tauri-specta command in M3)
- `cargo xtask fixtures [--check]` · `cargo xtask golden [--check]` (writes missing decision-log cases, never overwrites; a new case must also be listed in `crates/core/tests/golden.rs`) · `cargo xtask crash-loop --iterations N` (M2)
- Dependencies changed: run the `Refresh lockfiles` workflow on the PR branch (`gh api -X POST repos/margaux-faurie/culla/actions/workflows/lockfile-refresh.yml/dispatches -f ref=main -f inputs[branch]=<branch>`), then push any commit to rerun CI
- `cargo llvm-cov -p culla-core -p culla-adapters --html` · `cargo +nightly fuzz run tiff_ifd_walk -- -max_total_time=60` (setup: testing.md §7, §10) · `cargo build --release -p culla-bench` then `culla-bench scan|preview|swipe|io-baseline <folder> [--json]`, `culla-bench report <result.json>...` (`culla-bench help`; debug-build timings mean nothing)

## Layout
- `crates/core` (package `culla-core`) — domain, ports, pure logic (TIFF/IFD walker behind a `ReadAt` port; `grouping` and scan exclusions over plain `ScanEntry` data; `exif` parsing and capture order over plain `RawExif` data; `names`; `volume`: serial + folded relative path keys, run binding; `preview`: sizes, scale, pixel cap, orientation, working bytes; `prefetch`: window, scheduler, LRU, memory budgets; `decisions`: event log, `fold`, undo/redo, reconcile clears, text form; `plan`: decided groups to operations over a snapshot of the target roots; `preflight`: P1–P17 over a plan and a snapshot of facts; `journal`: op states, recovery table, one executor for apply and resume over `Disk`/`Store` ports; `applylog`: renders the plain-text apply log from the journal; `text`: the shared escape set for untrusted names, used by the CLI and the apply log). Property tests: seeded plain loops instead of proptest (proposed, SPEC §10), `CULLA_PROP_CASES`. No I/O, no Tauri, `#![forbid(unsafe_code)]`.
- `crates/adapters` (package `culla-adapters`) — Windows file ops, the read-only folder walk (`scan`, `read_dir` only, re-checks each folder before listing it, swap window until M2, never a partial folder; moves into `winfs` in M2), file reads for NEF previews, EXIF, `preview` decode (turbojpeg builds libjpeg-turbo: needs CMake, and NASM on `PATH` for SIMD, else it silently builds slow plain C), `pipeline` (reader thread, decode workers, in-memory cache; std threads only), `facts` (read-only lookups for the dry-run plan and preflight: path facts, target snapshot, the decision log file), SQLite; `winfs` (Win32 `unsafe` only in `winfs/win32.rs`, read-only volume and time-zone lookups so far, a stub off Windows). No Tauri.
- `crates/cli` (package `culla-cli`, binary `culla`) — headless scan (in; exit codes, JSON fields and off-Windows behaviour provisional, SPEC §10), preview (in; decodes, times and reports), plan (in; dry run, exit codes 0 to 5 provisional), apply, resume. Wiring only; writes nothing but stdout and stderr.
- `crates/bench` (package `culla-bench`) — benchmark harness (ADR 0020): scan, preview, swipe through the pipeline, io-baseline, report; read-only, writes nothing but stdout, std only, `--json` unstable. Campaign: docs/benchmarks.md §7.
- `src-tauri` (package `culla-app`) — wiring only: commands, `culla://` image scheme, capabilities. New command: `build.rs` `AppManifest::commands` + `allow-<command>` in `capabilities/main.json`.
- `src` — React UI, one Vite app, `--mode desktop` or `--mode demo`; `vite.config.ts` aliases `#backend-impl` to `backend/tauri.ts` or `backend/demo.ts` (ADR 0014); `decisions` (event log fold, golden-tested by `tests/golden.test.ts`), `route` (the route view: it prints the display paths the backend computes, `backend/demo-routes.ts` in the demo, and never builds one), `start`, `swipe` (key map, reducer, screen), `review` (screen, confirm dialog, `model.ts` words the backend's plan), `apply` (progress, pause, stop, report, log dialog; `model.ts` words the backend's status), `session`. Logic stays in pure `.ts` modules with Vitest tests (no DOM in Vitest); screens are covered by Playwright on the demo build. The demo deploys to Cloudflare on every push to main (`wrangler.jsonc`, CSP and headers in `public-demo/_headers`, copied in demo mode only; docs/deploy-demo.md). Proposed, to confirm (SPEC §10): those files count as CI and wait for Margaux's OK; not enforced by CODEOWNERS or the hook.
- `xtask` — dev tasks, std only, under the workspace file rules: `fixtures` never overwrites, so delete `testdata/synthetic/` by hand to regenerate. `fuzz` — cargo-fuzz, own workspace. `testdata` — synthetic and golden fixtures.
- `docs` — specs, ADRs, research.

## Invariants (never break; lints enforce the removal and rename rules now; tests for the rest arrive with M2 and M3)
- Nothing touches the user's files before the user confirms Apply.
- Never overwrite a file: renames are no-replace (an existing target is a hard error, never replaced; mechanism in design/file-engine.md §9.3, ADR 0022), copies use `create_new`.
- Never remove a file, except in the audited module `culla-adapters::winfs::remove_verified` (`remove_verified_source`, `remove_own_partial`; design/file-engine.md §9.7, §11), which removes only the source of a cross-volume move whose destination and source were re-read and hash-verified, and Culla's own `*.culla-partial` files. The lint covers `culla-core`, `culla-adapters`, `culla-cli` and `src-tauri`; the lint can be switched off only in `winfs/`, `crates/bench/` and `crates/*/tests/` (temp dirs only); inline `#[cfg(test)]` code is not exempt.
- Undecided photos are never touched.
- The webview gets no filesystem permissions; it only knows opaque photo IDs.

## How to add a feature
Slice along the hexagon, one slice per PR, test first: core function + tests → adapter → command + capability + regenerated bindings → `DemoBackend` → UI.

## Conventions
- Errors: `thiserror` in core; commands return typed `IpcError` (serde tagged). No `anyhow` in library code.
- Licence GPL-3.0-only; the name and logo are reserved; outside PRs are not merged (ADR 0026, CONTRIBUTING.md). Conventional Commits. A changed decision needs a new or superseding ADR (`docs/adr/`, MADR minimal).
- Docs and UI copy: plain, sentence case, no exclamation marks.

## Gotchas
- `std::fs::rename` overwrites on Windows. Use the adapters' no-replace rename, never the raw call.
- Generated bindings are not edited by hand.
- A hook blocks edits to protected paths (clippy and deny config, `.cargo/`, `scripts/ci/`, workflows, `winfs/`, capabilities, `.claude/` settings, hooks and agents). Don't work around it: ask Margaux, who can start a session with `CULLA_ALLOW_PROTECTED_EDITS=1`.
- `culla-app` has no test targets: Windows test binaries lack the app manifest Tauri needs and fail to load.

## Review and merge
Every PR gets an independent read-only reviewer agent (correctness, safety invariants, spec consistency; skip style) plus green CI. When both pass, the PR is squash-merged and work continues. Findings are fixed in the same PR and re-reviewed. Releases and anything irreversible outside the repo wait for Margaux. Margaux is a required code owner for every protected path (`.github/CODEOWNERS`); the main ruleset requires code-owner review, and PRs touching src-tauri, winfs, CI or dependencies wait for Margaux's OK in chat before merge (her decision of 27 Nov). The list matches the hook's, under Gotchas.
Reviewers in `.claude/agents/` (read-only): `spec-reviewer` always, `safety-reviewer` for file operations, `security-reviewer` for src-tauri, IPC, deps and CI; pass them the diff. Skills: `add-command`, `new-adr`, `release` (Margaux only). Hooks in `.claude/hooks/` format each edit and run the fast check at Stop; they need Python 3, Git Bash on Windows and Claude Code ≥ 2.1.295 (see its README).

## Keeping this file current
Update CLAUDE.md in the same PR whenever a command, the layout, an invariant or a convention changes, or when you hit a trap a future session would fall into too. Keep it short (under ~60 lines): move procedures to `.claude/skills/`, and rules that must always hold to hooks or lints. Remove lines that are no longer true.

## Done means
Tests, clippy, Biome and tsc green; new behaviour covered by a test; docs, ADR and CLAUDE.md updated if a decision or workflow changed. See `docs/testing.md`.
