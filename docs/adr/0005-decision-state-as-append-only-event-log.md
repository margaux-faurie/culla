---
status: accepted
date: 2026-11-24
---

# Store decision state as an append-only event log

## Context and problem statement

Culla needs global undo and redo that jump back to the photo, a per-photo undo (U), autosave on every decision, and resume after a restart. How is decision state stored?

## Considered options

* A mutable map `groupId → state`, with an undo stack kept in memory
* An append-only log with `{from, to}` transitions, undo as a reversing transition
* An append-only log of intent events (`Decided`, `Cleared`, `Undo`, `Redo`), state derived by a pure fold

## Decision outcome

Chosen option: "Append-only log of intent events", because undo, redo, autosave and resume all fall out of one structure, and the redo rules are explicit.

* Every event has a `seq` and a timestamp. Event types:
  * `Decided {groupId, verdict}`: keep or reject (←/→, drag, Keep and Reject buttons).
  * `Cleared {groupId}`: U, the current photo becomes undecided.
  * `Undo {targetSeq}`: Ctrl+Z undoes the most recent `Decided` or `Cleared` that is not yet undone, and the UI jumps to that photo.
  * `Redo {targetSeq}`: Ctrl+Shift+Z redoes the most recently undone event.
* Any new `Decided` or `Cleared` empties the redo stack.
* The log only grows. The current state, the undo stack and the redo stack are all `fold(log)`.
* Event types are defined in Rust and exported to TS. Golden fixtures (`log → expected state`) run in `cargo test` and Vitest, so the two reducers cannot drift.
* The UI commits to its reducer first, then mirrors the event to the backend (autosave).

### Consequences

* Good, because autosave is one small insert per event, and resume replays the log to the same state and history.
* Good, because undo and redo survive a restart.
* Bad, because there are two reducers (Rust and TS) to keep in step, guarded by shared fixtures.
* Bad, because the log grows with every keystroke; fine at thousands of events, may need compaction later.

Source: [SPEC.md §3.2](../SPEC.md), [practices.md §3](../research/practices.md). State machine details: [design/file-engine.md](../design/file-engine.md).
