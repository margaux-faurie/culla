---
name: add-command
description: Add a feature or a new Tauri command to Culla by slicing along the hexagon (core, adapter, command, DemoBackend, UI), one slice per PR, test first. Use when starting any new behaviour that crosses core, adapters, src-tauri or the UI.
argument-hint: "[feature or command name]"
---

Add `$ARGUMENTS` along the hexagon. Each step below is its own PR, and each PR starts with a failing test. Read `CLAUDE.md`, `docs/architecture.md` §5 and the story (`US-xx` in `docs/stories.md`) first. If the behaviour is not covered by `docs/SPEC.md` or an ADR, stop and ask Margaux, or record it with the `new-adr` skill as proposed.

1. **Core function + tests** (`crates/core`, package `culla-core`).
   - Pure logic and port traits only: no I/O, no Tauri, no `unsafe`. Errors with `thiserror`.
   - Write the unit or property test first and see it fail (`cargo test -p culla-core <name>`). Anything in plan, preflight, journal, apply or resume names the `INV-xx` it protects (`docs/design/file-engine.md` §11) and uses the fake `FileSystem`.
2. **Adapter** (`crates/adapters`, package `culla-adapters`).
   - Implements the port. File operations go through `winfs` only: no raw `std::fs::rename`, removal, `copy`, `write` or `File::create`. `winfs/` is a protected path: changes there need Margaux.
   - Integration tests under `crates/adapters/tests/`, temp dirs only. Windows-only tests run on `windows-latest`; on Linux say they are left to CI.
3. **Command + capability + bindings** (`src-tauri`, wiring only).
   - A `#[tauri::command]` that calls the core through the adapter and returns `Result<T, IpcError>`. Arguments are opaque IDs, never paths.
   - Add it to `AppManifest::commands` in `build.rs` and grant it in `src-tauri/capabilities/`. Capabilities are a protected path: write the exact change in the PR and ask Margaux to apply it.
   - Run `pnpm gen:bindings`; never edit the generated bindings by hand. Check with `git diff --exit-code` after regenerating.
4. **`DemoBackend`** (`src`).
   - Implement the same method on `DemoBackend` so the web demo and UI tests work without Tauri. Same types as the generated bindings.
5. **UI** (`src`).
   - Use the `Backend` interface only, never `invoke` directly. Keyboard reachable; copy in plain sentence case with no exclamation marks. Vitest first, Playwright and axe when a screen changes.

Each PR:
- Conventional Commit title, one slice, the story or issue linked.
- Evidence: paste the commands you ran and their output (`cargo test`, clippy `-D warnings`, `pnpm check`, `pnpm test` as relevant).
- `CLAUDE.md` updated if a command, the layout, an invariant or a convention changed.
- Ask the read-only reviewers: `spec-reviewer` always, `safety-reviewer` for anything touching files, `security-reviewer` for steps 3–5. Fix findings in the same PR.
