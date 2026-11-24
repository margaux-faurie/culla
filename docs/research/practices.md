> Historical input, written before the decisions were settled. Where it disagrees with `docs/SPEC.md` or an ADR, those win.

# Culla: practices research (Claude Code, security, architecture)

Researched 2026-11-24. Links are the pages actually read.

## 1. Building with Claude Code agents

What the current docs say:
- **CLAUDE.md stays short.** It only holds what Claude can't work out from the code: exact test/typecheck/lint commands (including how to run one test), misleading spots, decisions not in the repo, and things you've had to correct twice. Personal notes go in `CLAUDE.local.md`, which is gitignored ([best practices](https://code.claude.com/docs/en/best-practices)).
- **Guidelines vs. enforcement.** CLAUDE.md is a guideline the model can decide to skip. Anything that must hold every time belongs in a hook, a deny rule or a managed setting. A PostToolUse hook formats after each Edit/Write. A `Stop` hook that exits with code 2 sends Claude back to work until the check passes (it gives up after 8 blocks in a row). Deny rules apply in every permission mode. Keep the enforced set small (same page).
- **Skills** (`.claude/skills/<name>/SKILL.md`) hold procedures and only load when needed. Use `disable-model-invocation: true` for anything with side effects, such as a release. **Subagents** in `.claude/agents/` with `tools: Read, Grep, Glob` give you reviewers that can't edit, and that limit is enforced. A reviewer prompt should end with "skip style" (same page).
- **Workflow.** Have Claude interview you until it can write a spec, then build from that spec in a *fresh* session. Give it a check to run and ask for evidence (the command and its output). Use `/clear` between unrelated tasks, and after two failed corrections start over with a better prompt. Use worktrees (`claude -w`) for parallel work. The current page no longer leads with "explore→plan→code→commit". It talks about context, checks and enforcement instead. Plan mode is still the "plan" step.
- **Long runs** ([effective harnesses](https://anthropic.com/engineering/effective-harnesses-for-long-running-agents)): an initializer session writes a feature list in JSON with every item failing, plus a progress file. Each session then does one feature, verifies it end to end, commits, and leaves the repo safe to merge. [Context engineering](https://anthropic.com/engineering/effective-context-engineering-for-ai-agents) backs NOTES-style files and subagents that return short summaries. [Building effective agents](https://anthropic.com/engineering/building-effective-agents) says to "poka-yoke" tool interfaces, for example by requiring absolute paths. That applies to Culla's own IPC commands too.
- **CI.** Use `anthropics/claude-code-action@v1`, either for `@claude` mentions or to run the `code-review` plugin on PRs. Grant least-privilege `permissions:`, cap runs with `--max-turns`, and keep secrets in GitHub Secrets. Fork PRs don't receive secrets ([GitHub Actions doc](https://code.claude.com/docs/en/github-actions)).

**Draft CLAUDE.md outline (aim for under 60 lines):**
```
# Commands      pnpm i · pnpm check (biome+tsc) · pnpm test · pnpm test <file>
                cargo test -p culla-core · cargo clippy --all-targets -D warnings
                pnpm tauri dev · pnpm --filter web-demo dev
# Layout        crates/culla-core (pure, NO tauri dep) · apps/desktop/src-tauri (adapters+commands)
                packages/ui (shared React) · packages/backend (Backend interface + mock) · docs/adr
# Invariants    Nothing touches disk before Apply. No delete API outside apply::finalize_move.
                Every new command: core fn → port → command → capability → bindings → mock.
# Conventions   errors: thiserror in core, typed IpcError; regen bindings with `pnpm gen:bindings`
# Gotchas       bindings.ts is generated, don't edit · fixtures in testdata/ are real NEFs (LFS)
# Done =        tests + clippy + tsc green, ADR added if a decision changed
```

**Checklist**
- [ ] Commit `CLAUDE.md`, `.claude/settings.json` (hooks plus deny `Read(**/*.key)`, `Read(.env*)`, and `ask` for `git push *`), `.claude/agents/{security-reviewer,spec-reviewer}.md` (read-only), and `.claude/skills/{add-command,new-adr,release}/`.
- [ ] Gitignore `.claude/settings.local.json`, `CLAUDE.local.md` and worktree dirs.
- [ ] Hooks: PostToolUse runs `biome format` / `rustfmt` on the edited file. Stop runs a *fast* check (`tsc --noEmit` plus `cargo check -p culla-core`), with full tests left to CI. PreToolUse blocks edits to `src-tauri/capabilities/**` and `.github/workflows/**` unless a human asks.
- [ ] `docs/features.json` (a pass/fail list) plus `docs/PROGRESS.md`. Each agent task covers one feature and one PR, with a test written first (a failing test or fixture).
- [ ] Cut tasks along the hexagon: (a) a core function plus unit/property tests, (b) the adapter, (c) the command plus bindings, (d) the UI against the mock backend. Each slice can be verified on its own.

## 2. Security

- **Tauri.** Capabilities live in `src-tauri/capabilities/*.json` and are scoped per window label. By default *every registered command is callable from every window*. Restrict that with `AppManifest::commands` in `build.rs` ([capabilities](https://v2.tauri.app/security/capabilities/)). Set a strict CSP in `tauri.conf.json` (Tauri adds the nonces and hashes) and load nothing from a CDN ([CSP](https://v2.tauri.app/security/csp/)). Tauri "highly recommends" the isolation pattern, but on Windows the isolation iframe can't load ES modules ([isolation](https://v2.tauri.app/concept/inter-process-communication/isolation/)). Culla loads no remote content, so it adds little here. Treat it as optional and record the choice in an ADR. The fs plugin rejects `..` and its `deny` rules override `allow` ([fs plugin](https://v2.tauri.app/plugin/file-system/)). Even so, **don't give the webview any fs plugin permissions.** All file operations go through Culla's own commands, which take opaque photo IDs, not paths.
- **Previews.** The user picks the folder at runtime, so an asset-protocol scope ([config](https://v2.tauri.app/reference/config/)) would have to be widened dynamically to the user's drives. Use a custom URI scheme like `culla://preview/<id>/<size>` instead, served only for IDs from the current scan.
- **Updater.** Signing can't be turned off. Generate keys with `tauri signer generate`, put the private key in CI's `TAURI_SIGNING_PRIVATE_KEY`, put the inline pubkey in config, and serve updates over HTTPS ([updater](https://v2.tauri.app/plugin/updater/)).
- **Supply chain.** Run `cargo-deny` covering advisories, bans, licenses and sources ([checks](https://embarkstudios.github.io/cargo-deny/checks/index.html)). That makes a separate `cargo-audit` redundant. In pnpm, use `allowBuilds` as an allowlist, keep `strictDepBuilds` and `blockExoticSubdeps` on, set `minimumReleaseAge` (default 1 day, consider 7) and `trustPolicy: no-downgrade` ([pnpm](https://pnpm.io/supply-chain-security)). Dependabot now waits 3 days by default; enable it for cargo, npm and github-actions ([changelog](https://github.blog/changelog/2026-07-14-dependabot-version-updates-introduce-default-package-cooldown/)).
- **GitHub.** CodeQL scans Rust in default setup ([GA](https://github.blog/changelog/2025-10-14-codeql-scanning-rust-and-c-c-without-builds-is-now-generally-available)). Push protection is on by default for new public repos ([changelog](https://github.blog/changelog/2024-03-11-secret-scanning-and-push-protection-are-enabled-by-default-on-new-public-repositories/)). For actions, either pin SHAs (which can be enforced by policy, see [changelog](https://github.blog/changelog/2025-08-15-github-actions-policy-now-supports-blocking-and-sha-pinning-actions/)) or use the new `.github/workflows/actions.lock` via `gh actions-lock`, GA since 2026-10-28, which also pins transitive actions ([changelog](https://github.blog/changelog/2026-10-28-github-actions-workflow-lockfiles-are-generally-available)). Scorecard checks to target: Token-Permissions, Pinned-Dependencies, SAST, Fuzzing, Security-Policy, Signed-Releases, Branch-Protection ([scorecard](https://github.com/ossf/scorecard)).
- **Threat model.**
  - (1) *Path escape.* Every destination comes from `plan()`. Before running, check that it is under the chosen root (compare normalized absolute paths) and never inside the source tree, apart from `_rejected`, which the scanner must exclude.
  - (2) *Symlinks and junctions.* Walk with `follow_links(false)`, skip reparse points, and use `symlink_metadata`. Re-check right before each operation, because path checks race with filesystem changes. Rust's own `remove_dir_all` had exactly this TOCTOU symlink bug ([CVE-2022-21658](https://blog.rust-lang.org/2022/01/20/cve-2022-21658/)).
  - (3) *Overwrites.* `std::fs::rename` on Windows replaces an existing target. Make collisions a hard error, re-check them at execution time, and copy with `OpenOptions::create_new`.
  - (4) *Never deleting.* The spec's cross-volume move is copy, verify, then delete the source. That one unavoidable delete should live in a single audited function. Ban `std::fs::remove_*` everywhere else with clippy `disallowed-methods`.
  - (5) *Hostile EXIF/NEF.* Use `cargo-fuzz` targets (nightly, libFuzzer, `arbitrary` for structure-aware input) for the TIFF/IFD walker, embedded-JPEG extraction and EXIF parsing ([Rust Fuzz Book](https://rust-fuzz.github.io/book/print.html)). Also set decode size limits against decompression bombs, and keep `unsafe` out of the core (`#![forbid(unsafe_code)]`).

**Checklist**
- [ ] One capability for the `main` window, minimal `core:*` permissions, no fs/shell plugins, and `AppManifest::commands` in `build.rs`.
- [ ] Strict CSP. Custom `culla://` preview protocol with the asset protocol disabled. Isolation decision recorded in an ADR.
- [ ] `deny.toml`, pnpm hardening in `pnpm-workspace.yaml`, and `dependabot.yml` covering 3 ecosystems.
- [ ] Ruleset on `main` (PR required, status checks, no force-push). CodeQL for JS/TS, Rust and Actions. `permissions: {}` at the top of each workflow and per-job grants. `actions.lock`. Scorecard plus badge.
- [ ] `SECURITY.md` (private vulnerability reporting, supported versions, scope covering file operations and parsers) and `docs/threat-model.md`.
- [ ] `fuzz/` with 3 targets, a weekly scheduled run, and the crash corpus turned into regression tests.

## 3. Architecture

- **Hexagonal core.** `culla-core` has no Tauri dependency. It owns the domain (`Photo`, `Pair`, `Decision`, `Plan`, `Op`) and the ports (traits `FileSystem`, `PreviewSource`, `SessionStore`, `Clock`). `scan()`, `plan()` and the journal state machine are pure, or depend only on ports. The adapters (std fs, image decoding, JSON/SQLite store) and the thin `#[tauri::command]` wrappers live in `src-tauri`. That makes the core easy to test on Linux CI with an in-memory `FileSystem`.
- **TS `Backend` interface** in `packages/backend`, e.g. `scan(opts)`, `preview(id,size)→url`, `saveDecisions(log)`, `plan()`, `apply(onProgress)`. There are two implementations: `TauriBackend` (generated bindings) and `MockBackend` (bundled samples, simulated operations). The UI only sees the interface, injected through context, and the web demo is the same UI with the mock.
- **Swipe state as an event log.** Store an append-only log of `Decided{id,verdict,at}` and `Undone{id,at}` events, and derive state with a pure reducer `fold(log)`. Undo appends an event instead of removing one, so the history survives a resume, and Ctrl+Z walks back through the log. Persist the log as JSONL per folder. Define the event types in Rust and export them to TS. Share golden fixtures (`log.jsonl` → expected state) that both the Vitest and `cargo test` suites run, so the two reducers can't drift apart.
- **Errors.** Use `thiserror` enums in the core and keep `anyhow` out of library code. Commands return `Result<T, IpcError>`, and Tauri requires errors to implement `Serialize` ([calling Rust](https://v2.tauri.app/develop/calling-rust/)). Use `#[serde(tag="kind")]` so TS receives a discriminated union to `switch` on. Generate bindings with tauri-specta. **Caution:** v2 is still pre-release (`2.0.0-rc.25`, see [releases](https://github.com/specta-rs/tauri-specta/releases)), so pin the exact version and record the decision in an ADR, with `ts-rs` as the fallback.
- **ADRs** in `docs/adr/NNNN-title.md`, using Nygard's format: context, decision, status, consequences ([Fowler](https://www.martinfowler.com/bliki/ArchitectureDecisionRecord.html)).
- **Testing pyramid.**
  - Many fast tests: core unit tests, `proptest` on `plan()` (no duplicate destinations, every op under a root, no delete op except finalize-move), and `insta` snapshots of plans.
  - Middle layer: adapter integration tests on `tempfile` dirs on **windows-latest** (junctions, read-only files, collisions, a simulated crash mid-apply followed by resume). Vitest for the reducer and components against `MockBackend`.
  - Few end-to-end tests: Playwright against the web demo, which is cheap and runs in CI, plus a small `tauri-driver` smoke test on Windows.

**Checklist**
- [ ] `crates/culla-core` with `#![forbid(unsafe_code)]` and no `tauri` in its dependency tree (checked by `cargo deny` bans).
- [ ] `packages/backend` with the interface plus both implementations. Shared event-log fixtures.
- [ ] `IpcError` tagged enum, generated `bindings.ts`, and a CI check that bindings are up to date (`git diff --exit-code`).
- [ ] ADRs 0001–0008: record-ADRs, Tauri, hexagonal core, custom preview protocol, event-sourced decisions, never-delete semantics, tauri-specta RC, Biome.

## 4. Material for the blog enablement guide

- **"Guideline vs. guarantee":** a short CLAUDE.md next to the hooks, deny rules and the clippy `disallowed-methods` rule. Shows how "never delete" is enforced by the harness, the compiler and CI, not by prompting.
- **Agent-sized slices:** the `add-command` skill as one recipe (core → port → command → capability → binding → mock), with `features.json` and `PROGRESS.md` from a real session.
- **The hexagon diagram:** one core with two outer layers (Tauri and the mock web demo). The `Backend` interface snippet next to the Rust port trait.
- **Event-sourced undo:** the `fold()` reducer, plus the golden fixture shared by Rust and TS.
- **Safe apply:** `plan()` as a pure function, the property tests, and the write-ahead journal state machine with crash and resume.
- **Tauri hardening:** a capability file, CSP, and why a custom protocol beats a broad asset scope.
- **Fuzzing a NEF parser:** a `fuzz_target!` snippet and the first crash found.
- **Supply-chain baseline:** `deny.toml`, the pnpm settings, `actions.lock`, and the Scorecard badge before and after.
- **Honest caveats:** tauri-specta is still an RC, isolation doesn't fit well on Windows, and cross-volume move needs exactly one delete.
