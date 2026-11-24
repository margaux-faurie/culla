---
status: proposed
date: 2026-11-24
---

# Rename through the checked handle, with no-replace semantics

Refines [ADR 0004](0004-never-delete-or-overwrite.md) and [ADR 0010](0010-journaled-apply-with-roll-forward-only.md) (how a rename is issued). The rule they state, never overwrite and never copy-then-delete silently, is unchanged.

## Context and problem statement

ADR 0004 renames with `MoveFileExW` and only `MOVEFILE_WRITE_THROUGH`. The adversarial review of the file engine found two problems. `MoveFileExW` works on paths, so the file renamed is not necessarily the file that was just verified. And `MOVEFILE_WRITE_THROUGH` does not make a same-volume rename durable, while `FlushFileBuffers` needs a handle opened for writing.

## Considered options

- `MoveFileExW` by path, as in ADR 0004
- `SetFileInformationByHandle` with `FileRenameInfoEx` on the handle that was checked, without `FILE_RENAME_FLAG_REPLACE_IF_EXISTS`; fallback `FileRenameInfo` with `ReplaceIfExists = FALSE` where Ex is unsupported

## Decision outcome

Chosen option: rename through the handle that was checked. A partial is held with `GENERIC_READ | GENERIC_WRITE | DELETE`, renamed through that handle and flushed through it. A user file (same-volume keep or reject) is renamed through a `DELETE | GENERIC_READ` handle; Culla never opens a user file for writing, so that rename may be unflushed, and recovery detects a reverted rename. Details: [design/file-engine.md](../design/file-engine.md).

A technical refinement made during review on 24 Nov 2026. It changes how the rule is enforced, not what the user sees.

### Consequences

- Good: no path race between check and rename; the verified file is the renamed file.
- Good: partials are durable before `committed` is journaled.
- Bad: more Win32 code in `culla-adapters`; `FileRenameInfoEx` support on FAT32/exFAT must be proven in adapter tests (fallback above).
