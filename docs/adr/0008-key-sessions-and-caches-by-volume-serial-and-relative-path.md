---
status: accepted
date: 2026-11-24
---

# Key sessions and caches by volume serial + relative path

## Context and problem statement

Sessions (decisions, journal) and the preview cache must be found again when the user reopens a folder. SD cards and external drives often come back under a different drive letter (E: one day, F: the next). What identifies a source and its files?

## Considered options

* Absolute path (with the drive letter)
* Volume GUID path + relative path
* Volume serial number + relative path
* Content identity (EXIF date + shutter count, or a hash)

## Decision outcome

Chosen option: "Volume serial number + relative path", because it survives a drive-letter change and is stored on the media itself.

* A source is identified by (volume serial, path relative to the volume root). Files inside it by their relative path. Cache entries add size and mtime: (volume serial, path, size, mtime).
* The volume GUID was rejected because Windows assigns it per machine in the mount manager, so the same card can get a different GUID on another PC.
* On open, the app reconciles the folder with the stored session: missing files are flagged, new files are added as undecided.
* Renamed files are not detected in v1 (one missing plus one new). Content identity is a later option.

### Consequences

* Good, because "card back as F:" resumes the same session and reuses the cache.
* Good, because the same key serves sessions, journal and cache.
* Bad, because the serial is 32-bit and changes when a card is reformatted; a reformatted card starts a new session.
* Bad, because two cards with the same serial (cloned images) and the same folder path would collide; reconciliation must flag a mismatch, and the threat model reviews it.
* Bad, because resolving the serial needs a Win32 call (`GetVolumeInformationW`) in the adapters crate.

Source: [SPEC.md §3.3](../SPEC.md), [research-raw-perf.md, "wrong or naive" §1](../research/research-raw-perf.md), [critique.md §2](../research/critique.md).
