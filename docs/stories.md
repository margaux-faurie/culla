# User stories

Status: draft, 24 Nov 2026. Stories for M1–M3 in full, headline stories for M4 (detailed before M4 starts). Milestones are in [roadmap.md](roadmap.md). Links point to requirement families in [nfr.md](nfr.md) (`NFR-PERF`, `NFR-REL`, `NFR-A11Y`) and invariant families in [design/file-engine.md](design/file-engine.md) (`INV`); exact IDs are filled in once those docs settle. Test levels are defined in [testing.md](testing.md).

Roles:
- **Photographer:** culls a card of Nikon JPEG + NEF files on Windows. The main user.
- **Maintainer:** builds and checks Culla (the author, or an agent working for her). Used for the headless M1 and M2 stories, which have no UI yet.

## M1 — Core engine headless

### US-01 Streamed scan
As a photographer, I want the first photo of a folder available about a second after I open it, so that I can start culling while the rest of the card is still being read.
- Given a folder with 3,000 files on a UHS-I SD card, when the scan starts, then the first group is yielded within the first-photo budget, before the scan has finished.
- Given the scan is still running, when groups are yielded, then they come in filename order.
- Given a folder with subfolders and "include subfolders" on, when it is scanned, then files in all subfolders are found.

Links: NFR-PERF. Test: adapters unit, `culla-bench` B-01.

### US-02 Group a JPEG, its NEF and its sidecars
As a photographer, I want a JPEG, its NEF and their sidecars treated as one photo, so that I decide once and the files never get split up.
- Given `DSC_0001.JPG`, `dsc_0001.nef`, `DSC_0001.xmp`, `DSC_0001.NEF.xmp` and `NKSC_PARAM/DSC_0001.NEF.nksc` in one folder, when the folder is scanned, then they form one group with the JPEG as primary, the NEF as raw, and the three sidecars attached.
- Given each sidecar, when it is attached, then it records the file it describes (`X.NEF.xmp` and the `.nksc` describe the NEF, `X.JPG.xmp` the JPEG, `X.xmp` the group).
- Given `DSC_0001.JPG` in `100NIKON/` and `DSC_0001.NEF` in `101NIKON/`, when scanned, then they are two separate groups.

Links: INV (grouping). Test: core unit, property.

### US-03 Lone NEF
As a photographer, I want a NEF without a JPEG shown like any other photo, so that RAW-only shots get culled too.
- Given a NEF with no matching JPEG, when the folder is scanned, then it forms a group with the NEF as primary.
- Given that group, when its preview is requested, then the largest embedded JPEG is returned.
- Given an HE or HE\* NEF, when its preview is requested, then the embedded JPEG is returned and no RAW decode is attempted.

Links: NFR-PERF, INV (grouping). Test: adapters unit, fuzz, real samples.

### US-04 Unknown files are listed, not touched
As a photographer, I want to know about files Culla does not handle, so that nothing on the card surprises me after an apply.
- Given a folder with `.MOV`, `.txt` and `.dat` files, when it is scanned, then they are not part of any group and appear in an "unknown files" list with their paths.

Links: INV (undecided untouched). Test: core unit.

### US-05 No self-ingestion
As a photographer, I want Culla to skip its own output folders, so that rejected or kept photos are not offered again.
- Given `<source>/_rejected/` exists, when the source is scanned with subfolders on, then nothing under `_rejected/` is scanned.
- Given the session destination is inside the source, when the source is scanned, then nothing under the destination is scanned.
- Given a junction or symlink inside the source, when it is scanned, then it is not followed.

Links: INV (path containment). Test: core unit, Windows integration.

### US-06 Capture-time order without reordering under me
As a photographer, I want photos ordered by capture time, so that bursts stay together, without the list changing under me once I start.
- Given a scan in filename order, when the EXIF pass finishes before the first decision, then the list is sorted by DateTimeOriginal.
- Given at least one decision has been made, when the EXIF pass finishes, then the order of the list does not change.
- Given a file without DateTimeOriginal, when sorting, then it keeps its filename position relative to its neighbours (rule to confirm in [design/file-engine.md](design/file-engine.md) or [architecture.md](architecture.md)).

Links: NFR-PERF. Test: core unit, B-03.

### US-07 Upright previews at screen size
As a photographer, I want each preview upright and sharp at full screen, so that I judge the photo, not the pipeline.
- Given a JPEG with EXIF Orientation 6 or 8, when its preview is built, then it is rotated upright.
- Given a 24 MP or 45 MP JPEG, when its preview is built, then the long edge is just above the screen size (2880×1800 on the G14) and the file is a JPEG.
- Given a JPEG whose declared dimensions exceed the decode limit, when its preview is built, then it fails with an error and does not allocate the full size.

Links: NFR-PERF, NFR-REL. Test: adapters unit (one fixture per Orientation value), fuzz.

### US-08 Prefetch and cache
As a photographer, I want the next photo already prepared when I advance, so that culling never waits on the card.
- Given a swipe cadence of one advance every 400 ms on a UHS-I card, when advancing through 200 photos, then next-photo latency stays within its budget at p95.
- Given a folder culled before, when it is opened again from the same card, then previews come from the disk cache in `%LOCALAPPDATA%\Culla\cache`.
- Given a file on removable media, when it is read, then it is read into buffers and never memory-mapped.

Links: NFR-PERF, NFR-REL. Test: `culla-bench` B-08 to B-10, code review for mmap (lint if possible).

### US-09 Identity survives a drive letter change
As a photographer, I want my session found again when the card comes back under another letter, so that I can stop and resume freely.
- Given a session for a folder on the volume with serial S at `E:\DCIM`, when the same card is mounted as `F:`, then the session and cache are found by S and the relative path.
- Given two different cards with the same folder names, when each is opened, then each gets its own session.

Links: NFR-REL. Test: core unit, Windows integration (VHDX remounted under another letter).

## M2 — File engine

### US-10 Decisions are an event log
As a photographer, I want every decision recorded in order, so that undo, redo and resume always agree.
- Given a sequence of decisions, undos and redos, when the log is folded, then the state matches the expected state in the golden fixture, in Rust and in TS.
- Given any prefix of a log, when it is folded, then the result is a valid state.
- Given a group that already has a decision, when a new decide event is attempted without an undo first, then it is rejected and the log is unchanged.

Links: INV (decision log). Test: property, golden.

### US-11 Session saved and reconciled
As a photographer, I want my decisions saved as I go and checked against the folder when I come back, so that I never lose work and never act on files that are gone.
- Given decisions made, when the app is killed and reopened on the same folder, then every decision made before the kill is restored.
- Given a stored session, when a file it refers to is missing, then the group is flagged missing and excluded from the plan.
- Given a stored session, when new files are present, then they are added as undecided.

Links: NFR-REL. Test: adapters unit (SQLite), core unit.

### US-12 Plan: rejects go to `_rejected/`
As a photographer, I want rejected photos moved to `_rejected/` with their folder structure, so that nothing is deleted and I can still recover them.
- Given rejected groups in `100NIKON/` and `101NIKON/`, when the plan is built, then their files go to `<source>/_rejected/100NIKON/` and `<source>/_rejected/101NIKON/`.
- Given any plan, when it is inspected, then it contains no operation that deletes a photo.

Links: INV (never delete, path containment). Test: property.

### US-13 Plan: keep by move or copy
As a photographer, I want kept photos moved or copied to my destination with their folder structure, so that my library matches the card's layout.
- Given keep = move and destination D, when the plan is built, then each kept file has a move op to D with its relative path mirrored.
- Given keep = copy, when the plan is built, then each kept file has a copy op and the source stays.
- Given undecided groups, when the plan is built, then they have no op.

Links: INV (undecided untouched, path containment). Test: property, snapshot.

### US-14 RAW follows JPEG toggle
As a photographer, I want to choose per session whether the NEF follows its JPEG, so that I can cull JPEGs and leave RAWs for later.
- Given RAW follows on, when a pair is kept or rejected, then the NEF and its sidecars get the same op as the JPEG.
- Given RAW follows off, when a pair is kept or rejected, then the NEF, `X.NEF.xmp` and the NEF's `.nksc` are left untouched, and the JPEG with `X.JPG.xmp` gets the op.
- Given a lone NEF, when it is decided, then it is moved like any primary, whatever the toggle.

Links: INV (RAW pairing, group consistency). Test: property, unit. Open: where `X.xmp` goes when RAW follows is off (see [design/file-engine.md](design/file-engine.md)).

### US-15 Collisions get a group suffix
As a photographer, I want name clashes at the destination handled without overwriting, so that a counter reset never costs me a photo.
- Given `D/DSC_0001.JPG` already exists, when a kept pair `DSC_0001` is planned, then JPG, NEF and sidecars all get `DSC_0001_1`.
- Given `_1` is also taken by any member, when planned, then the whole group gets `_2`.
- Given any plan, when inspected, then no two ops share a destination and no destination exists.

Links: INV (no overwrite, group consistency). Test: property.

### US-16 Preflight before anything runs
As a photographer, I want problems found before the first file moves, so that an apply does not stop halfway on something predictable.
- Given a write-protected card or a read-only file in the plan, when preflight runs, then it reports it and apply cannot start.
- Given less free space at the destination than the copy or cross-volume move needs, when preflight runs, then it reports the shortfall.
- Given a destination inside `_rejected`, a path over the length limit without `\\?\`, a locked file, or a symlink or junction in the plan, when preflight runs, then each is reported.

Links: NFR-REL, INV (path containment). Test: core unit, Windows integration (read-only VHDX, full VHDX, junctions).

### US-17 Same-volume move never overwrites
As a photographer, I want moves on the same drive to be instant and safe, so that culling an internal folder is fast and loses nothing.
- Given a same-volume move, when it executes, then it is a single `MoveFileExW` with only `MOVEFILE_WRITE_THROUGH` (no replace-existing, no copy-allowed).
- Given the target appeared after planning, when the move executes, then it fails as a hard error, the existing file is unchanged, the op is marked failed, and the apply continues with the next op.

Links: INV (no overwrite). Test: Windows integration.

### US-18 Cross-volume move is verified before the source goes
As a photographer, I want a move from my card to my laptop verified before the card copy is removed, so that a flaky reader cannot corrupt a photo.
- Given a cross-volume move, when it executes, then the file is copied to `*.culla-partial` while hashing, fsynced, then destination and source are both re-read and hashed, the hashes compared, the partial renamed into place, and only then is the source removed.
- Given the read-back hash differs, when verify runs, then the source is kept, the op fails and the partial is cleaned up.
- Given any removal, when it happens, then it goes through the single audited function, which accepts only a verified cross-volume source or a Culla `*.culla-partial` file.
- Given a copy or move, when it completes, then the mtime is preserved (±2 s on FAT-family volumes).

Links: INV (never delete, no overwrite), NFR-REL. Test: Windows integration, fault injection.

### US-19 Locked files are retried, then skipped
As a photographer, I want a file held by Explorer, Defender or Lightroom retried and then reported, so that one locked file does not stop the whole apply.
- Given a file held for 300 ms, when its op runs, then it succeeds after a retry.
- Given a file held for the whole apply, when its op runs, then it is marked failed after the backoff and the apply continues.

Links: NFR-REL. Test: Windows integration.

### US-20 Resume an interrupted apply
As a photographer, I want an apply that was cut off by a crash or a pulled card to finish when I reopen the app, so that I never end up with half-moved photos.
- Given an apply killed at a random point, when the app starts, then resume rolls the journal forward and every planned file ends up exactly once at its source or destination with its original hash.
- Given the card was pulled and comes back under another letter, when resume runs, then it identifies the volume by serial before touching anything.
- Given resume finished, when it runs again, then it does nothing.
- Given any apply, when it ends, then a plain-text log of operations is written.

Links: NFR-REL, INV (journal, never delete). Test: fault injection, crash loop, volume detach.

## M3 — Desktop swipe loop

### US-21 Start a session
As a photographer, I want to pick a folder and my options on one screen, so that I can start culling in a few seconds.
- Given the start screen, when I choose a source folder, include subfolders, RAW follows, move or copy, and a destination, then the swipe screen opens on the first photo.
- Given a previous session, when the start screen opens, then the last destination is pre-filled.

Links: NFR-A11Y. Test: Vitest, Playwright.

### US-22 Keep and reject by keyboard
As a photographer, I want → to keep and ← to reject and move on, so that I can cull with one hand.
- Given an undecided photo, when I press →, then it is kept and the next photo is shown.
- Given an undecided photo, when I press ←, then it is rejected and the next photo is shown.
- Given a decision, when it is made, then it is committed to the log at once and autosaved.

Links: NFR-PERF, NFR-A11Y. Test: Vitest, Playwright keyboard-only.

### US-23 Holding a key decides one photo
As a photographer, I want a held arrow key to count once, so that I never keep 40 photos by accident.
- Given I hold → for 2 s, when key repeat fires, then exactly one decision is recorded.

Links: NFR-REL. Test: Vitest, Playwright.

### US-24 Input is never blocked by animation
As a photographer, I want to keep pressing keys as fast as I like, so that the animation never slows me down.
- Given I press → five times within 300 ms on undecided photos, then five decisions are recorded in order, and at most 3 cards are mounted at any time.

Links: NFR-PERF. Test: Vitest, Playwright.

### US-25 Swipe with the trackpad
As a photographer, I want to drag the card left or right, so that culling feels physical when I use the trackpad.
- Given I drag the card past the offset threshold, or flick past the velocity threshold, then it commits keep (right) or reject (left).
- Given I release before either threshold, then the card springs back and nothing is recorded.
- Given a keyboard decision, then the same exit animation plays as for a drag.

Links: NFR-A11Y (non-drag alternative exists). Test: Vitest (threshold logic), Playwright.

### US-26 Arrows on a decided photo explain themselves
As a photographer, I want to be told why an arrow did nothing, so that I do not think the app is broken.
- Given a photo already kept or rejected, when I press ← or →, then the decision does not change, the card shakes briefly, and a toast says "Press U to undo".
- Given reduced motion is on, then the shake is replaced by a non-moving cue and the toast still shows.

Links: NFR-A11Y. Test: Vitest, Playwright.

### US-27 Undo
As a photographer, I want to undo the photo in front of me or my last action anywhere, so that a mistake costs one key.
- Given the current photo has a decision, when I press U, then it becomes undecided and an event is appended.
- Given I made decisions on photos 10, 11 and 12 and am on 13, when I press Ctrl+Z, then the decision on 12 is undone and photo 12 is shown.
- Given I pressed Ctrl+Z, when I press Ctrl+Shift+Z, then the decision is redone and that photo is shown.

Links: INV (decision log). Test: Vitest, golden, Playwright.

### US-28 Navigate without deciding
As a photographer, I want to move around without deciding, so that I can compare and come back.
- Given any photo, when I press Shift+→ or Shift+←, then the next or previous photo is shown and no decision is recorded.
- Given undecided photos remain, when I press N, then the next undecided photo is shown.
- Given the filter key F, when I press it, then the list cycles all, undecided, kept, rejected, and the current filter is shown as text.

Links: NFR-A11Y. Test: Vitest, Playwright.

### US-29 End of the list
As a photographer, I want to know what is left when I reach the end, so that I can finish or review.
- Given I pass the last photo, then a summary card shows "Jump to undecided (n)" and "Review".
- Given n is 0, then "Jump to undecided" is not offered.

Links: NFR-A11Y. Test: Vitest, Playwright.

### US-30 Progress and state at a glance
As a photographer, I want counts and the current photo's state always visible, so that I know where I am.
- Given the swipe screen, then it shows n / total, kept, rejected and undecided counts, and the current decision badge.
- Given a decision, then the badge uses a label and a shape (filled for keep, outline for reject) as well as colour.

Links: NFR-A11Y. Test: Playwright + axe.

### US-31 Shortcut overlay
As a photographer, I want to see the keys when I forget them, so that I do not need the README.
- Given the swipe screen, when I press ?, then an overlay lists every shortcut; Esc closes it and focus returns where it was.

Links: NFR-A11Y. Test: Playwright + axe.

### US-32 Reduced motion
As a photographer who turned off animations in Windows, I want Culla to respect that, so that culling does not make me queasy.
- Given `prefers-reduced-motion: reduce`, then the swipe exit, spring and shake are replaced by instant or fade transitions, and all functions still work.

Links: NFR-A11Y. Test: Playwright (emulated media).

### US-33 Previews come only through `culla://`
As a maintainer, I want images served only by the custom scheme for IDs from the current scan, so that the webview never reads arbitrary files.
- Given a preview ID from the current scan, when the webview requests `culla://preview/<id>`, then the encoded preview is returned from memory.
- Given an unknown ID or a path-like value, then the request is refused.
- Given the CSP, then `img-src` allows only that origin.

Links: NFR-REL, threat model ([threat-model.md](threat-model.md)). Test: `src-tauri` smoke, adapters unit.

## M4 — Headline stories

To be detailed with acceptance criteria before M4 starts.

| ID | Story | Links |
|---|---|---|
| US-34 | As a photographer, I want a review screen with counts, kept and rejected grids, the full plan, preflight results, unknown files and a time and size estimate, so that I know exactly what apply will do. | INV, NFR-PERF |
| US-35 | As a photographer, I want apply to ask for confirmation stating the counts, so that two keystrokes never move thousands of files. | NFR-REL |
| US-36 | As a photographer, I want per-file progress with pause and cancel (after the current file), and a report at the end, so that a long cross-volume apply stays under my control. | NFR-REL, INV |
| US-37 | As a photographer, I want to resume a session or start fresh when I reopen a folder, including on another drive letter, so that I can cull a card over several sittings. | NFR-REL |
| US-38 | As a photographer, I want an interrupted apply resumed with a clear message when the app opens, so that I know my photos are safe. | NFR-REL, INV |
| US-39 | As a photographer, I want to zoom to 100 % at the cursor (Z toggle or hold Space) and pan with the arrows, so that I can check focus before deciding. | NFR-PERF, NFR-A11Y |
| US-40 | As a photographer, I want an EXIF overlay (I) with date, lens, exposure, so that I can tell near-identical shots apart. | NFR-A11Y |
