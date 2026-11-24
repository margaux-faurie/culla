---
status: accepted
date: 2026-11-24
---

# Never delete or overwrite: rejects go to `_rejected/`, collisions get a suffix per group

## Context and problem statement

A file tool for photos must not lose a photo. Nikon counters roll over, so a destination often already holds a `DSC_0001`. On Windows `std::fs::rename` silently replaces an existing target. How does Culla handle rejects and name collisions?

## Considered options

* Reject: permanent delete
* Reject: Recycle Bin
* Reject: move to `<source>/_rejected/`, mirroring subfolders
* Collision: overwrite, skip, or ask per file
* Collision: suffix the whole group (`_1`, `_2`…) and show it in the review

## Decision outcome

Chosen option: "Move rejects to `<source>/_rejected/` and suffix the whole group on collision", because the app then never deletes a photo and never overwrites one.

* The only removal is the source of a verified cross-volume move. It lives in one audited function (`finalize_move`). `std::fs::remove_*` is banned everywhere else with clippy `disallowed-methods`.
* Every member of a group (JPEG, NEF, sidecars) gets the same target name, so a group stays a group.
* Collisions are re-checked right before each operation. Copies open targets with `create_new`. Renames must not replace the target.
* `_rejected/` and an in-source destination are excluded from scans.

### Consequences

* Good, because the worst outcome of a bug is a file in the wrong folder, not a lost file.
* Good, because the rule can be enforced by lints and property tests, not only by review.
* Bad, because rejects still take space on the card until the user empties `_rejected/` themselves.
* Bad, because a re-check before a plain `rename` still leaves a race window; a no-replace rename is needed in the adapter (see [architecture.md §11](../architecture.md#11-risks-and-technical-debt)).

Source: [SPEC.md §2, §5.2](../SPEC.md), [practices.md §2 threat model](../research/practices.md), [critique.md §2](../research/critique.md). Details: [design/file-engine.md](../design/file-engine.md).
