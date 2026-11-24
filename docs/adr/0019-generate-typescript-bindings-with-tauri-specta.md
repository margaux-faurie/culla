---
status: accepted
date: 2026-11-24
---

# Generate TypeScript bindings with tauri-specta, pinned RC, ts-rs as fallback

## Context and problem statement

The UI calls Rust commands and receives Rust types: groups, decision events, plans, progress and `IpcError`. Hand-written TS types drift from Rust. The decision events must have one shape in both reducers ([ADR 0005](0005-decision-state-as-append-only-event-log.md)). How are the TS types and command wrappers kept in step?

## Considered options

* Hand-written TS types and `invoke` calls
* `ts-rs` for types, hand-written `invoke` wrappers
* `tauri-specta` for types and typed command wrappers

## Decision outcome

Chosen option: "`tauri-specta`, pinned to an exact RC, with `ts-rs` as fallback", because it generates both the types and the command functions, including the tagged `IpcError` union.

* tauri-specta v2 is still pre-release (`2.0.0-rc.25` at the time of research, see [releases](https://github.com/specta-rs/tauri-specta/releases)), so the version is pinned exactly (`=`) and upgraded on purpose.
* `IpcError` uses `#[serde(tag = "kind")]`, so TS gets a discriminated union to `switch` on. Tauri requires command errors to be `Serialize` ([calling Rust](https://v2.tauri.app/develop/calling-rust/)).
* `pnpm gen:bindings` writes `src/bindings.ts`. It is never edited by hand. CI regenerates and fails on `git diff --exit-code`.
* Fallback: if tauri-specta breaks or stalls, switch to `ts-rs` for types plus thin hand-written wrappers. The `Backend` interface ([ADR 0014](0014-backend-interface-with-build-time-implementations.md)) keeps that change inside `TauriBackend`.

### Consequences

* Good, because a Rust type change that breaks the UI fails `tsc`.
* Good, because errors are typed end to end.
* Bad, because the project depends on an RC; upgrades may break the generated code.
* Bad, because the fallback means more hand-written code if it is ever needed.

Source: [practices.md §3](../research/practices.md), [SPEC.md §7](../SPEC.md).
