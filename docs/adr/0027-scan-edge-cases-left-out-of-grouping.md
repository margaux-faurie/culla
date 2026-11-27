---
status: proposed
date: 2026-11-27
---

# Scan edge cases the spec does not cover: case-colliding folders, NKSC_PARAM contents, partial listings, a link as the source

Refines [ADR 0007](0007-pair-files-by-basename-within-one-folder.md) and [design/file-engine.md](../design/file-engine.md) §3.1–§3.3. Proposed in the M1 scan and grouping slice (`culla_core::grouping`, `culla_adapters::scan`); waits for Margaux.

## Context and problem statement

The first scan and grouping code met cases that SPEC §3 and file-engine §3 leave open. Each needs a rule, and guessing about a user's files is not allowed (file-engine §3.3: "ambiguity is never resolved by guessing"). Which rules does the scan follow?

1. Two folders whose names fold to the same value (`100NIKON` and `100nikon`). This can only happen in a case-sensitive folder (WSL, `fsutil file setCaseSensitiveInfo`, or a card written on Linux), but then two groups share one session identity (`folder_key`, `stem_key`).
2. Files inside `NKSC_PARAM` other than its own `*.nksc` files: a stray JPEG there, a subfolder, a nested `NKSC_PARAM`.
3. A folder whose listing fails part-way (a card pulled, a bad sector).
4. A source folder that is itself a symlink or junction. File-engine §3.1 resolves a junction above the chosen folder once, with the volume identity that is not built yet.
5. A folder (not a file) whose name fails validation (file-engine §3.4 only names files).

## Considered options

* Keep the case-colliding folders apart and make their colliding groups ambiguous / merge them as one folder / exclude both
* Group photos found inside `NKSC_PARAM` as an ordinary folder / list them as unknown and never group them
* Group what a failed listing returned / report the folder as unreadable and group none of it
* Resolve a link at the root now / refuse it until volume identity resolves the chosen folder

## Decision outcome

Proposed, in each case the option that leaves the files where they are:

* **Case-colliding folders** stay separate folders: nothing is paired across them. A group whose folded folder and stem equal another group's becomes `ambiguous` (shown, decidable, planned with no operations). A streamed batch cannot see the other folder, so `Grouping::merge` marks these at the end of the scan.
* **`NKSC_PARAM` contents:** only the direct `*.nksc` files are matched, against the parent folder. Every other file inside an `NKSC_PARAM`, at any depth, is listed as `unknown_ext` and never grouped. A nested `NKSC_PARAM` is an ordinary folder.
* **A partial listing** makes the whole folder `unreadable`: none of its entries are grouped, because a partial list could split a JPEG from its NEF. An unreadable `NKSC_PARAM` is reported, and the parent's photos are grouped without those sidecars.
* **A link or placeholder as the source** is refused (`Refused(reparse_point)` or `Refused(cloud_placeholder)`) until the volume identity slice resolves the chosen folder once (file-engine §3.1 step 1).
* **A folder with an invalid name** is not entered and is reported as `invalid_name`, like a file.
* Smaller points, recorded in the code: when exclusions overlap, the reported reason is the first of unreadable, cloud placeholder, reparse point, invalid name, then the path rules (`_rejected`, destination, hidden folder name), then hidden or system, then `*.culla-partial`. In an ambiguous group the primary is the first JPEG (else NEF) in folded-name order; it is only what is shown. Device names include `COM0`, `LPT0`, `COM¹`–`COM³`, `LPT¹`–`LPT³`, `CONIN$` and `CONOUT$`.

### Consequences

* Good, because none of these cases can produce an operation that moves part of a shot, or a file Culla does not understand.
* Good, because every entry still ends up listed somewhere (group, unknown or excluded), so the review screen can account for the whole folder.
* Bad, because a group streamed as `ok` can turn `ambiguous` at the end of a scan in the case-colliding case, and a source chosen through a junction cannot be opened until volume identity lands.

Source: [SPEC.md §3.1, §3.3](../SPEC.md), [design/file-engine.md §3](../design/file-engine.md), [threat-model.md T2, T2b](../threat-model.md).
