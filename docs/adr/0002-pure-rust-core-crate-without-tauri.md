---
status: accepted
date: 2026-11-24
---

# Keep the domain in a pure Rust core crate with no Tauri dependency

## Context and problem statement

The file engine is the riskiest part of Culla and needs the most tests, including interrupted-apply scenarios. Logic mixed into Tauri commands is hard to test without a running app. Where does the domain live?

## Considered options

* All logic in `src-tauri` commands
* A `crates/core` library with no Tauri dependency, hexagonal ports and adapters
* A core crate that also contains the real adapters (fs, SQLite, codecs)

## Decision outcome

Chosen option: "`crates/core` with no Tauri dependency, hexagonal ports and adapters", because the domain (groups, decisions, plan, journal state machine) becomes pure or port-only and runs under `cargo test` on Linux CI with in-memory fakes.

* Ports are traits such as `FileSystem`, `VolumeIdentity`, `SessionStore`, `ImageCodec`, `PreviewCache` and `Clock` (starting set, see [architecture.md §5.2](../architecture.md#52-core-components-c4-level-3-hexagonal)).
* Real adapters and thin `#[tauri::command]` wrappers live in `src-tauri`.
* The core has `#![forbid(unsafe_code)]`. `cargo deny` bans `tauri` from its dependency tree.
* Prior art: [mountain-loop/yaak](https://github.com/mountain-loop/yaak) keeps `crates/` apart from `crates-tauri/`.

### Consequences

* Good, because `plan()` and the journal can be property-tested and snapshot-tested quickly.
* Good, because work for agents slices cleanly: core function, adapter, command, UI.
* Bad, because Windows-specific behaviour (junctions, locks, read-only media) is only covered by adapter tests, which must run on `windows-latest`.
* Bad, because ports add a layer of indirection for a small app.

Source: [practices.md §3](../research/practices.md), [research-stack-repo.md §5](../research/research-stack-repo.md).
