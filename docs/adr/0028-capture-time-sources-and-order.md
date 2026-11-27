---
status: accepted
date: 2026-11-27
decision-makers: Margaux Faurie (accepted 27 Nov 2026)
---

# Capture time: DateTimeOriginal, then CreateDate, then the file time; camera-local order with filename tie-breaks

Refines [ADR 0011](0011-own-ifd-walker-for-nef-previews-and-kamadak-exif.md) and story US-06. Proposed in the M1 EXIF slice (`culla_core::exif`, `culla_adapters::metadata::exif`) and accepted by Margaux on 27 Nov 2026, with one change: the file-time fallback is read in the PC's local time zone, not UTC, so it sorts with camera-local EXIF times. Until the time-zone lookup lands (with the drive-identity slice, which brings the Windows API), the code still uses UTC.

## Context and problem statement

SPEC §5.1 sorts the list by capture time once the EXIF pass ends, and US-06 leaves open the rule for a photo without `DateTimeOriginal`. The pass also needs a rule for which file of a group gives its time, for sub-seconds and UTC offsets, for equal times, and for how much of each file it reads. Which time does each group sort by, and in what order?

## Considered options

* Sort by `DateTimeOriginal` only; photos without it go last
* Fallback chain `DateTimeOriginal`, `CreateDate`, file time; order by camera-local time; ties by path
* As above, but order by UTC instant when the camera wrote an offset

## Decision outcome

Chosen option: "fallback chain, camera-local order, ties by path", because it gives every photo a place, is total and deterministic, and fits the `photo_group.capture_time` column (ms, camera-local).

* Sources, first valid one wins: `DateTimeOriginal` + `SubSecTimeOriginal` + `OffsetTimeOriginal`; then `DateTimeDigitized` (`CreateDate`) with its own sub-second and offset tags; then the file's modification time from the listing, read in the PC's local time zone (interim: UTC until the time-zone lookup lands with the drive-identity slice). A blank or all-zero value counts as absent; an unreadable one is recorded as an issue and skipped.
* A group sorts by its primary's time: the JPEG of a pair (its NEF is not read in the pass), the NEF of a lone NEF.
* Order by camera-local milliseconds. Equal times by the primary's relative path, folded component by component, then as written. A group with no time at all (no EXIF time and no file time) goes right after the dated group that precedes it in filename order, even when other groups share that group's time, and before them in filename order when nothing dated precedes it; this is US-06's "keeps its filename position".
* The pass reads the first 128 KiB of each file (`EXIF_HEAD_BYTES`, SPEC's ~128 KB) and parses with `continue_on_error`: on the Z6III the tags end before 36.2 KB, while the MakerNote (from ~36 KB to 225–266 KB) and the GPS IFD after it run past the cap and are skipped.
* Implementation detail added in PR #23, not part of the author's decision: only TIFF data reaches `kamadak-exif` (`read_raw`): the adapter recognises TIFF and JPEG by their magic bytes and extracts the JPEG's APP1 Exif payload itself; any other container has no EXIF for Culla.
* Orientation outside 1..=8 is treated as 1. The lens is `LensModel`, else a name built from `LensSpecification` (`24-200mm f/4-6.3`), else none.

### Consequences

* Good, because a burst stays in shutter order (sub-seconds) and a counter rollover across `100NIKON`/`101NIKON` sorts by time.
* Good, because the order never depends on scan order or case.
* Bad, until the time-zone lookup lands: the file time is still read as UTC, so such a photo can be off by the PC's offset.
* Bad, because a trip across time zones sorts by wall clock, not by instant, when the camera clock was changed.

Source: [SPEC.md §5.1](../SPEC.md), [stories.md US-06](../stories.md), [z6iii-nef-layout.md](../research/z6iii-nef-layout.md), [design/file-engine.md §2.3](../design/file-engine.md).
