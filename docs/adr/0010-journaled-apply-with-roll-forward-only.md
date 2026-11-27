---
status: accepted
date: 2026-11-24
---

# Journaled apply: copy, hash, fsync, verify, rename; roll forward only

Refined by: [ADR 0022](0022-rename-through-the-checked-handle.md) (accepted 27 Nov 2026), for how renames are issued.

Note: the operation states listed below are superseded by [design/file-engine.md](../design/file-engine.md) §9.2.

Refined by: [ADR 0021](0021-unbuffered-source-re-read-for-hash-3.md) (accepted 27 Nov 2026), for the source re-read.

## Context and problem statement

An apply can move 2,000 pairs (about 140 GB) across volumes. The app can crash, the laptop can sleep, the card can be pulled. Flaky readers can corrupt data without changing the size. How does the engine execute a plan so that no file is lost and an interrupted apply can finish?

## Considered options

* Plain `std::fs::copy` / `rename`, no journal
* Journal with rollback of the whole apply on failure
* Write-ahead journal per file, roll forward only
* Verification by size, by mtime, or by hash

## Decision outcome

Chosen option: "Write-ahead journal per file, roll forward only, hash verification", because rollback after a cross-volume source removal would mean copying everything back, and a per-file journal is enough to finish safely.

* One journal row per file, written before each step. States in SPEC: `pending → copying → verified → done | failed`. The exact state machine is owned by [design/file-engine.md](../design/file-engine.md).
* Same-volume move: one `MoveFileExW` rename (atomic, also on exFAT).
* Cross-volume move or copy: copy to `*.culla-partial` with `create_new` while hashing the source (xxh3 or blake3, to decide), fsync, read the destination back unbuffered and hash it, hash the source again, require all three hashes to match, rename into place, preserve mtime. For a move, `remove_verified` then removes the source ([ADR 0004](0004-never-delete-or-overwrite.md)).
* Sharing violations (Explorer, Defender, Lightroom) are retried with backoff, then the file is marked `failed` and the apply continues.
* Resume: on the next start, check that the volumes are present and their serials match, then roll each unfinished row forward. A stale partial file is removed by `remove_verified`.
* Size alone catches truncation only. FAT mtimes are ±2 s.

### Consequences

* Good, because any interruption leaves every photo in at least one complete, verified place.
* Good, because the engine is a state machine that can be tested with injected faults at each step.
* Bad, because there is no undo of an apply in v1 (the reverse plan is a v2 candidate; the log file is in v1).
* Bad, because verification re-reads data, which lengthens cross-volume applies.

Source: [SPEC.md §5.2](../SPEC.md), [research-raw-perf.md §5](../research/research-raw-perf.md), [research-stack-repo.md, spec issue 2](../research/research-stack-repo.md), [critique.md §2](../research/critique.md).
