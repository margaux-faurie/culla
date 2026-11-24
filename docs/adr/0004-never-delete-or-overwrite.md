---
status: accepted (definition of "verified" refined by 0021)
date: 2026-11-24
---

# Never delete or overwrite: rejects go to `_rejected/`, collisions get a suffix per group

## Context and problem statement

A file tool for photos must not lose a photo. Nikon counters roll over, so a destination often already holds a `DSC_0001`. On Windows `std::fs::rename` silently replaces an existing target. How does Culla handle rejects, name collisions and the one removal a cross-volume move needs?

## Considered options

* Reject: permanent delete
* Reject: Recycle Bin
* Reject: move to `<source>/_rejected/`, mirroring subfolders
* Collision: overwrite, skip, or ask per file
* Collision: suffix the whole group (`_1`, `_2`…) and show it in the review
* No-overwrite: check that the target is free, then `std::fs::rename`
* No-overwrite: let the OS refuse (`MoveFileExW` without replace, `create_new` for copies)

## Decision outcome

Chosen option: "Rejects to `_rejected/`, group suffix on collision, and let the OS refuse any overwrite", because the app then never deletes a photo and never overwrites one, without a check-then-act race.

* Renames use `MoveFileExW` with only `MOVEFILE_WRITE_THROUGH`: no `MOVEFILE_REPLACE_EXISTING`, so an existing target is a hard error; no `MOVEFILE_COPY_ALLOWED`, so a cross-volume rename fails with `ERROR_NOT_SAME_DEVICE` instead of copying silently. Copies open targets with `create_new`.
* The single audited removal function is `remove_verified` in `culla-adapters`. It removes only (a) the source of a cross-volume move once it is verified and (b) Culla's own `*.culla-partial` files. `std::fs::remove_*` is banned outside `remove_verified`; `std::fs::rename` is banned everywhere (the adapters use the no-replace `MoveFileExW`). Both through clippy `disallowed-methods`.
* Verified means: the destination is fsynced, read back unbuffered (`FILE_FLAG_NO_BUFFERING`) and hashed, the source is re-read unbuffered and hashed a second time ([ADR 0021](0021-unbuffered-source-re-read-for-hash-3.md)), and all three hashes (copy-time source, destination read-back, source re-read) match.
* Every member of a group gets the same suffix, including inside sidecar names (`X_1.xmp`, `X_1.NEF.xmp`).
* `_rejected/` and an in-source destination are excluded from scans.

### Consequences

* Good, because the worst outcome of a bug is a file in the wrong folder, not a lost or replaced file.
* Good, because the rule is enforced by the OS call, lints and tests, not only by review.
* Bad, because rejects still take space on the card until the user empties `_rejected/` themselves.
* Bad, because verification reads every cross-volume file twice more, which lengthens long applies; the review estimate must include it.
* Bad, because `MoveFileExW` needs a Win32 call (`unsafe`) in the adapters crate.

Source: [SPEC.md §2, §5.2](../SPEC.md), [practices.md §2 threat model](../research/practices.md), [research-raw-perf.md §5](../research/research-raw-perf.md), [critique.md §2](../research/critique.md). Details: [design/file-engine.md](../design/file-engine.md).
