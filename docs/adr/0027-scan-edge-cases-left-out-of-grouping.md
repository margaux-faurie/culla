---
status: accepted
date: 2026-11-27
decision-makers: Margaux Faurie (accepted 27 Nov 2026)
---

# Never split a shot: how the scan handles what it cannot see completely

Refines [ADR 0007](0007-pair-files-by-basename-within-one-folder.md) and [design/file-engine.md](../design/file-engine.md) §3.1–§3.3. Proposed in the M1 scan and grouping slice (`culla_core::grouping`, `culla_adapters::scan`); waits for Margaux.

## Context and problem statement

The first scan and grouping code met cases that SPEC §3 and file-engine §3 leave open. In each, the scan cannot be sure it sees every file of a shot, or sees a file it should not touch. Guessing about a user's files is not allowed (file-engine §3.3), and a plan that moves a JPEG without its NEF or its edits splits a shot. Which rules does the scan follow?

1. Two folders whose names fold to the same value (`100NIKON` and `100nikon`, only in a case-sensitive folder): two groups would share one session identity (`folder_key`, `stem_key`).
2. Two members of one group whose paths fold alike (`X.xmp` and `x.XMP`, `X.NEF.xmp` and `x.nef.xmp`, two `NKSC_PARAM` folders differing by case): on Windows they would land on one target.
3. A JPEG or NEF that is excluded for its own attributes (hidden, cloud placeholder, link, unreadable, invalid name) next to a group with the same stem.
4. A folder that cannot be listed completely: an entry whose type or metadata cannot be read, a listing that fails part-way, a folder over the entry cap, or a folder that became a link between its parent's listing and its own.
5. An `NKSC_PARAM` that cannot be read through: unreadable, a link, a placeholder.
6. Files inside `NKSC_PARAM` other than its own `*.nksc` files.
7. A source folder that is itself a link; a folder (not a file) with an invalid name.
8. Huge or deep trees (threat model T19).
9. Where the walk lives before the `FileSystem` port and `winfs` exist.

## Considered options

* Group what is visible and report the rest
* Hold back what is in doubt: a group becomes `ambiguous` (shown, decidable, no operations), or a folder groups nothing
* Refuse the whole scan

## Decision outcome

Proposed: "Hold back what is in doubt", because it never splits a shot and still lets the user see and decide everything else. The guiding rule is: never group partially.

* **Ambiguous groups** (shown, decidable, planned with no operations): two JPEGs or two NEFs with one folded stem (file-engine §3.3, unchanged); two members whose paths fold alike (case 2, checked when grouping and again in `Grouping::merge`); a JPEG or NEF with the group's stem excluded for its own attributes in the same folder (case 3; photos excluded because of their folder do not count, nor files inside `NKSC_PARAM`); a group whose folded folder and stem equal another group's (case 1; the folders stay separate and nothing is paired across them, and `merge` marks these at the end of the scan because a streamed batch cannot see the other folder).
* **Held-back folders** (nothing grouped, every file excluded as `held_back` or `too_large`, the cause listed next to them): a folder with an entry that could not be described (case 4); a folder whose `NKSC_PARAM` is skipped, unreadable, a link or a placeholder, or is anything but a plain file or folder (case 5); a folder with more entries than the cap.
* **Unlisted folders:** the adapter lists a folder completely or not at all. A failed or over-cap listing, or a folder that is now a link, a placeholder or not a folder when its turn comes (`symlink_metadata` just before listing), is reported once against its parent as unlisted, with nothing below it emitted. The root failing this way fails the scan.
* **`NKSC_PARAM` contents:** only the direct `*.nksc` files are matched, against the parent folder. Every other file inside an `NKSC_PARAM`, at any depth, is listed as `unknown_ext` and never grouped. A nested `NKSC_PARAM` is an ordinary folder. `NKSC_PARAM` counts as part of its parent for the depth cap and for "include subfolders".
* **A link or placeholder as the source** is refused until the volume identity slice resolves the chosen folder once (file-engine §3.1 step 1). **A folder with an invalid name** is not entered and is reported as `invalid_name`, like a file.
* **Caps** (threat model T19), in `ScanPolicy`: `max_depth = 32` folders below the source (a card is `DCIM\100NIKON`, depth 2; archives rarely pass 6), and `max_entries_per_folder = 100_000` (a DCF folder holds at most 9,999 files; with a JPEG, a NEF and three sidecars per shot that is under 50,000). A deeper folder is not entered and is reported as `too_deep`; listing stops at the first entry over the cap and the folder is reported as `too_large`. Entries of a folder share one copy of its path components, so 100,000 entries stay within tens of MB.
* **New `scan_issue` kinds:** `too_deep`, `too_large` and `held_back`, to add to the schema (file-engine §2.3) with the session store.
* **Where the walk lives:** for now in `culla_adapters::scan`, over `std::fs` (`read_dir`, non-following `DirEntry` metadata, `symlink_metadata`; no new crate). In M2 it moves behind the core `FileSystem` port, implemented in `winfs` with handle-relative listing (`FILE_FLAG_OPEN_REPARSE_POINT`), so the recording fake (testing.md §4) covers the scan like the rest of a session. Until then, scan output is not trusted against swaps (threat model T2): preflight and every apply step re-check each path component.
* Smaller points, recorded in the code:
  * Until `winfs` reads the reparse tag, a hydrated cloud file that carries neither `RECALL_ON_*` nor `OFFLINE` is reported as `reparse_point`, not `cloud_placeholder`; it is excluded either way.
  * An entry that is neither a file nor a folder (a device, a socket) is reported as `unreadable`.
  * When exclusions overlap, the reported reason is the first of: the folder's own path rules (invalid name, `_rejected`, destination, hidden folder name, too deep, subfolders off), unreadable or unlisted, cloud placeholder, reparse point, invalid name, not a file, hidden or system, `*.culla-partial`.
  * In an ambiguous group the primary is the first JPEG (else NEF) in folded-name order; it is only what is shown.
  * Device names include `COM0`, `LPT0`, `COM¹`–`COM³`, `LPT¹`–`LPT³`, `CONIN$` and `CONOUT$`.

### Consequences

* Good, because none of these cases can produce an operation that moves part of a shot, or a file Culla does not understand.
* Good, because every entry still ends up listed somewhere (group, unknown or excluded), so the review screen can account for the whole folder.
* Bad, because one unreadable file, a broken `NKSC_PARAM` or a stray hidden NEF keeps photos out of the plan until the user fixes the cause and rescans; the review must say why.
* Bad, because a group streamed as `ok` can turn `ambiguous` at the end of a scan in the case-colliding case, and a source chosen through a junction cannot be opened until volume identity lands.

Source: [SPEC.md §3.1, §3.3](../SPEC.md), [design/file-engine.md §3](../design/file-engine.md), [threat-model.md T2, T2b, T19](../threat-model.md).
