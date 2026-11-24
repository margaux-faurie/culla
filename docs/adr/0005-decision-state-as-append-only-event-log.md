---
status: accepted
date: 2026-11-24
---

# Store decision state as an append-only event log

## Context and problem statement

Culla needs global undo and redo that jump back to the photo, a per-photo undo (U), autosave on every decision, and resume after a restart. How is decision state stored?

## Considered options

* A mutable map `groupId → state`, with a separate undo stack in memory
* An append-only log of events, with state derived by a pure fold

## Decision outcome

Chosen option: "Append-only event log", because undo, redo, autosave and resume all fall out of one structure.

* Each change appends `{seq, groupId, from, to, ts}`. The current state is `fold(log)`.
* Ctrl+Z and Ctrl+Shift+Z walk the log and append the reversing event. U appends an event that makes the current photo undecided. History is never rewritten, so it survives a resume.
* Event types are defined in Rust and exported to TS. Golden fixtures (`log → expected state`) run in both `cargo test` and Vitest so the two reducers cannot drift.
* The UI commits to its reducer first, then mirrors the event to the backend.

### Consequences

* Good, because autosave is one small insert per decision.
* Good, because resume replays the log and gives the same state and history.
* Bad, because there are two reducers (Rust and TS) to keep in step, guarded only by shared fixtures.
* Bad, because the log grows with every keystroke; fine at thousands of events, may need compaction later.

Source: [SPEC.md §3.2](../SPEC.md), [practices.md §3](../research/practices.md). The event shape in SPEC replaces the `Decided` / `Undone` shape proposed in practices.md.
