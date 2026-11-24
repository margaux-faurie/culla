---
status: accepted
date: 2026-11-24
---

# Unbuffered source re-read for hash 3

Partly supersedes [ADR 0004](0004-never-delete-or-overwrite.md) (its definition of "verified").

## Context and problem statement

ADR 0004 and SPEC §5.2 verify a cross-volume move or copy with three hashes: the source while copying (hash 1), the destination read back unbuffered (hash 2) and the source re-read (hash 3). Hash 3 exists to catch a flaky card or reader that returned wrong bytes during the copy. A file that was just copied is almost certainly still in the Windows standby cache, so a normal re-read is answered from memory and hash 3 only repeats hash 1.

## Considered options

- Re-read the source with `FILE_FLAG_NO_BUFFERING`
- Re-read the source normally (accept that the cache may answer)
- Drop hash 3

## Decision outcome

Chosen option: the source re-read for hash 3 also opens the file with `FILE_FLAG_NO_BUFFERING`, as the destination read-back does. Reads use sector-aligned offsets, lengths and buffers, with the sector size queried per volume (`FileStorageInfo`), and handle a final partial sector.

Decided by Claude during review on 24 Nov 2026, as the way to make the author's three-hash rule do what it is for. Margaux to confirm.

### Consequences

- Good: hash 3 reads from the device, not from the OS cache.
- Bad: the card is read twice in full, which roughly doubles the read time of a cross-volume apply (SPEC §9). The reader's own hardware cache may still answer; this is a known limit.
- Bad: aligned unbuffered I/O is more code and needs tests with odd file sizes.
