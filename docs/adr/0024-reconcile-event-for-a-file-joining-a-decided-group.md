---
status: accepted
date: 2026-11-27
---

# A new file joining a decided group makes it undecided, through a reconcile event

Refines [ADR 0005](0005-decision-state-as-append-only-event-log.md) (adds a `cause` to events). The event types and the fold are unchanged for user events.

## Context and problem statement

When a folder with a stored session is reopened, a new file can join an existing group: NX Studio wrote a sidecar since the last session, or a JPEG now sits next to a lone NEF. The user decided on a different set of files. Should the decision still hold, and if not, how is the change recorded in the append-only log?

## Considered options

* Keep the decision, and move the new file with the group
* Make the group undecided with a normal `Cleared` event (undoable with Ctrl+Z)
* Make the group undecided with a `Cleared` event marked `cause = reconcile`, kept out of the undo and redo stacks

## Decision outcome

Chosen option: "A `Cleared` event with `cause = reconcile`", because the decision no longer covers the files on disk, and an undo should not bring back a decision the user never made on these files.

* Every event gets `cause: User | Reconcile`; only `Cleared` can have `cause = reconcile`.
* A reconcile `Cleared` makes the group undecided, removes the group's earlier events from both stacks and pushes nothing, so the earlier `Decided` cannot be undone or redone. The review says why the group is undecided again.
* It never touches a locked or applied group; a file that joins an applied group is listed as `joins_applied_group` and never touched.
* Decided by Margaux on 27 Nov 2026. Details: [design/file-engine.md](../design/file-engine.md) §5.1 and §6.2.

### Consequences

* Good, because no plan acts on a decision made before a file existed.
* Good, because the log stays append-only and state stays a pure fold (Rust and TS share fixtures for reconcile events too).
* Bad, because the user decides that photo again, and Ctrl+Z skips the cleared decision.

Source: [SPEC.md §3.3](../SPEC.md), [design/file-engine.md §5.1, §6.2](../design/file-engine.md).
