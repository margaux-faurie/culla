---
status: accepted
date: 2026-11-24
---

# Stage decisions in a queue; nothing touches the disk until an explicit apply

## Context and problem statement

A culling session makes thousands of fast decisions. Acting on each one immediately (move on swipe) makes mistakes costly and turns every keystroke into a file operation on a slow, removable card. When do decisions reach the disk?

## Considered options

* Apply each decision immediately
* Queue all decisions, then review screen, explicit confirm, apply
* Queue, but apply on a single shortcut (for example Ctrl+Enter)

## Decision outcome

Chosen option: "Queue, review screen, explicit confirm, apply", because the review step is what makes the engine safe and is the product's main difference from other cullers.

* The review screen shows counts, kept and rejected grids, the full operation plan, preflight results, and estimated size and time.
* The confirm dialog states the counts. The critique showed that a two-keystroke apply could move thousands of files.
* Undecided photos are never touched.

### Consequences

* Good, because mistakes cost nothing until apply, and the plan can be checked as a whole (collisions, free space, locks).
* Good, because swiping does no file I/O apart from reads and the autosave.
* Bad, because the user waits for one long apply at the end (2,000 cross-volume pairs is about 140 GB and 25+ minutes), so the review must show an estimate.
* Bad, because decisions must persist safely between sessions ([ADR 0005](0005-decision-state-as-append-only-event-log.md), [ADR 0009](0009-sqlite-per-source-for-sessions-and-journal.md)).

Source: [SPEC.md §2, §4.2](../SPEC.md), [critique.md §1](../research/critique.md).
