# User stories

Status: draft, 24 Nov 2026. Stories for M1–M4 in full (the M4 criteria were added on 28 Nov 2026 and carry proposals awaiting Margaux, marked in the text), and one M6 story. IDs are stable: stories added later get the next free number (US-41 onward), whatever their milestone. Milestones are in [roadmap.md](roadmap.md). Links point to requirement families in [nfr.md](nfr.md) (`NFR-PERF`, `NFR-REL`, `NFR-A11Y`) and invariant families in [design/file-engine.md](design/file-engine.md) (`INV`); exact IDs are filled in once those docs settle (the M4 stories already use them). Numbers marked "(est.)" are repeated from nfr.md so the criteria can be checked here. Test levels are defined in [testing.md](testing.md).

Roles:
- **Photographer:** culls a card of Nikon JPEG + NEF files on Windows. The main user.
- **Maintainer:** builds and checks Culla (the author, or an agent working for her). Used for the headless M1 and M2 stories, which have no UI yet.

## M1 — Core engine headless

### US-01 Streamed scan
As a photographer, I want the first photo of a folder available about a second after I open it, so that I can start culling while the rest of the card is still being read.
- Given a folder with 3,000 files on a UHS-I SD card, when the scan starts, then the first group is yielded and its preview is ready in 1.5 s or less (est.), before the scan has finished.
- Given the same folder, when the scan and the EXIF pass run to the end, then they finish in 15 s or less (est.).
- Given the scan is still running, when groups are yielded, then they come in filename order.
- Given a folder with subfolders and "include subfolders" on, when it is scanned, then files in all subfolders are found.

Links: NFR-PERF. Test: adapters unit, `culla-bench` B-01.

### US-02 Group a JPEG, its NEF and its sidecars
As a photographer, I want a JPEG, its NEF and their sidecars treated as one photo, so that I decide once and the files never get split up.
- Given `DSC_0001.JPG`, `dsc_0001.nef`, `DSC_0001.xmp`, `DSC_0001.NEF.xmp` and `NKSC_PARAM/DSC_0001.NEF.nksc` in one folder, when the folder is scanned, then they form one group with the JPEG as primary, the NEF as raw, and the three sidecars attached.
- Given each sidecar, when it is attached, then it records the file it describes (`X.NEF.xmp` and the `.nksc` describe the NEF, `X.JPG.xmp` the JPEG, bare `X.xmp` and `X.pp3` the NEF if the group has one and otherwise the JPEG).
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
- Given a file without DateTimeOriginal, when sorting, then it falls back to `CreateDate`, then to the file time; a photo with no time at all keeps its filename position relative to its neighbours (rules in [ADR 0028](adr/0028-capture-time-sources-and-order.md)).

Links: NFR-PERF. Test: core unit, B-03.

### US-07 Upright previews at screen size
As a photographer, I want each preview upright and sharp at full screen, so that I judge the photo, not the pipeline.
- Given a JPEG with EXIF Orientation 6 or 8, when its preview is built, then it is rotated upright.
- Given a 24 MP or 45 MP JPEG, when its preview is built, then it fits the screen box (2880×1800 on the G14) in its own orientation without upscaling, filling the height for a 3:2 photo (2700×1800, or 1800×2700 portrait), and the file is a JPEG.
- Given a JPEG whose declared dimensions exceed the decode limit, when its preview is built, then it fails with an error and does not allocate the full size.

Links: NFR-PERF, NFR-REL. Test: adapters unit (a JPEG built in the test for each Orientation value, not a fixture), core unit (sizes, caps, orientation), fuzz (`jpeg_header`).

### US-08 Prefetch and cache
As a photographer, I want the next photo already prepared when I advance, so that culling never waits on the card.
- Given a swipe cadence of one advance every 400 ms on a UHS-I card, when advancing through 200 photos, then next-photo latency is 50 ms or less at p95 (est.) for prefetched photos, and peak memory stays at 1 GB or less (est.).
- Given a folder culled before, when it is opened again from the same card, then previews come from the disk cache in `%LOCALAPPDATA%\Culla\cache`.
- Given a file on removable media, when it is read, then it is read into buffers and never memory-mapped.

Links: NFR-PERF, NFR-REL. Test: `culla-bench` B-08 to B-10, core unit (`prefetch`), adapters integration (`tests/pipeline.rs`: prefetch, cancellation, cache capacity, memory budgets), code review for mmap (lint if possible).

### US-09 Identity survives a drive letter change
As a photographer, I want my session found again when the card comes back under another letter, so that I can stop and resume freely.
- Given a session for a folder on the volume with serial S at `E:\DCIM`, when the same card is mounted as `F:`, then the session and cache are found by S and the relative path.
- Given two different cards with the same folder names, when each is opened, then each gets its own session.

Links: NFR-REL. Test: core unit, Windows integration (VHDX remounted under another letter).

## M2 — File engine

### US-10 Decisions are an event log
As a photographer, I want every decision recorded in order, so that undo, redo and resume always agree.
- Given a sequence of decisions, undos and redos, when the log is folded in Rust, then the state matches the expected state in the golden fixture. (The TS half of the golden tests lands with the TS reducer in M3, see US-27.)
- Given any prefix of a log, when it is folded, then the result is a valid state.
- Given a group that already has a decision, when a new decide event is attempted without an undo first, then it is rejected and the log is unchanged.

Links: INV (decision log). Test: property, golden.

### US-11 Session saved and reconciled
As a photographer, I want my decisions saved as I go and checked against the folder when I come back, so that I never lose work and never act on files that are gone.
- Given decisions made, when the app is killed and reopened on the same folder, then every decision made before the kill is restored.
- Given a stored session, when a file it refers to is missing, then the group is flagged missing and excluded from the plan.
- Given a stored session, when new files are present, then they are added as undecided.
- Given a kept or rejected group, when a new file joins it on reopen (for example NX Studio wrote `NKSC_PARAM/X.NEF.nksc`), then the group becomes undecided, the review says why, and Ctrl+Z does not bring the old decision back ([ADR 0024](adr/0024-reconcile-event-for-a-file-joining-a-decided-group.md)).

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
- Given RAW follows off, when a pair is kept or rejected, then the NEF, `X.NEF.xmp`, `NKSC_PARAM/X.NEF.nksc` and a bare `X.xmp` or `X.pp3` are left untouched, and the JPEG with `X.JPG.xmp` and `NKSC_PARAM/X.JPG.nksc` gets the op ([ADR 0023](adr/0023-bare-sidecars-follow-the-nef.md)).
- Given a bare `X.xmp` or `X.pp3` next to a JPEG with no NEF, when the JPEG is decided, then it follows the JPEG ([SPEC.md](SPEC.md) section 3.1).
- Given a lone NEF, when it is decided, then it is moved like any primary, whatever the toggle.

Links: INV (RAW pairing, group consistency). Test: property, unit.

### US-15 Collisions get a group suffix
As a photographer, I want name clashes at the destination handled without overwriting, so that a counter reset never costs me a photo.
- Given `D/DSC_0001.JPG` already exists, when a kept pair `DSC_0001` is planned, then JPG, NEF and sidecars all get `DSC_0001_1`.
- Given `_1` is also taken by any member, when planned, then the whole group gets `_2`.
- Given a suffixed group, when planned, then the suffix is applied inside sidecar names too (`DSC_0001_1.NEF.xmp`, `NKSC_PARAM/DSC_0001_1.NEF.nksc`).
- Given any plan, when inspected, then no two ops share a destination and no destination exists.

Links: INV (no overwrite, group consistency). Test: property.

### US-16 Preflight before anything runs
As a photographer, I want problems found before the first file moves, so that an apply does not stop halfway on something predictable.
- Given a write-protected card, when preflight runs, then it reports it and apply cannot start.
- Given a camera-protected (read-only) file in a kept or rejected group, when the plan is built, then the whole group is planned as a copy, the originals stay in place, and the review and report say so.
- Given a destination inside a OneDrive or other cloud-synced folder, when preflight runs, then the review shows a warning and apply is still allowed.
- Given less free space at the destination than the copy or cross-volume move needs, when preflight runs, then it reports the shortfall.
- Given a destination inside `_rejected`, a path over the length limit without `\\?\`, a locked file, or a symlink or junction in the plan, when preflight runs, then each is reported.

Links: NFR-REL, INV (path containment). Test: core unit, Windows integration (read-only VHDX, full VHDX, junctions).

### US-17 Same-volume move never overwrites
As a photographer, I want moves on the same drive to be instant and safe, so that culling an internal folder is fast and loses nothing.
- Given a same-volume move, when it executes, then it is a single no-replace rename through the checked handle (ADR 0022).
- Given the target appeared after planning, when the move executes, then it fails as a hard error, the existing file is unchanged, the op is marked failed, and the apply continues with the next op.

Links: INV (no overwrite). Test: Windows integration.

### US-18 Cross-volume move is verified before the source goes
As a photographer, I want a move from my card to my laptop verified before the card copy is removed, so that a flaky reader cannot corrupt a photo.
- Given a cross-volume move, when it executes, then the file is copied to `*.culla-partial` while hashing, fsynced, then destination and source are both re-read and hashed, the hashes compared, the partial renamed into place, and only then is the source removed.
- Given verify, when it reads the destination back (hash 2) and re-reads the source (hash 3), then it opens both with `FILE_FLAG_NO_BUFFERING`, so the OS cache cannot answer for either disk.
- Given another file took the target name before the final rename, when the rename fails, then the partial is removed through `remove_verified`, the op ends `failed`, the source is kept and the other file is unchanged.
- Given any of the three hashes differs, when verify runs, then the source is kept, the op fails and the partial is cleaned up.
- Given any removal, when it happens, then it goes through `remove_verified`, which accepts only a verified cross-volume source or a Culla `*.culla-partial` file.
- Given a copy or move, when it completes, then the mtime is preserved (±2 s on FAT-family volumes).

Links: INV (never delete, no overwrite), NFR-REL. Test: Windows integration, fault injection.

### US-19 Locked files are retried, then skipped
As a photographer, I want a file held by Explorer, Defender or Lightroom retried and then reported, so that one locked file does not stop the whole apply.
- Given a file held for 300 ms, when its op runs, then it succeeds after a retry.
- Given a file held for the whole apply, when its op runs, then it is marked failed after the backoff and the apply continues.

Links: NFR-REL. Test: Windows integration.

### US-20 Resume an interrupted apply
As a photographer, I want an apply that was cut off by a crash or a pulled card to finish when I reopen the app, so that I never end up with half-moved photos.
- Given an apply killed at a random point, when the app starts, then resume rolls the journal forward and every op ends in the end state for its kind ([testing.md](testing.md) section 6.1): a move leaves the file at exactly one place, a copy leaves it at both, always with its original hash.
- Given an op that had reached `verifying` with a matching partial, or `committed`, when resume runs, then it completes the rename into place (and, cross-volume, the source removal once the group is committed) and ends `done` (a same-volume rename ends `done` when the plan closes); it is never rolled back or re-copied.
- Given a same-volume rename that the file system reverted after the plan closed (the file is back at its source), when the app next launches, then it shows the file as a reverted move and offers "Redo moves"; nothing runs until I confirm, and the redo is a new small plan journaled like any other.
- Given a move between volumes whose source removal was lost after the plan closed (the file is at both places), when I next open Culla, then it is listed and "Finish moves" is offered; confirming re-verifies both files and then removes the source; a mismatch keeps both and says why.
- Given the card was pulled and comes back under another letter, when resume runs, then it identifies the volume by serial before touching anything.
- Given resume finished, when it runs again, then it does nothing.
- Given any apply, when it ends, then a plain-text log of operations is written.

Links: NFR-REL, INV (journal, never delete). Test: fault injection, crash loop, volume detach.

### US-41 Nothing touches the disk before Apply
As a photographer, I want Culla to leave my card exactly as it was until I confirm Apply, so that I can open, cull and close without any risk.
- Given a full session (scan, EXIF pass, decisions with undo and redo, review, preflight) driven through a recording `FileSystem` port, then the port records zero mutating calls under the source or the destination.
- Given the same session on real volumes, closed without applying, then the hash, mtime and attributes of every file in the source and destination trees are unchanged, and no file was added or removed there.
- Given any session, then everything Culla writes before Apply (sessions, cache, logs) is under `%LOCALAPPDATA%\Culla`, never on the card.

Links: INV (nothing before Apply), NFR-REL. Test: core unit (recording port), Windows integration ([testing.md](testing.md) section 4).

## M3 — Desktop swipe loop

### US-21 Start a session
As a photographer, I want to pick a folder and start sorting at once, so that I start culling in a few seconds and set where photos go when I apply. (Decided by Margaux 28 Nov 2026, ADR 0029; this replaces the version of 27 Nov where every option and the route were on the start screen and move or copy was chosen fresh each session.)
- Given the start screen, then it shows only "What do you want to sort?", "Choose a folder" and the recent folders: no route, no options (not even folded), no move or copy, no destination and no Start button.
- Given the start screen, when I choose a folder in the dialog or click a recent folder, then the swipe screen opens on the first photo, with subfolders included.
- Given a new session, then "RAW follows JPEG" is on, move or copy is my last choice, and the destination is the one used for this folder, otherwise the last one used anywhere; nothing is asked.
- Given a first run, then no destination and no move or copy are set, and I choose both on review before kept photos can be applied.
- Given only rejected photos, then Apply works with neither move or copy nor a destination chosen.
- Given the remembered destination is missing (drive unplugged), then review shows it as not available and I must choose another destination before kept photos can be applied.
- Given the review screen, then "RAW follows JPEG" is in a folded "Options", on by default, and I can change it until Apply.
- Given no destination yet, then the swipe screen still says where kept photos go, honestly. **(proposed)** Under Keep it reads "You choose where on Review".
- **(proposed)** Given kept photos that would get operations, when move or copy or a connected destination is missing, then Apply stays disabled with a hint saying which, and no plan is shown; kept photos held back (applied, missing) do not count.
- **(proposed)** Given I choose move or copy or a destination on review, then they are remembered at once and again when I confirm; the remembered destination is labelled "same as last time".
- **(proposed)** Given a recent folder whose drive is not connected, or a folder with no photos, when I choose it, then the start screen stays and says why.

Links: NFR-A11Y. Test: Vitest, Playwright.

### US-22 Keep and reject by keyboard
As a photographer, I want → to keep and ← to reject and move on, so that I can cull with one hand.
- Given an undecided photo, when I press →, then it is kept and the next photo is shown.
- Given an undecided photo, when I press ←, then it is rejected and the next photo is shown.
- Given a decision, when it is made, then it is committed to the log at once and autosaved.
- Given an AZERTY or QWERTZ layout, when I press U, N or Ctrl+Z, then they act as labelled on the keys (undo, next undecided, global undo); the arrows keep and reject on any layout ([SPEC §4.1](SPEC.md#41-keyboard-map)).

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
- Given a photo already kept or rejected, when I drag past a threshold, then the card springs back, no event is recorded, and the "Press U to undo" toast shows ([ADR 0006](adr/0006-arrow-keys-do-not-change-an-existing-decision.md)).
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

- Given the golden fixtures from M2, when the TS reducer folds each log, then it gives the same state as the Rust reducer.

Links: INV (decision log). Test: Vitest, golden (TS half), Playwright.

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
- Given the swipe screen, then under the Keep and Reject buttons it shows where the current photo would go (example path, following "RAW follows JPEG"), or "You choose where on Review" under Keep while no destination is chosen (ADR 0029); on an already decided photo it shows where it will go without implying the arrows act ([ADR 0006](adr/0006-arrow-keys-do-not-change-an-existing-decision.md)).

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

### US-42 Keep and Reject buttons (M3)
As a photographer who does not want to drag or use the keyboard, I want visible Keep and Reject buttons, so that I can cull with single clicks (WCAG 2.5.7).
- Given an undecided photo on the swipe screen, when I click Keep, then it is kept and the next photo is shown, with the same exit animation as →.
- Given an undecided photo, when I click Reject, then it is rejected and the next photo is shown.
- Given a photo that already has a decision, when I click either button, then the decision does not change and the "Press U to undo" toast shows, as for the arrows.
- Given the buttons, then each has a visible text label, is reachable by Tab, shows a focus ring, and has a target size of at least 24×24 CSS px.

Links: NFR-A11Y. Test: Playwright click-only flow (cull 10 photos with the mouse only, no drag, no keys), axe, Vitest.

### US-44 Choosing the folder is part of the experience (M3)
As a photographer, I want the start screen to help me pick what to cull, so that starting a session feels like part of the app, not a file dialog. (Principle decided by Margaux, 27 Nov 2026. For v1 she chose recent folders with their status, plus the native dialog; detected drives, the found-photos preview and drag and drop move to v2.)
- (v2) Given an SD card or removable drive is plugged in, when I open Culla, then the start screen shows it with its volume label and photo count, and I can start on it in one click.
- Given folders I culled before, then they are listed as recent folders with their session status (in progress, applied), and I can resume one.
- (v2) Given I drag a folder onto the window, then it becomes the source.
- (v2) Given a source is chosen, then a preview shows what was found (JPEG and NEF counts, lone NEFs, date range) before I start.
- Given I prefer it, then the native folder dialog, opened from Rust, is still available.
- Given any of these, then detection, dropped paths and folder reads stay in Rust: the webview only receives opaque source IDs and display data, never a path, and gets no filesystem or dialog permission (CLAUDE.md invariant, threat model T7). Detection and counting are read-only on the card (INV-03).

Links: NFR-A11Y, NFR-PERF. Test: Vitest on the start screen with DemoBackend sources, Playwright, adapter test for removable-drive detection on Windows (v2).

## M4 — Review and apply UI, sessions, zoom, EXIF

Acceptance criteria written 28 Nov 2026 by an agent, to be reviewed like any other doc PR before M4 starts ([roadmap.md](roadmap.md), M4 entry criteria). Each scenario names the doc that fixes it. A scenario or a detail marked **(proposed)** is proposed by the agent and awaiting Margaux, and is listed in [SPEC §10](SPEC.md#10-open-questions); everything else restates SPEC, the ADRs, [design/file-engine.md](design/file-engine.md) (cited as FE), [PRD.md](PRD.md) or [nfr.md](nfr.md). Quoted UI copy is the design canvas's (Review, Apply, Decided boards, SPEC §5.6) or the merged UI's where they have it; any other wording is proposed. Display paths are the backend's display strings, printed as they are (SPEC §10, "Backend shape for display paths").

Tests named here:
- **Vitest:** pure `.ts` modules with no DOM (review model, confirm text, progress and estimate formatting, zoom geometry, EXIF formatting), against `DemoBackend` where a backend is needed.
- **Playwright + axe:** the demo build (`pnpm build:demo && pnpm e2e`), with no serious or critical axe violation on each new screen and state, as `e2e/helpers.ts` checks today. **(proposed)** The demo backend offers named scenarios for the states a demo folder cannot reach on its own (write-protected card, held groups, card pulled mid-apply, interrupted apply at launch, launch check findings). Margaux to confirm: choose them by a query parameter in the demo build only, or by a demo-only build flag or another route?
- **Rust integration:** `culla-core` tests over the fake `Disk` and `Store` ports, and the `windows-it` suite on `windows-latest` with VHDX volumes ([testing.md](testing.md) §4 to §6).
- **Manual:** the checklist in [testing.md](testing.md) §8, on the G14.

### US-34 Review before anything moves
As a photographer, I want a review screen headed by the route, with counts, kept and rejected grids, the full plan, preflight results, unknown files and a time and size estimate, so that I know exactly what apply will do before any file moves.

Opening and layout
- Given the swipe screen, when I press Enter or click Review, then the review screen opens on cream (SPEC §5.6), focus goes to its heading, and no file on the source or destination has changed (INV-03, NFR-REL-01).
- Given the review screen, then the route view heads it, the same as on the swipe and apply screens: From (source, volume, photo count), Kept (moved or copied, destination, one real example path with its JPEG, NEF and edit files), Rejected (`<source>\_rejected`, marked "new folder" only while it does not exist, "nothing deleted") and "undecided · stay where they are" (SPEC §2, "Where photos go"; canvas Review).
- Given the route, then each branch gives its counts as on the canvas: photos, files by kind and bytes ("96 JPEG · 96 NEF · 12 edit files · 2.1 GB"), and per mirrored subfolder its photo count and whether it is a new folder ("Barcelona · 58 photos · new folder").
- Given the review screen, then the full folder trees and the per-file list ("See every file (286)") are one activation away (SPEC §2); the per-file list shows every operation of the plan with its source, target (suffix included) and kind.
- Given "RAW follows JPEG" is off, then the example and the per-file list show the NEF, its sidecars and bare `X.xmp`/`X.pp3` as staying in the source folder (ADR 0023, INV-08).
- Given the review screen, then the kept and rejected grids show each decided photo's preview; activating one (click, or Enter on the focused thumbnail) goes back to the swipe screen on that photo, with no decision recorded (SPEC §4.2). **(proposed)** The filter is set to "all" on the way back, so the photo is always reachable.
- Given the review screen, when I press Esc or click "Back to sorting", then the swipe screen comes back on the photo I left (SPEC §4.1, canvas). Given undecided photos remain, "Sort them now" takes me to the first undecided photo (canvas).
- Given the review screen, then the single-key swipe shortcuts (U, N, Z, I, F, ?) do nothing there (NFR-A11Y-10).

Plan and preflight
- Given decisions, when the review opens, then a draft plan is built and preflighted, and whenever I go back, change decisions and return, it is rebuilt; only the last draft can be confirmed (FE §7.4). **(proposed)** While plan or preflight runs, the plan area shows loading dots ("Checking"), Apply stays disabled, and no main-thread task takes over 50 ms (NFR-PERF-13).
- Given a name already taken at the destination or in `_rejected`, then the review says how many and the suffix, for example "2 names are taken in Selects 2026. Those photos get _1 (MGF_7101_1). Nothing is replaced." (canvas, US-15, NFR-REL-02); every member of such a group shows the same suffix (INV-07).
- Given a camera-protected (read-only) file among the files a group moves, then the review lists the group as "camera-protected: copied, original kept" (FE §7.2) and the "Worth knowing" list says "1 kept photo is protected on the camera. It is copied, and the original stays on the photos drive." (canvas). The same holds for a reject, copied into `_rejected` (FE §7.2). **(proposed)** Only a read-only file that moves counts; one that stays because RAW does not follow does not (SPEC §10, M3 item).
- Given files Culla does not handle (unknown extensions, orphan sidecars, same-basename unknown files, invalid names, foreign partials), then they are listed with their paths and are not in the plan (INV-20, US-04), for example "3 other files (.mov, .txt) are left alone." (canvas).
- Given a group whose stored files changed since the scan, are missing, or that became undecided because a file joined it on reopen, then the review lists each with its reason ("changed since scan, rescan", missing, the reconcile reason of ADR 0024) and plans no operation for it (FE §6.2, US-11, INV-04).
- Given a destination inside OneDrive or another cloud-synced folder, then the review shows a warning and apply is still allowed (P17, SPEC §2). Given a locked source (P12) or a target path over 260 characters (P15), then a warning is listed and apply is still allowed (FE §8).
- Given a preflight blocker: the source or destination volume not bound to exactly one mounted volume (P1, P2), a write-protected card (P3, the message names the lock switch), a read-only destination (P4), not enough free space (P5, with the shortfall), a destination that is the source, inside `_rejected`, a placeholder or a refused volume type (P6), a P7 path bug, or the preview pipeline still busy (P16, the apply waits); then the review names each blocker in words, Apply is disabled, and the reason is shown next to it (NFR-REL-08, US-16). **(proposed)** Two mounted volumes sharing a serial (P1, P2) show a list of volumes with labels and drive letters to pick from (FE §6.3 step 2).
- Given a group held back by preflight (P8 name or path too long, P9 link or placeholder, P10 changed since scan, P14 over 4 GiB to FAT32), then it is listed with its reason, it gets no operation, and apply is allowed for the rest (FE §8, `Report::runnable`); the counts and the confirm dialog exclude it.
- Given a target taken since the plan was built (P11), then the plan is rebuilt and the review shows the new name before I confirm (FE §8).
- Given only rejected photos and no destination chosen, then the plan holds the rejects and apply is allowed **(proposed, SPEC §10 `plan()` (c))**.
- Given no kept or rejected photo that would get an operation, then Apply is disabled and the review says why **(proposed: "Nothing to apply yet. Keep or reject some photos first.")**.
- Given a group already applied by an earlier plan, then it gets no operation (INV-04) and is listed as applied **(proposed)**.

Estimate
- Given a plan with copies or cross-volume moves, then the review shows the bytes to write, exact to the byte, and a time estimate that counts the second read of each source (SPEC §9, NFR-PERF-12), for example "Kept photos change drive, so each copy is read back and checked before its original leaves E:. About 4 minutes." (canvas).
- Given a plan of renames only (rejects, keeps on the same volume), then the estimate says it takes seconds **(proposed wording)**.
- **(proposed)** Until B-13 has measured the reader's throughput, the time estimate uses a default of 90 MB/s for removable sources and is shown as "About …"; NFR-PERF-12's ±25 % applies once measured throughput replaces it.

Safety
- Given any plan shown, then it contains no operation that removes a photo, none for an undecided group, and every target lies under the destination or `<source>\_rejected` (INV-04, INV-05, US-12, US-13).

Links: NFR-REL-01, NFR-REL-02, NFR-REL-05, NFR-REL-07, NFR-REL-08, NFR-PERF-12, NFR-PERF-13, NFR-A11Y-01, NFR-A11Y-04, NFR-A11Y-06, NFR-A11Y-08, NFR-A11Y-10, NFR-A11Y-11; INV-03 to INV-09, INV-20. Test: Vitest (review model from a plan and a preflight report: counts per branch and subfolder, bytes, held, warnings, blockers, the estimate, Apply's enabled state); Playwright + axe (open with Enter and the button, Esc back, grid thumbnail jumps back, "Sort them now", per-file list, RAW follows off, the scenarios for each blocker, hold and warning, keyboard-only pass, 320 px reflow); Rust integration (the recording port of [testing.md](testing.md) §4 driven through review and preflight, zero mutating calls; preflight on the read-only and full VHDX, US-16).

### US-35 Confirm with the counts
As a photographer, I want apply to ask for confirmation stating the counts, so that two keystrokes never move thousands of files.

- Given a review with no blocker, when I activate the primary button, then a confirm dialog opens and nothing has moved yet (ADR 0003, INV-03). **(proposed)** The button reads "Move 138 photos" when every planned photo moves (canvas), and "Apply to 138 photos" when some are copied.
- Given the dialog, then it states the counts and where they go: kept photos with moved or copied and the destination, rejected photos moved to `_rejected` ("nothing is deleted"), undecided photos staying where they are, held-back photos left out, the file count, the bytes and the time estimate (ADR 0003, SPEC §4.2). **(proposed wording)** Title "Move 138 photos?"; body "96 kept photos will be moved to Selects 2026. 42 rejected photos will be moved to E:\2026\_rejected. Nothing is deleted. 274 undecided photos stay where they are. 286 files, 3.0 GB, about 4 minutes."; buttons "Go back" and "Move 138 photos".
- Given the dialog opens, then focus is on "Go back" **(proposed)**, so a stray Enter closes it rather than confirming.
- Given the swipe screen, when I press Enter any number of times, or hold it, then the review opens and no apply starts: focus lands on the review's heading, never on the primary button (US-34), and a held Enter counts once (SPEC §10, M3 keys). Starting an apply from the keyboard takes reaching the primary button, activating it, then reaching the confirm button and activating it.
- Given the dialog, when I press Esc or choose "Go back", then it closes, nothing is recorded or moved, and focus returns to the primary button (NFR-A11Y-01, no keyboard trap).
- Given the dialog, then focus stays inside it while it is open, it has an accessible name and description, and its focus ring is visible (NFR-A11Y-04, NFR-A11Y-09).
- Given I confirm, then preflight runs again (FE §7.4), the plan is confirmed with the decisions it was built from (`decisions_upto_seq`), its groups are locked, and the apply screen opens (FE §5.4, §9.1). The webview sends only the plan id (FE §4, NFR-SEC-01).
- Given preflight at confirm finds a new blocker (card locked, drive unplugged since review), then nothing starts and the review comes back with the blocker shown (FE §8). **(proposed)** Given it finds a new hold or a taken name, then the dialog closes and the review shows the rebuilt plan, to be confirmed again.
- Given a confirmed plan, when I go back to the swipe screen during or after apply, then keep, reject, U and undo on a locked or applied group change nothing and say "Already applied" (FE §5.4, §12).
- Given the web demo, when I confirm, then the apply is simulated and the disk is never touched (SPEC §5.5).

Links: NFR-REL-01, NFR-REL-05, NFR-A11Y-01, NFR-A11Y-04, NFR-A11Y-09, NFR-SEC-01; INV-03, INV-17. Test: Vitest (confirm text from the plan for keep by move, keep by copy, rejects only, held groups, singular and plural); Playwright + axe (dialog contents and focus, Esc and "Go back" leave everything unchanged, Enter pressed 5 times and held from the swipe screen never opens the apply screen, focus trapped, keyboard-only); Rust integration (confirm reruns preflight and refuses a plan whose card was locked after review; confirm locks the groups).

### US-36 Apply with progress, pause and cancel
As a photographer, I want per-file progress with pause and cancel (after the current file), and a report at the end, so that a long cross-volume apply stays under my control.

Progress
- Given a confirmed plan, then the apply screen shows the route view as on review (SPEC §4.2), the photos done of the total, the bytes done of the total and the time left, for example "Moving your photos", "{done} of 138 photos done. You can keep using your computer.", "About 2 minutes left", "1.0 of 2.1 GB" (canvas Apply).
- Given an operation running, then the screen shows the current photo, its files and its step in words: copying, "Reading the copy back and checking it matches", "Then the original leaves Photos drive (E:) › 2026 › Barcelona" (canvas, FE §9.3). Progress is announced through a polite live region at most every few seconds **(proposed: every 10 photos or 5 s)** (NFR-A11Y-09).
- Given an apply, then the UI stays responsive: no main-thread task over 50 ms (NFR-PERF-13), and Windows is asked not to sleep (FE §9.4).
- Given a cross-volume move, then its source is removed only after the copy is read back and both hashes match the first, and only once every member of its group is committed (NFR-REL-04, INV-12, US-18).
- Given any apply, then undecided photos keep their path, bytes and mtime (NFR-REL-05), rejected photos are in `<source>\_rejected` with their subfolders, and no photo is removed except a verified cross-volume source (NFR-REL-03, INV-01).

Pause and cancel
- Given an apply, when I click "Pause", then the UI acknowledges it at once, saying it is pausing, and the pause takes effect after the current file: the engine checks between groups and before each operation, so the current file's whole operation (copy, verify, commit) finishes first; then the screen says it is paused and "Pause" becomes "Continue" (NFR-PERF-13, FE §9.5). **(proposed)** The label "Continue". FE §9.5 says "between steps", while the merged executor (`culla_core::journal::run`) stops between operations; which one holds is open (SPEC §10, file-engine §13).
- Given a paused apply, when I click "Continue", then it carries on from the journal with nothing repeated or lost (FE §9.5).
- Given an apply, when I click "Stop after this file", then the UI acknowledges it at once and the stop takes effect after the current file: its whole operation (copy, verify, commit) finishes, no new operation starts, and the report opens (NFR-PERF-13, SPEC §4.2, FE §9.5). Given a cross-volume group was part-way, then its committed members end "copied, source kept", with both files present, and the report says so (FE §9.5). Given a rename group (a reject, or a keep on the same volume) was part-way, then it can stay split between members, for example the JPEG in `_rejected` and the NEF still in the source, and the report lists each member's state (FE §9.5). **(proposed)** The button reads "Stop after this file", following SPEC; the canvas's "Stop after this photo" reads as if the whole group finished first, which the engine does not do. Margaux to confirm: keep the engine's rule and this label, so a stop can leave a cross-volume group "copied, source kept" or a rename group split, or finish the current group before stopping?
- Given the apply screen, then Pause and Stop are reachable by Tab with a visible focus ring, and Esc does not cancel **(proposed)** (NFR-A11Y-01, NFR-A11Y-04).

Failures during the apply
- Given a file held by Explorer, Defender or Lightroom, then it is retried with backoff (100 to 1600 ms), then marked failed and the apply goes on (FE §9.4, US-19, NFR-REL-09).
- Given a target that appeared after planning, then that group fails as a hard error, the existing file is unchanged, and the apply goes on (US-17, FE §12).
- Given the source card is pulled or the destination drive unplugged, then the apply pauses with "Insert the card <label>" and every handle is closed (FE §6.3, §9.4). Given the same card returns, under any drive letter, then it is bound by serial and the apply can continue (US-09, FE §12). **(proposed)** It continues when I click "Continue", not on its own.
- Given two mounted volumes with the card's serial, then the apply pauses and asks me to pick the volume from a list with labels and drive letters (FE §6.3).
- Given the destination fills up while a file is being copied or verified, then that file goes back to pending, its partial is removed, and the apply pauses saying the destination is full (FE §9.4). Given it fills up in any other step, then the apply pauses the same way with no journal entry, and recovery takes the operation on resume (FE §9.4, proposed there). Given the card's lock switch is flipped during the apply, then it pauses as write-protected (FE §9.4).
- Given a verify mismatch, then the file fails, its source is kept and its partial removed, and the report shows it first, pointing at the reader or the card (FE §9.4, NFR-REL-04).
- Given the app is closed during an apply, then the next launch resumes it (US-38). **(proposed)** Closing the window asks first: "An apply is running. Close and finish it next time Culla opens?".

Report
- Given an apply ends (finished, cancelled or stopped), then a report lists per group: done, split, copied with source kept, failed with its reason, held back, and partials kept because their source vanished (FE §9.6); alarms (`lost`, `fs_inconsistent`) come first with the advice to run `chkdsk` (FE §10.2); camera-protected groups say "copied, original kept" (FE §7.2).
- Given the report, then it offers "Retry failed" for failed groups that never reached committed, "Apply remaining" after a cancel or for unstarted groups, and "Finish moves" for groups with copied, source kept members, each run only when I choose it (FE §9.6). "Finish moves" re-verifies each copy from scratch before any source goes; a mismatch keeps both files and says why (SPEC §2).
- Given any run of a plan (apply, resume, redo, also after a pause, a cancel or a journal write failure), then a plain-text log is written under `%LOCALAPPDATA%\Culla\logs` and the report opens it (NFR-REL-14, FE §9.8). **(proposed)** "Open log" asks Rust to open the log by its run id in the default text editor; the webview never sees its path (threat model T7).
- Given the report, then it is reachable and readable by keyboard and screen reader, and the outcome of each state has a label as well as a colour (NFR-A11Y-01, NFR-A11Y-08).
- Given the web demo, then the apply is simulated with the same screens, pause, stop and report (SPEC §5.5).

Links: NFR-REL-02 to NFR-REL-07, NFR-REL-09, NFR-REL-11, NFR-REL-12, NFR-REL-14, NFR-PERF-10, NFR-PERF-11, NFR-PERF-13, NFR-A11Y-01, NFR-A11Y-04, NFR-A11Y-08, NFR-A11Y-09; INV-01, INV-04, INV-11, INV-12, INV-14. Test: Vitest (progress model from journal events: counts, bytes, time left, step wording, pause and stop states, report grouping and order); Playwright + axe on the demo (progress, pause acknowledged at once and taking effect after the current file, continue, stop after the current file, report contents and actions, scenarios for card pulled, disk full and verify mismatch, keyboard-only); Rust integration (journal executor over the fake ports: pause and cancel between operations, a cancelled cross-volume group ends copied with source kept, a cancelled rename group can end split; `windows-it`: locked file, target appearing between plan and execute, VHDX detach mid-copy and reattach under another letter, full VHDX, log written per run and checked against the journal); `culla-bench` B-12, B-13 (NFR-PERF-10, 11, 12); manual checklist (apply a real session, pull the card, Lightroom holding files, SD lock switch).

### US-37 Resume a session or start fresh
As a photographer, I want to resume a session or start fresh when I reopen a folder, including on another drive letter, so that I can cull a card over several sittings.

- Given a folder culled before, then the start screen lists it under recent folders with its status: in progress with the number decided, or applied (US-44, SPEC §4.2).
- Given a source with an active session and no open plan, when I open it, then I can resume it or start fresh (FE §6.1 step 4). **(proposed wording)** "Resume sorting" and "Start fresh", with the count decided so far.
- Given I resume, then every decision is restored, the undo and redo stacks are the same as before, and the list keeps its stored order with no re-sort (FE §5.3, US-06, NFR-REL-15). **(proposed)** The swipe screen opens on the first undecided photo in that order.
- Given I resume, then the destination is restored and labelled "same as last time" (US-21), and moved or copied starts as last chosen, as for a new session; both can be changed on review (decided by Margaux 28 Nov 2026, ADR 0029). **(proposed)** "RAW follows JPEG" is restored as the session had it, and can be changed on review; subfolders are always included.
- Given I start fresh, then the old session is archived with its events and journal kept, and every photo starts undecided (FE §6.1 step 4).
- Given the card comes back under another drive letter, then its session and cache are found by volume serial and relative path (US-09, NFR-REL-13).
- Given two cards with the same folder names, then each has its own session (US-09).
- Given files changed on the card since the last sitting, then reconcile on open flags missing files, adds new files as undecided, makes a decided group undecided when a file joined it and says why, lists a file joining an applied group as `joins_applied_group` and never touches it, and lists changed files (FE §6.2, US-11, ADR 0024).
- Given a reverted move found on open (`reverted_move`), then the review offers "Redo moves (n)" or "Finish moves (n)" and nothing runs until I confirm (FE §6.2, §9.6).
- Given the source has an open plan, then recovery and resume come first (US-38) and "Start fresh" is not offered until that plan is closed (FE §6.1 step 3).
- Given another session on the same card, for a folder above or below this one, has an open plan, then opening this folder is refused with a message to finish or cancel that apply first (FE §6.1 step 2). **(proposed wording)** "Another apply on this card is not finished. Finish or cancel it first."
- Given a stored decision log that is corrupt, then **(proposed, SPEC §10 decision log (d))** the session opens with the decisions before the bad event, nothing more is recorded until I choose to resume from there or start fresh, and the problem is shown.
- Given the remembered destination is on a drive that is not connected, then it is shown as not available and I must choose another or connect it (US-21).
- Given a source whose drive is not connected, then its recent entry says "Not connected" and cannot be started (merged UI, US-44).
- Given any of this, then nothing on the card is written before Apply: sessions live under `%LOCALAPPDATA%\Culla\sessions` (US-41, NFR-REL-01).

Open, not decided here: what happens to a session after a completed apply (FE §9.6), and how a card with the same serial but other contents is told apart on reopen (FE §6.2).

Links: NFR-REL-01, NFR-REL-10, NFR-REL-13, NFR-REL-15, NFR-PRIV-06, NFR-A11Y-01; INV-15, INV-17, INV-18, INV-23. Test: Vitest (start screen model with sessions in progress and applied; resume and start fresh choices; restored options); Playwright + axe on the demo (resume restores decisions, undo after resume, start fresh, refusal for a nested open plan, corrupt log scenario, keyboard-only); Rust integration (SQLite store: kill and reopen restores every event older than 500 ms, NFR-REL-10; reconcile with files removed, added and joining; `windows-it`: VHDX remounted under another letter finds the session); golden fixtures (fold after resume equals fold before, INV-15).

### US-38 An interrupted apply is finished at launch
As a photographer, I want an interrupted apply resumed with a clear message when the app opens, so that I know my photos are safe.

- Given an apply was cut off by a crash, a power cut, a killed process or a pulled card, when Culla starts, then it finds the open plan, takes the source lock first (FE §10.1, §2.2), sets a plan in `applying` to `interrupted` (a plan already `paused`, for example `paused (volume_lost)` after a pulled card, stays paused), and says so before anything else (FE §9.1, §10.1, architecture §6.5). **(proposed wording)** "An apply was interrupted. Each photo is either in its original place or checked at its new one. Culla will tell you about anything that needs attention." The line "Culla is finishing it." is added only if Margaux settles the open question below on rolling forward on its own; with a confirm, the notice says "Continue the apply" instead.
- Given the source or destination is not connected, then it waits with "Insert the card <label>" and touches nothing (FE §10.1, §6.3). Given the card returns under another letter, then it is identified by serial before anything is touched (US-20).
- Given both volumes are present, then the apply rolls forward from the journal, never back, on the apply screen with the same progress, pause and stop as US-36 (SPEC §5.2, FE §10.2). **(proposed)** It starts on its own once the volumes are present, with pause and stop available; SPEC §5.2 says it rolls forward the next time the app opens, and the choice is open in FE §10.1 and architecture §6.5. Margaux to confirm: roll forward on its own at launch once the volumes are present, or wait for a confirm on the launch notice?
- Given recovery, then every operation ends in the end state for its kind: a move leaves the file at exactly one place, a copy at both, always with its original hash; nothing is re-copied after `verifying` with a matching partial or after `committed` (US-20, [testing.md](testing.md) §6.1).
- Given Culla crashes again during recovery, any number of times, then the next launch converges to the same result (INV-14). Given recovery has finished, when it runs again, then it does nothing (US-20).
- Given another Culla process holds the source's lock, then this one leaves the plan alone (FE §10.1, INV-23).
- Given recovery finds a torn rename or a lost file (`fs_inconsistent`, `lost`), then it touches nothing for that file, and the report shows it first with the advice to run `chkdsk` (FE §10.2).
- Given recovery ends, then the report of US-36 is shown and a log is written for the resume run (FE §9.8).
- Given the most recently closed plan of a session had a same-volume move reverted after it closed (file back at its source), when Culla starts, then the launch check lists it as a reverted move and offers "Redo moves (n)"; nothing runs until I confirm, and the redo is a new small plan, preflighted and journaled like any other (US-20, FE §9.6).
- Given a cross-volume move whose source came back after the plan closed (file at both places), then it is listed and "Finish moves (n)" is offered; confirming re-verifies both files and removes the source; a mismatch keeps both and says why (US-20, SPEC §2).
- Given a launch check finds a torn state or a target gone, then it is reported as an alarm and nothing is offered (FE §9.6).
- Given the launch check, then it only reads: nothing is written on a user volume until I confirm a redo (INV-03).
- **(proposed)** Given the launch notice, then it is a screen of its own with a heading that takes focus and a live region for its progress, and the start screen comes after the report is closed.

Links: NFR-REL-06, NFR-REL-14, NFR-A11Y-01, NFR-A11Y-09; INV-11, INV-13, INV-14, INV-18, INV-23. Test: Vitest (launch notice model: waiting for a volume, recovering, report, findings to offer); Playwright + axe on the demo (interrupted-apply and launch-check scenarios: message, wait for the card, progress, report, "Redo moves" and "Finish moves" only after confirm, keyboard-only); Rust integration (crash loop over the journal executor, `cargo xtask crash-loop`; generated recovery-table tests; launch check over closed plans; `windows-it`: process killed mid-apply and VHDX detached mid-copy, then restart, testing.md §6); manual checklist (pull the card during a cross-volume apply, reinsert under another letter, restart, resume completes).

### US-39 Zoom to 100 %
As a photographer, I want to zoom to 100 % at the cursor (Z toggle or hold Space) and pan with the arrows, so that I can check focus before deciding. Z is the key labelled Z on AZERTY and QWERTZ too ([SPEC §4.1](SPEC.md#41-keyboard-map)).

- Given the swipe view has focus and the pointer is over the photo, when I press Z, then the photo shows at 100 % centred on the point under the pointer; with the pointer elsewhere, on the centre (SPEC §4.1). Pressing Z again, or Esc, returns to the fitted preview (SPEC §4.1).
- Given the swipe view has focus, when I hold Space, then the photo zooms to 100 % until I release it (SPEC §4.1). **(proposed)** With focus on a button, Space presses the button as usual and does not zoom.
- Given an AZERTY or QWERTZ layout, when I press the key labelled Z, then it zooms; Space matches by position (SPEC §4.1, US-22).
- Given zoom, when I press the arrows, then they pan and never decide (SPEC §4.1). **(proposed)** Each press pans a tenth of the view, a held arrow pans continuously, panning stops at the photo's edges, and dragging the photo pans instead of swiping.
- Given zoom, then the full image is loaded on demand through `culla://` by the photo's opaque id (SPEC §5.1, §5.3), and while it loads the preview stays visible with loading dots (SPEC §5.6).
- Given a 45 MP JPEG, then the full image is visible within 1.0 s from a UHS-I card and 400 ms from NVMe, and panning runs at 60 fps or more (NFR-PERF-07).
- Given one 100 % zoom on a 45 MP photo, then memory stays within NFR-PERF-09, and **(proposed)** the full image is released when zoom ends.
- Given a lone NEF or an HE/HE\* NEF, then zoom shows its largest embedded JPEG, not RAW pixels (SPEC §9, ADR 0011). **(proposed)** A note says "Embedded preview, not the RAW".
- Given zoom, when I click Keep or Reject **(proposed)**, then the decision is recorded, zoom ends and the next photo shows; Shift+arrows browse and also end zoom **(proposed)**.
- Given the full image cannot be read (card pulled, damaged file), then zoom stays on the preview with a message and the session is intact (NFR-REL-12).
- Given reduced motion, then zooming in and out is an instant swap (NFR-A11Y-05). **(proposed)**
- Given zoom, then the swipe and zoom screens use the Night theme (SPEC §5.6), the zoom state is announced ("Zoomed to 100 %", proposed wording), and the shortcut overlay lists Z, Space and the arrows' pan (US-31, NFR-A11Y-09).
- Given focus outside the swipe view (review, a dialog), then Z does nothing (NFR-A11Y-10). Given a held Z, then key repeat does not toggle it again **(proposed)**.

Links: NFR-PERF-07, NFR-PERF-09, NFR-REL-12, NFR-A11Y-01, NFR-A11Y-05, NFR-A11Y-09, NFR-A11Y-10. Test: Vitest (zoom geometry: point under the cursor kept, centre, pan steps and clamping, the key map for Z, Space and arrows while zoomed, Z repeat ignored); Playwright + axe on the demo (Z and Space zoom and unzoom, arrows pan without deciding, Esc leaves zoom, AZERTY Z, Space on a focused button presses it, reduced motion, keyboard-only); Rust integration (full-image route serves only ids of the current scan, US-33); `culla-bench` for NFR-PERF-07 **(proposed: a `zoom` run of 50 zooms per medium)**; manual checklist (focus check on real 24 and 45 MP files, [testing.md](testing.md) §8).

### US-40 EXIF overlay
As a photographer, I want an EXIF overlay (I) with date, lens and exposure, so that I can tell near-identical shots apart.

- Given the swipe view has focus, when I press I, then an overlay shows the current photo's capture date and time, lens and exposure; I again or Esc hides it (SPEC §4.1). **(proposed)** Exposure is shutter speed, aperture, ISO, focal length and exposure compensation, written "1/250 s · f/4 · ISO 400 · 50 mm · −0.3 EV", and the camera model is shown too.
- Given the overlay is open, when I move to another photo, then it shows that photo's values **(proposed: it stays open until I close it)**, and decisions, undo and browsing work as without it.
- Given a lens with no `LensModel`, then the lens comes from `LensSpecification` (SPEC §5.1). Given no capture time in DateTimeOriginal, then it falls back to `CreateDate`, then to the file time in the PC's local time zone (ADR 0028). **(proposed)** A time from the file says "from the file time"; a missing field says "Not recorded".
- Given the EXIF pass has not reached this photo yet, then the overlay shows loading dots and fills in when it has **(proposed)**.
- Given EXIF text from the card (lens name, camera model), then it is shown as text with control, bidi and invisible characters escaped, never as markup (threat model T6b).
- **(proposed)** Given the overlay, then GPS position and the body serial are not shown.
- Given the overlay, then its text has at least 4.5:1 contrast on Night (NFR-A11Y-06), it never covers the focused control or the Keep and Reject buttons (NFR-A11Y-04), it has an accessible name, and the shortcut overlay lists I (US-31).
- Given focus outside the swipe view, then I does nothing (NFR-A11Y-10).
- Given the web demo, then the values come from the EXIF pre-extracted to JSON (SPEC §5.5).

Links: NFR-A11Y-01, NFR-A11Y-04, NFR-A11Y-06, NFR-A11Y-09, NFR-A11Y-10, NFR-SEC-06. Test: Vitest (EXIF formatting: every field, missing fields, the lens and time fallbacks, escaping); Playwright + axe on the demo (I and Esc, values follow the photo, decisions with the overlay open, focus not obscured, contrast, keyboard-only); Rust integration (EXIF fields from the adapters' fixtures, fuzzed parsers, US-06).

## M6

### US-43 Uninstall keeps or removes my Culla data, never my photos
As a photographer removing Culla, I want to choose whether my sessions and cache go too, so that I can reinstall later without losing a half-culled card, or leave nothing behind.
- Given Culla is installed and `%LOCALAPPDATA%\Culla` holds sessions, cache and logs, when I uninstall without ticking "remove my Culla data", then the app is removed and `%LOCALAPPDATA%\Culla` is kept.
- Given the same, when I uninstall with the box ticked, then `%LOCALAPPDATA%\Culla` is removed.
- Given either choice, then no photo, `_rejected/` folder or destination folder is touched.

Links: NFR-REL, privacy in [nfr.md](nfr.md). Test: installer test on `windows-latest` ([testing.md](testing.md) section 5.3), manual checklist.
