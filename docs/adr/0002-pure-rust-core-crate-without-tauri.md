---
status: accepted
date: 2026-11-24
---

# Keep the domain in a pure Rust core crate, with adapters in a separate crate and no Tauri in either

## Context and problem statement

The file engine is the riskiest part of Culla and needs the most tests, including interrupted-apply scenarios. Logic mixed into Tauri commands is hard to test without a running app, and logic mixed with Windows I/O cannot run on fast Linux CI. How is the Rust code split?

## Considered options

* All logic in `src-tauri` commands
* `crates/core` with domain and ports; real adapters in `src-tauri`
* `crates/core` that also contains the real adapters (fs, SQLite, codecs)
* Three crates: `crates/core` (pure), `crates/adapters` (real I/O, no Tauri), `src-tauri` (wiring only)

## Decision outcome

Chosen option: "Three crates", because the domain stays pure and fast to test, the Windows adapters can be tested on their own without Tauri, and the Tauri layer stays thin.

* `crates/core` (`culla-core`): domain types, port traits, pure logic (grouping, decision log and fold, `plan()`, journal state machine) and services that drive the ports. No I/O, no Tauri, `#![forbid(unsafe_code)]`. Tested with in-memory fakes on any OS.
* `crates/adapters` (`culla-adapters`): Windows file operations, EXIF and NEF parsing, turbojpeg decode, the preview pipeline, the SQLite store. No Tauri. Integration-tested on `windows-latest` with `cargo test -p culla-adapters`. Win32 `unsafe` is confined here.
* `src-tauri`: wiring only. Commands, the `culla://` scheme, capabilities, composition root.
* `cargo deny` bans `tauri` from the dependency trees of both library crates.
* Prior art: [mountain-loop/yaak](https://github.com/mountain-loop/yaak) keeps `crates/` apart from `crates-tauri/`.

### Consequences

* Good, because `plan()` and the journal can be property-tested and snapshot-tested in seconds.
* Good, because work for agents slices cleanly: core function, adapter, command, UI.
* Good, because the adapters' Windows behaviour (junctions, locks, read-only media, no-replace rename) is tested without starting the app.
* Bad, because there are three crates and a layer of port traits for a small app.
* Bad, because adapter tests only mean something on Windows runners, which are slower in CI.

Source: [practices.md §3](../research/practices.md), [research-stack-repo.md §5](../research/research-stack-repo.md). Layout: [architecture.md §5.2](../architecture.md#52-core-components-c4-level-3-hexagonal).
