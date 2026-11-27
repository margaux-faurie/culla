---
status: proposed
date: 2026-11-27
---

# Capture time: DateTimeOriginal, then CreateDate, then the file time; camera-local order with filename tie-breaks

Refines [ADR 0011](0011-own-ifd-walker-for-nef-previews-and-kamadak-exif.md) and story US-06. Proposed in the M1 EXIF slice (`culla_core::exif`, `culla_adapters::metadata::exif`); waits for Margaux.

## Context and problem statement

SPEC §5.1 sorts the list by capture time once the EXIF pass ends, and US-06 leaves open the rule for a photo without `DateTimeOriginal`. The pass also needs a rule for which file of a group gives its time, for sub-seconds and UTC offsets, for equal times, and for how much of each file it reads. Which time does each group sort by, and in what order?

## Considered options

* Sort by `DateTimeOriginal` only; photos without it go last
* Fallback chain `DateTimeOriginal`, `CreateDate`, file time; order by camera-local time; ties by path
* As above, but order by UTC instant when the camera wrote an offset

## Decision outcome

Chosen option: "fallback chain, camera-local order, ties by path", because it gives every photo a place, is total and deterministic, and matches the `photo_group.capture_time` column (camera-local, ms).

* Sources, first valid one wins: `DateTimeOriginal` + `SubSecTimeOriginal` + `OffsetTimeOriginal`; then `DateTimeDigitized` (`CreateDate`) with its own sub-second and offset tags; then the file's modification time from the listing, in UTC. A blank or all-zero value counts as absent; an unreadable one is recorded as an issue and skipped.
* A group sorts by its primary's time: the JPEG of a pair (its NEF is not read in the pass), the NEF of a lone NEF.
* Order by camera-local milliseconds. Equal times by the primary's relative path, folded component by component, then as written. A group with no time at all (no EXIF time and no file time) goes right after the group that precedes it in filename order, which is US-06's "keeps its filename position".
* The pass reads the first 128 KiB of each file (`EXIF_HEAD_BYTES`, SPEC's ~128 KB) and parses with `continue_on_error`: on the Z6III the tags end before 36.2 KB, while the MakerNote (225–266 KB) and GPS IFD run past the cap and are skipped.
* Orientation outside 1..=8 is treated as 1. The lens is `LensModel`, else a name built from `LensSpecification` (`24-200mm f/4-6.3`), else none.

### Consequences

* Good, because a burst stays in shutter order (sub-seconds) and a counter rollover across `100NIKON`/`101NIKON` sorts by time.
* Good, because the order never depends on scan order or case.
* Bad, because the file-time fallback is UTC while EXIF times are camera-local: such a photo can land off by the camera's offset (11 hours on the author's samples). Reading the PC's time zone would fix most of it; to decide.
* Bad, because a trip across time zones sorts by wall clock, not by instant, when the camera clock was changed.

Source: [SPEC.md §5.1](../SPEC.md), [stories.md US-06](../stories.md), [z6iii-nef-layout.md](../research/z6iii-nef-layout.md), [design/file-engine.md §2.3](../design/file-engine.md).
