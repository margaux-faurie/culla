> Historical input, written before the decisions were settled. Where it disagrees with `docs/SPEC.md` or an ADR, those win.

# Critique of spec-draft v0

The core idea is sound: decide everything first, review, then apply. The weak spots are undo, the pairing model and session identity. The swipe is not the differentiator. [QuickRawPicker](https://github.com/RawLabo/QuickRawPicker) (open source, cross-platform) already does keyboard culling and copies the picks along with their JPEGs and .xmp files. [Louppe](https://github.com/alexander-markin-meow/louppe/releases) (open source, Mac only) recovers interrupted moves and protects sidecars. [FastRawViewer](https://fastrawviewer.com/node/318) treats RAW+JPEG as one unit. What would make this one stand out: **a provably safe, pair- and sidecar-aware apply engine on Windows, with a review step before anything moves.** Write the spec around that.

## 1. UX gaps

- **Undo is broken.** After ← you have already moved on, so "undo current" hits the wrong photo. You need a global undo stack: Ctrl+Z undoes the last action and jumps back to that photo. Add Ctrl+Y, and drop `U`.
- **Dead keys.** ←/→ doing nothing on a decided photo feels like a bug. Let it overwrite the decision, and record that in undo history.
- **Missing navigation:** jump to the next undecided photo, a state filter (undecided/kept/rejected), Home/End. The end of the list is undefined. Suggest a "N undecided left" card with two choices: jump to the first undecided, or open the review.
- **Zoom vs arrows.** Do the arrows pan or decide while zoomed? Pick one. Settle hold vs toggle: suggest holding Z/Space for a temporary loupe at the cursor.
- **Key repeat.** Holding → would reject 30 photos. Ignore `event.repeat` on the decide keys. State changes instantly and the animation is purely cosmetic; it never blocks input.
- **Apply on a shortcut.** Two keystrokes would move thousands of files. Ask for a confirmation that shows the counts.
- **Spec gaps:** undecided photos are never touched (say so), a `?` help overlay, and **EXIF orientation** (portrait shots).

## 2. Data-safety risks

- **"RAW follows JPEG = off" is undefined.** Is the NEF a separate card, or silently left behind? Define it or drop the toggle.
- **Pair within one folder only.** Counter rollover (DSC_9999→0001) and second bodies produce the same basenames in several DCIM subfolders.
- **Nikon dual-slot** (NEF on slot 1, JPEG on slot 2): the halves of a pair sit in different roots. "RAW follows" then quietly does nothing. At minimum, detect it and warn. Better: accept several sources and pair on basename + DateTimeOriginal + SubSecTime.
- **Sidecars are not an open question.** Move `DSC_0001.xmp`, `.NEF.xmp`, `.JPG.xmp` and `.NEF.pp3`, plus NX Studio's `NKSC_PARAM/*.nksc`, which lives in a *subfolder*. List any other files with the same basename in the review screen.
- **Collisions are the norm.** After a counter reset, the destination already has a DSC_0001. Never overwrite. Suffix the whole group consistently (JPG, NEF and sidecars) and show it in the review.
- **Self-ingestion.** With subfolders included, a rescan picks up `_rejected` and any destination that sits inside the source. Exclude both. Specify where `_rejected` lives (I'd say the source root, mirrored).
- **Session key = folder path** breaks when the drive letter changes (E:→F:), which happens all the time with SD cards. Key on volume GUID + relative path. Identify photos by size + mtime, with EXIF date + shutter count as a fallback for renamed files. Answer "folder changed" in v1: reconcile on open and report decisions that point to missing files.
- **Mid-apply unplug.** The journal needs a state per file (planned/copied/verified/deleted), and resuming must check volume identity first. On cross-volume moves, verify with a hash (flaky readers can corrupt data without changing the size), then flush, then delete the source. Preflight checks: SD write-protect lock, free space for cross-volume *moves*, long paths, files locked by Lightroom.
- **Preserve timestamps** on copy. Write a plain-text log of operations into the destination.

## 3. Scope

**Over-engineered for v1:** the File System Access real-folder demo; release-please, CoC and SECURITY.md; Playwright; rollback in the middle of an apply (resume-forward is enough); "JSON *or* SQLite" (pick SQLite, which also holds the journal); mouse-drag swipe physics on a keyboard-first laptop.

**Missing:** everything marked Must below, plus colour management. The G14's OLED panel is wide-gamut and WebView2 handles embedded profiles only partially, so test AdobeRGB JPEGs. A quick toggle between the current and previous photo covers most burst-comparison needs at almost no cost.

## 4. Architecture weak points

- **Sorting by DateTimeOriginal** means reading every header before photo #1 appears, which is slow on SD. Start in filename order, stream the EXIF in, and reorder only photos not yet shown. Watch for unsynced clocks across bodies.
- **Decode:** a 45 MP JPEG is slow to decode in full. Use libjpeg-turbo DCT scaling, or the NEF's embedded preview. Serve cached images over a custom protocol; the asset-protocol scope is awkward with arbitrary Windows paths.
- **Thumbnail cache key** has the same drive-letter problem. Reuse the session identity.
- **Autosave:** write atomically, or use SQLite in WAL mode.
- **Define a `Backend` interface** shared by desktop and demo. Put property tests on the plan engine in `crates/core` (collisions, pairs, sidecars).

## 5. Prioritized changes

**Must**
1. A global undo stack that navigates back. Re-deciding overwrites.
2. Define "RAW follows = off", or remove it.
3. Pair within one folder. A full sidecar list, including `NKSC_PARAM`.
4. Never overwrite. Group-consistent suffixes.
5. Exclude `_rejected` and the destination from scans.
6. Identity that doesn't depend on the drive letter, plus reconciliation on open.
7. A journal with per-file state, hash verification on cross-volume moves, and preflight checks.
8. Ignore key repeat. Animation never blocks input.
9. EXIF orientation. Confirm before apply. Undecided photos are untouched.

**Should**
10. Jump to the next undecided photo, filters, end-of-list behaviour.
11. Define zoom input.
12. Streamed scan with lazy EXIF ordering.
13. Dual-slot detection / multiple sources.
14. Undo-apply (reverse plan) and a log file.
15. A `?` overlay. Colour-management tests. SQLite only.

**Could**
16. Toggle between the current and previous photo, then burst compare.
17. Zoom on the AF point (Nikon MakerNotes).
18. File System Access demo mode.
19. Full open-source boilerplate and code signing.
