---
status: accepted
date: 2026-11-24
---

# Pair files by case-insensitive basename within the same folder only

## Context and problem statement

A Nikon shot can produce a JPEG, a NEF and several sidecars. The user decides once per shot, so these files must be grouped. Nikon file counters roll over (DSC_9999 → DSC_0001), and second bodies use the same names, so the same basename can appear in several DCIM subfolders. How are files grouped, and which sidecars go where?

## Considered options

* Pair by basename across the whole source tree
* Pair by basename within the same folder only
* Pair by basename + `DateTimeOriginal` + `SubSecTime` (would also cover dual-slot setups across cards)

## Decision outcome

Chosen option: "Case-insensitive basename within the same folder only", because it is correct for single-card shoots, cheap (no EXIF needed before the first photo appears) and never merges two different shots.

* A group has a `primary` (the JPEG if any, otherwise the NEF), an optional `raw` (the NEF) and its `sidecars`.
* A sidecar follows the file it describes:
  * `X.xmp` and `X.pp3` follow the group.
  * `X.JPG.xmp` and `NKSC_PARAM/X.JPG.nksc` follow the JPEG.
  * `X.NEF.xmp` and NX Studio's `NKSC_PARAM/X.NEF.nksc` follow the NEF, so they stay put when "RAW follows JPEG" is off.
* A collision suffix is applied inside sidecar names too (`X_1.xmp`, `NKSC_PARAM/X_1.NEF.nksc`).
* Unknown extensions are ignored and listed in the review screen.

### Consequences

* Good, because counter rollover and second bodies cannot produce false pairs.
* Good, because grouping is a pure function of a directory listing, easy to property-test.
* Bad, because Nikon dual-slot setups (NEF on one card, JPEG on the other) are not paired; "RAW follows JPEG" then does nothing. Detection and multiple sources are v2 candidates.
* Bad, because the `NKSC_PARAM` subfolder must be handled as part of its parent folder, not as a folder of its own.

Source: [SPEC.md §3.1, §2](../SPEC.md), [critique.md §2](../research/critique.md), [research-raw-perf.md, "wrong or naive" §4](../research/research-raw-perf.md) (NX Studio sidecars, [FastRawViewer](https://www.fastrawviewer.com/node/1005)).
