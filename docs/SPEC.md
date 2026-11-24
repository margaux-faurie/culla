# Culla — Technical Specification (v1)

Status: draft v1, 24 Nov 2026. Supersedes `spec-draft` v0 after research and critique (see `docs/research/`).

## 1. What Culla is

A Windows desktop app that culls a folder of Nikon photos (JPEG + NEF) one photo at a time with a swipe interface. Every decision goes into a queue. Nothing touches the disk until the user reviews the queue and confirms. Open source (MIT), with a browser demo of the same UI.

**What sets it apart from QuickRawPicker, FastRawViewer and Photo Mechanic:** a swipe-first interface, plus a file engine that is provably safe. It treats a JPEG, its RAW and its sidecars as one group, never overwrites, journals every operation and resumes an apply that was interrupted.

## 2. Decisions (from the author)

| Topic | Decision |
|---|---|
| Platform | Windows 11 first; Windows 10 22H2 to decide (target machine: ROG Zephyrus G14 2024, 2880×1800 OLED) |
| Choices | Binary: keep / reject |
| Reject | Move to `<source>/_rejected/`, mirroring subfolders. The app never deletes a photo; the only removals are listed in §5.2 step 5. |
| Keep | Move or copy, chosen per session; destination folder chosen per session (last one remembered); subfolder structure mirrored |
| Apply | Queue → review screen → explicit confirm → apply |
| RAW pairing | Per-session toggle "RAW follows JPEG". When off, the NEF is left untouched. |
| Lone NEF | Shown via its embedded JPEG preview, sorted like any photo |
| Sidecars | Always follow the file they describe (`.xmp`, `.NEF.xmp`, `.pp3`, NX Studio `NKSC_PARAM/*.nksc`), see §3.1 |
| Subfolders | Included (recursive scan) |
| Volume | Thousands of photos, often on SD card or external drive |
| v1 extras | Zoom to 100%, EXIF overlay, resume a session later |
| Animations | First-class (swipe physics) |

## 3. Core model

### 3.1 Photo group
A **group** is the unit the user decides on. It holds:
- `primary`: the JPEG if one exists, otherwise the NEF.
- `raw`: the NEF, if any.
- `sidecars`: every sidecar file that matches.

Pairing is by case-insensitive basename **within the same folder only**. Nikon file counters roll over, so the same name can appear in two folders. Unknown extensions are ignored and listed in the review screen.

Sidecar matching (case-insensitive, same folder):
- `X.xmp`, `X.pp3` describe the group: they follow the group's primary.
- `X.NEF.xmp`, `X.JPG.xmp` and `NKSC_PARAM/X.NEF.nksc` / `NKSC_PARAM/X.JPG.nksc` describe one file: they follow that file. So when "RAW follows JPEG" is off, the NEF's sidecars stay with the NEF.
- A collision suffix is applied inside sidecar names too (`X_1.NEF.xmp`, `NKSC_PARAM/X_1.NEF.nksc`).

### 3.2 Decisions and history
- Group state: `undecided | kept | rejected`.
- Every change is appended to an event log as `{seq, ts, event}`, where `event` is one of the types below. The current state is derived from the log, which is what makes undo and resume work.
- Event types: `Decided{groupId, verdict}`, `Cleared{groupId}` (from U), `Undo{targetSeq}`, `Redo{targetSeq}`. The log only grows; state is a fold over it.
- **Ctrl+Z** appends `Undo` for the most recent decision event (`Decided` or `Cleared`) that is not already undone, restores that photo's previous state, and jumps to it. Repeated Ctrl+Z walks further back.
- **Ctrl+Shift+Z** appends `Redo` for the most recently undone event. Any new `Decided` or `Cleared` empties the redo stack.
- **U** appends `Cleared` for the photo shown (it becomes undecided). It is itself undoable with Ctrl+Z.
- ←/→ on a photo that already has a decision do nothing. A short shake animation and a toast saying "Press U to undo" explain why. This is the author's choice: undo first, then re-decide.

### 3.3 Identity and resume
- Sessions and caches are keyed by **volume serial + relative path**, never by drive letter. An SD card that comes back as `F:` instead of `E:` still resumes its session.
- When a folder is opened, the app reconciles it against the stored session. Missing files are flagged. New files are added as undecided. Renamed files cannot be detected in v1: they show as one missing file plus one new file.
- `_rejected/` and the destination folder (if it sits inside the source) are excluded from scans.

## 4. Interaction

### 4.1 Keyboard map
| Key | Swipe screen |
|---|---|
| → | Keep (if undecided), then advance |
| ← | Reject (if undecided), then advance |
| Shift+→ / Shift+← | Next / previous photo, no decision |
| N | Jump to next undecided |
| Ctrl+Z / Ctrl+Shift+Z | Global undo / redo (jumps to the photo) |
| U | Undo the current photo's decision |
| Z (toggle) or hold Space | Zoom 100% at the cursor or centre; arrow keys pan while zoomed |
| I | EXIF overlay |
| F | Filter: all / undecided / kept / rejected |
| ? | Shortcut overlay |
| Enter | Open the review screen |
| Esc | Leave zoom / overlay / review |

Rules:
- Single-key shortcuts (U, N, Z, I, F, ?) work only while the swipe view has focus (WCAG 2.1.4).
- Auto-repeat on held keys is ignored for decisions, so holding → never keeps 40 photos.
- Input is never blocked by an animation. Each decision is committed immediately; the animation catches up, and at most 3 cards are mounted.
- Mouse/trackpad: drag the card horizontally. A swipe commits on an offset or velocity threshold; otherwise the card springs back.
- At the end of the list, a summary card offers "Jump to undecided (n)" and "Review".

### 4.2 Screens
1. **Start:** source folder, include subfolders, RAW follows JPEG, keep = move/copy, destination. If a session already exists for the folder, offer to resume it (or start fresh).
2. **Swipe:** full-screen photo, progress bar (n / total, kept, rejected, undecided), decision badge, EXIF overlay, zoom, and visible **Keep** and **Reject** buttons (a single-click alternative to dragging, WCAG 2.5.7).
3. **Review:** counts; kept / rejected grids (click to jump back); the full operation plan; preflight results; estimated time and size; "Apply" needs a confirm dialog that states the counts. Undecided photos are never touched.
4. **Apply:** progress per file, can be paused or cancelled (cancel stops after the current file). Ends with a report and a log file (`%LOCALAPPDATA%\Culla\logs`, openable from the report).

## 5. Architecture

```
culla/
  crates/core/      package culla-core: domain, ports (traits), pure logic (grouping, decision log, plan,
                    journal state machine). No I/O, no Tauri. Tests run on any OS.
  crates/adapters/  package culla-adapters: Windows file ops, EXIF/NEF parsing, decode, SQLite store.
                    No Tauri. Integration tests on windows-latest.
  src-tauri/        Tauri 2 shell, wiring only: commands, culla:// image scheme, capabilities
  src/              React + TypeScript UI (one Vite app, two build modes: desktop | demo)
  docs/             SPEC.md, adr/, research/
```

### 5.1 Rust core and adapters (`crates/core`, `crates/adapters`)
The items below describe behaviour; the pure parts live in `culla-core` and the I/O in `culla-adapters`.
- **Scan:** streamed and recursive. It yields groups in filename order right away, so the first photo appears in about a second. A background pass reads only the first ~128 KB of each file to get EXIF (DateTimeOriginal, orientation), then sorts by capture time. Once the user has started swiping, the list is never re-sorted.
- **EXIF:** `kamadak-exif` (BSD-2) for JPEG and NEF. The lens comes from `LensModel`, falling back to `LensSpecification`.
- **NEF preview:** our own TIFF/IFD walker (~200 lines). It reads the SubIFDs and the MakerNote PreviewIFD and picks the largest embedded JPEG. This works for HE/HE\* NEFs too, since they embed ordinary JPEG previews. `rawler` is LGPL, so it is avoided (licensing would be murky for an MIT binary).
- **Decode/resize:** `turbojpeg` with scaled decode, scaling to just above screen size (1/2 or 3/8 for 45 MP), then `fast_image_resize`, then JPEG encoding. EXIF orientation is applied at this point. The 100% zoom view loads the full image on demand.
- **I/O:** 1–2 sequential reader threads, kept separate from the decode workers. Prefetch 5 ahead and 2 behind. The in-memory LRU holds about 12 screen previews. Thumbnails are cached on the internal disk (`%LOCALAPPDATA%\Culla\cache`), keyed by volume serial + path + size + mtime, capped at 2 GB with least-recently-used eviction. Files on removable media are **never memory-mapped**.
- **Sessions & journal:** SQLite (`rusqlite`), one database per source (volume serial + relative path), stored in `%LOCALAPPDATA%\Culla\sessions`, never on the card. Uninstalling leaves this data unless the user ticks "remove my Culla data".

### 5.2 Apply engine
1. **Plan:** each group expands to a list of operations (primary + raw if "RAW follows" + sidecars). Every member of a group gets the same target name. If a target name is taken, the whole group gets `_1`, `_2`… Nothing is ever overwritten.
2. **Preflight:** before any operation, check:
   - the source is writable (SD card lock, read-only flags);
   - free space at the destination, for moves across drives as well as copies;
   - the destination path is not inside `_rejected`;
   - paths fit within the length limit (use the `\\?\` prefix);
   - no files are locked;
   - there are no symlinks or junctions (refuse to follow them).
3. **Execute,** journalled with one row per file (`pending → copying → verified → done | failed`):
   - Every rename uses `MoveFileExW` with no flags except `MOVEFILE_WRITE_THROUGH`. Without `MOVEFILE_REPLACE_EXISTING`, an existing target makes it fail atomically (no check-then-rename race). Without `MOVEFILE_COPY_ALLOWED`, a rename across volumes fails with `ERROR_NOT_SAME_DEVICE` instead of silently copying and deleting. Copies open their target with `create_new`. "Target exists" is a hard error for that group.
   - Move on the same volume: a single such rename.
   - Move across volumes: copy to `*.culla-partial` while hashing the source (xxh3/blake3), fsync, then verify: read the destination back with `FILE_FLAG_NO_BUFFERING` (so the OS cache can't answer for the disk) and **re-read the source for a second hash**; all three hashes must match (this catches a flaky card reader, which would corrupt the copy and the first hash alike). Then rename into place and remove the source.
   - Copy: same as across volumes, but the source is kept. The mtime is preserved.
   - Sharing violations (Explorer, Defender, Lightroom): retry with backoff, then mark the file failed and continue.
4. **Resume:** if an apply is interrupted (crash, card pulled), it rolls forward from the journal the next time the app opens. There is no rollback in v1.
5. **Removal:** one audited function, `remove_verified`, is the only code allowed to remove a file. It removes only (a) the source of a cross-volume move whose verification passed, and (b) Culla's own `*.culla-partial` files (for example when resuming from `copying`). `std::fs::remove_*` is banned elsewhere by clippy `disallowed-methods`.

### 5.3 Image delivery to the webview
A Tauri async **custom URI scheme** (`culla://preview/<id>`, which appears as `http://culla.localhost` on Windows) serves the pre-encoded previews from memory. Images never go through IPC strings, and the asset protocol is not used for originals. The CSP `img-src` allows only that origin. The UI calls `img.decode()` before animating a card in.

### 5.4 Frontend
- React 19 + TypeScript (strict) + Vite. Animations use Motion (`motion/react`): `drag="x"`, `useMotionValue`/`useTransform`, and a commit threshold on offset and velocity in `onDragEnd`. Keyboard decisions trigger the same exit animation as a drag.
- A `Backend` interface has two implementations, `TauriBackend` and `DemoBackend`, chosen at build time so the demo bundle never includes `@tauri-apps/*`.
- State is a reducer over the decision event log, mirrored to the backend on each event (autosave).

### 5.5 Web demo
- About 25 author photos at 1600 px long edge in AVIF/WebP (~5 MB in total), with EXIF pre-extracted to JSON. Apply is simulated, and the review screen shows the plan it would have run.
- An optional "your folder" mode uses `<input webkitdirectory>`. It is read-only, works in every browser, and nothing is uploaded.
- Hosted on Cloudflare Workers static assets (subdomain of margauxfaurie.com, to confirm).

### 5.6 Visual design
Culla follows the **Portfolio Design System** (https://claude.ai/artifact/WvPpsmsFesTQ7nHuMbJLms), so the app, the demo and margauxfaurie.com look like one family. In practice:
- **Typeface:** Bricolage Grotesque, self-hosted (the app has to work offline, so it isn't loaded from Google Fonts).
- **Colours:**
  - `paper`, `ink`, `ink-muted` and `rule` for the chrome.
  - `orange` for the one thing that matters on screen: the keep feedback, the primary Apply button, the progress line.
  - No shadows or gradients.
  - `radius-pill` buttons and pills, `radius-panel` panels.
  - Loading states are drawn as dots, not spinners.
- **Exception, the photo surround.** Judging a photo against cream shifts how its colour and exposure read. So the swipe and zoom screens use the system's **Night** theme (`paper` #0c0b0a, `ink` #f1e9d8), while the start, review and apply screens stay cream. This needs an addition to the design system ("photo viewing surfaces use Night").
- **Keep and reject feedback (decided 24 Nov).**
  - Keep gets a second colour, a soft sage green (`keep`, #8fd3a8; 11.3:1 on Night, but only 1.44:1 on cream, so it is a fill on dark screens only). On cream screens the "Kept" label uses a darker `keep-ink` (to define, at least 4.5:1). Orange stays the brand accent: the Apply button (a fill with ink text) and, on Night, the progress line; on cream the progress line uses `orange-ink`. Reject outlines and focus rings use `ink-muted` or `ink`, never `rule` (1.3:1).
  - Reject has no colour: a dimmed card, an outline badge and a "Rejected" label.
  - Colour is never the only signal. Every state also has a label, a direction and a shape (filled for keep, outline for reject), so the app stays usable with colour blindness.
  - Both additions (`keep`, and photo screens on Night) go back into the Portfolio Design System.
- **Motion and copy:**
  - Animations are short and respect `prefers-reduced-motion`. The swipe exit is the screen's one playful moment.
  - Copy follows the system's voice: sentence case, plain, no exclamation marks.
- Tokens are copied into `src/styles/tokens.css` from the design system's `tokens.json`, with a note recording the version they came from.

## 6. Distribution
- `tauri-action` builds an NSIS installer (per-user, no admin rights) as a draft GitHub Release.
- `tauri-plugin-updater` with its own signing key, which works on unsigned builds. The update check can be turned off in settings; its default and frequency are to decide. No telemetry of any kind.
- Windows code signing is postponed. Azure Artifact Signing is not open to individuals in the EU. Once v1 is out, apply to SignPath Foundation (free for OSS). The README explains the SmartScreen warning.
- winget manifest after v1.

## 7. Engineering standards
- pnpm + Cargo workspaces; Biome (lint + format for TS); rustfmt + clippy `-D warnings`; cargo-deny.
- Tests: `cargo test` on core with tempfile fixtures (the file engine gets the most tests, including interrupted-apply scenarios); Vitest on the reducer and the UI logic; Playwright on the web demo.
- CI on PRs: lint, test, build. Release via release-please with Conventional Commits. Dependabot (grouped). Action SHAs pinned. OpenSSF Scorecard.
- MIT license, README with GIF, CONTRIBUTING, SECURITY.md, third-party licences file.
- Decisions recorded as ADRs in `docs/adr/` (these also feed the blog enablement guide).
- Agent practices, security and architecture conventions: see `docs/research/practices.md`. Key rules: removal lives only in `remove_verified` (§5.2 step 5), and `std::fs::remove_*` is banned elsewhere via clippy `disallowed-methods`; `std::fs::rename` overwrites on Windows, so the adapters use the no-replace `MoveFileExW` and copies open targets with `create_new`; Tauri commands restricted with `AppManifest::commands`; the webview gets no fs plugin permissions; cargo-deny (no cargo-audit), CodeQL (JS/TS + Rust), workflow lockfiles, pnpm supply-chain settings; tauri-specta pinned (still RC), `ts-rs` as fallback; cargo-fuzz targets on the EXIF/NEF parsers.

## 8. Scope

**v1:** everything above, Windows only.

**v2 candidates:**
- Undo an apply (reverse plan), with the log file already in v1.
- Detect dual-slot setups (RAW and JPEG on two cards) and support multiple sources.
- A/B toggle against the previous photo, then a burst comparison view.
- Zoom on the AF point (Nikon MakerNotes).
- Builds for macOS and Linux.
- Multiple destinations on number keys.
- Star ratings / XMP output.
- Code signing, winget and the Microsoft Store.

## 9. Known limits and risks
- On HE/HE\* NEFs, 100% zoom shows the embedded JPEG rather than the RAW pixels; no open-source decoder exists.
- Files moved outside Lightroom show as missing in its catalog (the README warns about this).
- Colour on the G14's wide-gamut OLED: test that sRGB/Display P3 previews look right in WebView2.
- Timings in the research notes are estimates and must be benchmarked on a real SD card (milestone 1).
- Moving 2,000 pairs across volumes is about 140 GB of files, but about 280 GB read (the source is read twice for verification), so 50+ minutes from a UHS-I card. The review screen shows an estimate that includes the verification reads.

## 10. Open questions
- Windows 10 22H2 support (out of mainstream support since Oct 2025): must, should, or dropped.
- Update check: on or off by default, and how often.
- Minimum WebView2 version and installer mode (the bootstrapper downloads WebView2 at install time if missing).
- Demo subdomain and which photos to publish.
- Nikon body(ies) used, which determines the NEF variants to test with.
