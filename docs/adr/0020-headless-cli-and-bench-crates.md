---
status: accepted
date: 2026-11-24
---

# Headless CLI and benchmark harness as separate crates

## Context and problem statement

The roadmap proves performance (M1) and data safety (M2) before any UI exists ([roadmap.md](../roadmap.md)). That needs a way to run scan, preview, plan, apply and resume without Tauri. The crash tests have to kill and restart a real process, and the benchmarks have to run end to end on real folders on the G14. Where does this code live, and how does it relate to the removal and rename rules ([ADR 0004](0004-never-delete-or-overwrite.md))?

## Considered options

* Subcommands of the Tauri binary (`culla.exe --headless …`)
* One `xtask` crate holding the CLI, the bench harness and the dev tasks
* Two crates: `crates/cli` for the user-facing headless commands, `crates/bench` for the benchmark harness

## Decision outcome

Chosen option: "two crates", because the CLI runs the real engine against real user folders and must follow every invariant, while the bench harness creates and cleans up scratch data and must not.

* `crates/cli`, package `culla-cli`, binary `culla`, with subcommands `scan`, `preview`, `plan`, `apply` and `resume`. It depends on `culla-core` and `culla-adapters` only, never on Tauri. Like `src-tauri`, it is wiring: it makes no file operation of its own and reaches the disk only through `culla-adapters`, so removal still goes only through `remove_verified` and nothing is written before `apply`.
* `crates/bench`, package `culla-bench`, binary `culla-bench` ([benchmarks.md](../benchmarks.md)). It depends on `culla-core` and `culla-adapters`.
* The clippy `disallowed-methods` rules are enforced on `culla-core`, `culla-adapters`, `culla-cli` and `src-tauri` only. Test code, `xtask` and `culla-bench` are outside them. They create and remove only their own scratch data inside temporary directories (`tempfile::TempDir`), never in a user folder. `culla-bench apply` copies its dataset into such a directory first.
* Neither binary is shipped in the installer in v1.

### Consequences

* Good, because crash tests (`culla apply` killed, then `culla resume`) exercise the same code path as the app, in a real process.
* Good, because the Tauri shell stays wiring only, and the CLI proves the core and adapters are usable without it.
* Good, because the bench can clean up after itself without weakening the rules on product code.
* Bad, because two more crates to build in CI.
* Bad, because the lint scope is now a list of crates; a new product crate must be added to it by hand.

Source: [testing.md](../testing.md), [benchmarks.md](../benchmarks.md), [SPEC.md §5.2, §7](../SPEC.md).
