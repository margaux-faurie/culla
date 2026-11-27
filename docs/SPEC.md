# Culla — Specification and document map

Status: v1, 24 Nov 2026, updated 27 Nov 2026. This file is the record of the author's product decisions and the map of the design docs. Each topic below has an owning document: where a section here and its owner disagree, the owner wins, and this file is updated to match.

| Topic | Owner |
|---|---|
| Problem, users, scope, no-gos, success criteria | [PRD.md](PRD.md) |
| Measurable requirements (performance, reliability, accessibility, privacy, compatibility) | [nfr.md](nfr.md) |
| Architecture, crates, runtime views, glossary | [architecture.md](architecture.md) |
| Decisions and their reasons | [adr/](adr/README.md) |
| File engine: data model, grouping, plan, apply, recovery, invariants | [design/file-engine.md](design/file-engine.md) |
| Threats and mitigations | [threat-model.md](threat-model.md) |
| Incident response (keys, bad releases, vulnerabilities) | [incident-response.md](incident-response.md) |
| Web demo deployment (Worker, Workers Builds settings, headers) | [deploy-demo.md](deploy-demo.md) |
| Tests and Definition of Done | [testing.md](testing.md) |
| Performance measurement | [benchmarks.md](benchmarks.md) |
| Milestones | [roadmap.md](roadmap.md) |
| User stories | [stories.md](stories.md) |
| Agent instructions | [../CLAUDE.md](../CLAUDE.md) |
| Background research (historical) | [research/](research/) |

On file-engine topics (journal states, how renames are issued, verification), `design/file-engine.md` wins over ADRs 0004 and 0010; ADRs 0021 and 0022 (accepted 27 Nov 2026) record how it refines them. Open decisions for the author are collected in §10 here and in §13 of the file-engine design.

## 1. What Culla is

A Windows desktop app that culls a folder of Nikon photos (JPEG + NEF) one photo at a time with a swipe interface. Every decision goes into a queue. Nothing touches the disk until the user reviews the queue and confirms. Open source (GPL-3.0-only, ADR 0026), with a browser demo of the same UI.

**What sets it apart from QuickRawPicker, FastRawViewer and Photo Mechanic:** a swipe-first interface, plus a file engine that is provably safe. It treats a JPEG, its RAW and its sidecars as one group, never overwrites, journals every operation and resumes an apply that was interrupted.

## 2. Decisions (from the author)

| Topic | Decision |
|---|---|
| App identifier | `com.margauxfaurie.culla` (reverse-DNS; Tauri bundle identifier, used by the installer and the WebView2 data folder; Culla's own data stays in `%LOCALAPPDATA%\Culla`, architecture §8.7; decided 27 Nov 2026) |
| Platform | Windows 11 only; Windows 10 is dropped (decided 27 Nov 2026). Target machine: ROG Zephyrus G14 2024, 2880×1800 OLED |
| Camera | Nikon Z series (Z6, Z7, Z8, Z9 family), so lossless, lossy, uncompressed and HE/HE\* NEFs are all possible; test resolutions 24 MP (Z6 class) and 45 MP (Z7, Z8, Z9 class) (decided 27 Nov 2026) |
| Choices | Binary: keep / reject |
| Reject | Move to `<source>/_rejected/`, mirroring subfolders. The app never deletes a photo; the only removals are listed in §5.2 step 5. |
| Keep | Move or copy, chosen fresh each session with nothing preselected (Start stays disabled until one is chosen); destination remembered per source folder and labelled "same as last time": resuming a folder restores that folder's destination, a new folder proposes the last destination used anywhere, a first run has none (the user chooses), and a remembered destination that is missing (drive unplugged) is shown as not available and the user must choose; subfolder structure mirrored (decided 27 Nov 2026) |
| Apply | Queue → review screen → explicit confirm → apply |
| RAW pairing | Per-session toggle "RAW follows JPEG". When off, the NEF is left untouched. |
| Lone NEF | Shown via its embedded JPEG preview, sorted like any photo |
| Sidecars | Always follow the file they describe (`.xmp`, `.NEF.xmp`, `.pp3`, NX Studio `NKSC_PARAM/*.nksc`), see §3.1. A bare `X.xmp` or `X.pp3` describes the RAW (Lightroom's and RawTherapee's convention): when "RAW follows JPEG" is off, it stays with the NEF (decided 27 Nov 2026, ADR 0023) |
| Camera-protected files | A source file with the read-only attribute (camera "protect") is copied, never moved: its group is planned as a copy (for a reject, a copy into `_rejected`), the original stays in place, and the review and report say so (decided 27 Nov 2026) |
| Cloud-synced destination | A destination inside a OneDrive or other cloud-synced folder is allowed, with a warning in the review screen (decided 27 Nov 2026) |
| Reconcile | A new file that joins an already decided group makes that group undecided again (decided 27 Nov 2026, ADR 0024) |
| Interrupted moves | A group left "copied, source kept" can be finished from the report with "Finish moves", which re-verifies each copy from scratch and removes its source through `remove_verified` (decided 27 Nov 2026, design/file-engine.md §9.6). The same is offered when a move's source removal was lost after the plan closed (file at both places) |
| Preview size | Each preview fits a 2880×1800 box in the photo's own orientation, never upscaled: a 3:2 photo is 2700×1800 (1800×2700 portrait), filling the G14's screen height (decided 27 Nov 2026, US-07) |
| Colour profiles in previews | The author shoots sRGB, so the re-encoded preview carries no ICC profile in v1; AdobeRGB or Display P3 JPEGs show with the wrong colours, a known limit (§9) (decided 27 Nov 2026) |
| Sample photos | The author's real samples are licensed CC BY 4.0 (tests and web demo) and stored in Git LFS (`testdata/real/`), GPS and body serial stripped (decided 27 Nov 2026) |
| File engine | ADR 0021 (unbuffered source re-read for hash 3) and ADR 0022 (rename through the checked handle) accepted. A same-volume rename stays `committed` until the plan ends, so recovery can redo a rename the file system reverted; one reverted after the plan closed is shown at the next launch and redone after the user confirms (decided 27 Nov 2026, §5.2) |
| Update check | On by default, once a day; can be turned off in settings (decided 27 Nov 2026) |
| Success criteria | The PRD §7 targets are confirmed: 1,000 pairs from SD with zero lost files, recovery after a card pull, 30 minutes or less of swiping for 1,000 pairs (decided 27 Nov 2026) |
| Licence and name | GPL-3.0-only (not "or later"), copyright Margaux Faurie; the "Culla" name and logo are reserved and forks must rename; issues are welcome but outside pull requests are not merged, so the code keeps one author (decided 27 Nov 2026, ADR 0026) |
| Capture time | DateTimeOriginal, then CreateDate, then the file time in the PC's local time zone; camera-local order with filename tie-breaks (decided 27 Nov 2026, ADR 0028) |
| Review and merge | Agent PRs merge on an independent reviewer's approval plus green CI; PRs that touch src-tauri, winfs, CI or dependencies also wait for Margaux's OK in chat (decided 27 Nov 2026) |
| Where photos go | Shown as one route, the same on the start, swipe, review and apply screens: the origin, then a Kept branch (move or copy, destination, one real example path with its JPEG, NEF and edit files), a Rejected branch (`<source>\_rejected\…` written literally, marked as a new folder only while `_rejected\` doesn't exist yet, "nothing is deleted") and "undecided photos stay where they are". The example path follows the "RAW follows JPEG" toggle (when off, the NEF and its sidecars stay at the origin, ADR 0023) and shows copy for camera-protected files. The swipe screen shows where the current photo would go under each button; on an already decided photo it shows where the photo will go without implying the arrows act (ADR 0006). Full folder trees and the per-file list are one click away on review. Button placement is still open (NFR-A11Y-02) (decided 27 Nov 2026; design canvas linked in §5.6) |
| Keyboard layouts | By the letter: letters and Ctrl shortcuts match the character on the key (`KeyboardEvent.key`, case-insensitive), so Z and Ctrl+Z stay on the Z key on AZERTY and QWERTZ; arrows, Space, Enter and Esc match by position (`KeyboardEvent.code`); `?` by character. Hints show the key as labelled (decided 27 Nov 2026, §4.1) |
| Web demo hosting | `culla.margauxfaurie.com`, Cloudflare Workers static assets; same Cloudflare Access policy as the rest of the site until the portfolio goes public; the repository itself stays public (decided 27 Nov 2026) |
| Web demo deployment | Worker `culla`, built and deployed by Workers Builds from `main` on every push (config in `wrangler.jsonc`, headers in `public-demo/_headers`); Cloudflare Access on; no preview URLs, no non-production branch builds, workers.dev off. Settings in [deploy-demo.md](deploy-demo.md) (decided by Margaux 27 Nov 2026). Since a merge to `main` deploys, `wrangler.jsonc` and `public-demo/_headers` count as CI under the review and merge rule: they wait for Margaux's OK in chat |
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
- A bare `X.xmp` or `X.pp3` describes the RAW, as Lightroom and RawTherapee write them: it follows the NEF when the group has one, otherwise the primary (a JPEG-only group). With "RAW follows JPEG" on, it moves with the whole group; with it off, it stays with the NEF (decided by Margaux, 27 Nov 2026, ADR 0023).
- `X.NEF.xmp`, `X.JPG.xmp` and `NKSC_PARAM/X.NEF.nksc` / `NKSC_PARAM/X.JPG.nksc` describe one file: they follow that file. So when "RAW follows JPEG" is off, the NEF's sidecars (bare `X.xmp` and `X.pp3` included) stay with the NEF.
- A collision suffix is applied inside sidecar names too (`X_1.NEF.xmp`, `NKSC_PARAM/X_1.NEF.nksc`).

### 3.2 Decisions and history
- Group state: `undecided | kept | rejected`.
- Every change is appended to an event log as `{seq, ts, event}`, where `event` is one of the types below. The current state is derived from the log, which is what makes undo and resume work.
- Event types: `Decided{groupId, verdict}`, `Cleared{groupId}` (from U), `Undo{targetSeq}`, `Redo{targetSeq}`. The log only grows; state is a fold over it.
- **Ctrl+Z** appends `Undo` for the most recent decision event (`Decided` or `Cleared`) that is not already undone, restores that photo's previous state, and jumps to it. Repeated Ctrl+Z walks further back.
- **Ctrl+Shift+Z** appends `Redo` for the most recently undone event. Any new `Decided` or `Cleared` empties the redo stack.
- **U** appends `Cleared` for the photo shown (it becomes undecided). It is itself undoable with Ctrl+Z.
- ←/→ on a photo that already has a decision do nothing. A short shake animation and a toast saying "Press U to undo" explain why. This is the author's choice: undo first, then re-decide. The same rule applies to the Keep and Reject buttons and to drag (decided by Margaux, 27 Nov 2026; ADR 0006).

### 3.3 Identity and resume
- Sessions and caches are keyed by **volume serial + relative path**, never by drive letter. An SD card that comes back as `F:` instead of `E:` still resumes its session.
- When a folder is opened, the app reconciles it against the stored session. Missing files are flagged. New files are added as undecided; a new file that joins an already decided group makes that group undecided again (decided by Margaux, 27 Nov 2026; ADR 0024, design/file-engine.md §6.2). Renamed files cannot be detected in v1: they show as one missing file plus one new file.
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

Letters and Ctrl shortcuts (U, N, Z, I, F, Ctrl+Z, Ctrl+Shift+Z) are matched by the character on the key (`KeyboardEvent.key`, case-insensitive), so Z and Ctrl+Z stay on the Z key on AZERTY and QWERTZ. Arrows, Space, Enter and Esc are matched by position (`KeyboardEvent.code`), and `?` by character. The overlay and hints show each key as labelled on the keyboard (decided by Margaux, 27 Nov 2026).

Rules:
- Single-key shortcuts (U, N, Z, I, F, ?) work only while the swipe view has focus (WCAG 2.1.4).
- Auto-repeat on held keys is ignored for decisions, so holding → never keeps 40 photos.
- Input is never blocked by an animation. Each decision is committed immediately; the animation catches up, and at most 3 cards are mounted.
- Mouse/trackpad: drag the card horizontally. A swipe commits on an offset or velocity threshold; otherwise the card springs back.
- At the end of the list, a summary card offers "Jump to undecided (n)" and "Review".

### 4.2 Screens
1. **Start:** choosing the folder is part of the experience (Margaux, 27 Nov 2026). v1: recent folders with their session status (in progress, applied) and the native folder dialog opened from Rust; detected drives, a found-photos preview and drag and drop are v2 ([stories.md](stories.md) US-44). The webview only gets opaque source IDs and display data, never a path. Then: source folder, include subfolders, RAW follows JPEG, keep = move/copy, destination. If a session already exists for the folder, offer to resume it (or start fresh). The route view (§2, "Where photos go") shows origin, kept, rejected and left in place as the options are set.
2. **Swipe:** full-screen photo, progress bar (n / total, kept, rejected, undecided), decision badge, EXIF overlay, zoom, and visible **Keep** and **Reject** buttons (a single-click alternative to dragging, WCAG 2.5.7). Under each button, the route view shows where the current photo would go.
3. **Review:** counts; kept / rejected grids (click to jump back); the full operation plan; preflight results; estimated time and size; "Apply" needs a confirm dialog that states the counts. Undecided photos are never touched. The route view heads the screen, with folder trees and the per-file list one click away.
4. **Apply:** progress per file, can be paused or cancelled (cancel stops after the current file). Ends with a report and a log file (`%LOCALAPPDATA%\Culla\logs`, openable from the report). The route view stays visible, the same as on review.

## 5. Architecture

```
culla/
  crates/core/      package culla-core: domain, ports (traits), pure logic (grouping, decision log, plan,
                    journal state machine). No I/O, no Tauri. Tests run on any OS.
  crates/adapters/  package culla-adapters: Windows file ops, EXIF/NEF parsing, decode, SQLite store.
                    No Tauri. Integration tests on windows-latest.
  crates/cli/       package culla-cli, binary culla: headless scan|preview|plan|apply|resume (ADR 0020)
  crates/bench/     package culla-bench: benchmark harness (ADR 0020)
  src-tauri/        Tauri 2 shell, wiring only: commands, culla:// image scheme, capabilities
  src/              React + TypeScript UI (one Vite app, two build modes: desktop | demo)
  docs/             SPEC.md, adr/, research/
```

### 5.1 Rust core and adapters (`crates/core`, `crates/adapters`)
The items below describe behaviour; the pure parts live in `culla-core` and the I/O in `culla-adapters`.
- **Scan:** streamed and recursive. It yields groups in filename order right away, so the first photo appears in about a second. A background pass reads only the first ~128 KB of each file to get EXIF (DateTimeOriginal, orientation), then sorts by capture time. Once the user has started swiping, the list is never re-sorted.
- **EXIF:** `kamadak-exif` (BSD-2) for JPEG and NEF. The lens comes from `LensModel`, falling back to `LensSpecification`. Capture time falls back from DateTimeOriginal to CreateDate to the file time, read in the PC's local time zone; photos are ordered camera-local with filename tie-breaks (decided 27 Nov 2026, ADR 0028).
- **NEF preview:** our own TIFF/IFD walker (~200 lines). It reads the SubIFDs and the MakerNote PreviewIFD and picks the largest embedded JPEG. This works for HE/HE\* NEFs too, since they embed ordinary JPEG previews. `rawler` is avoided (LGPL and `memmap2`, ADR 0011).
- **Decode/resize:** `turbojpeg` with scaled decode, at the smallest scale that covers the preview box (1/2 for 24 MP, 3/8 for 45 MP), then `fast_image_resize` to fit 2880×1800 (2700×1800 for 3:2), then JPEG encoding without an ICC profile. EXIF orientation is applied at this point. The 100% zoom view loads the full image on demand.
- **I/O:** 1–2 sequential reader threads, kept separate from the decode workers. Prefetch 5 ahead and 2 behind. The in-memory LRU holds about 12 screen previews. Thumbnails are cached on the internal disk (`%LOCALAPPDATA%\Culla\cache`), keyed by volume serial + path + size + mtime, capped at 2 GB with least-recently-used eviction. Files on removable media are **never memory-mapped**.
- **Sessions & journal:** SQLite (`rusqlite`), one database per source (volume serial + relative path), stored in `%LOCALAPPDATA%\Culla\sessions`, never on the card. Uninstalling leaves this data unless the user ticks "remove my Culla data".

### 5.2 Apply engine
1. **Plan:** each group expands to a list of operations (primary + raw if "RAW follows" + sidecars). Every member of a group gets the same target name. If a target name is taken, the whole group gets `_1`, `_2`… Nothing is ever overwritten.
2. **Preflight:** before any operation, check:
   - the source is writable, from the volume's read-only flag (SD card lock); preflight itself is read-only and writes no probe file;
   - source files with the read-only attribute (camera "protect") are planned as copies, so the original stays in place; the review lists them;
   - a destination inside a cloud-synced folder (OneDrive and the like) gets a warning in the review screen, not a block;
   - free space at the destination, for moves across drives as well as copies;
   - the destination path is not inside `_rejected`;
   - paths fit within the length limit (use the `\\?\` prefix);
   - no files are locked;
   - there are no symlinks or junctions (refuse to follow them).
3. **Execute,** journalled with one row per file. The states and recovery rules are owned by [design/file-engine.md](design/file-engine.md) (`pending, renaming, copying, verifying, committed, source_removed, done, copied_source_kept, failed`):
   - Every rename is no-replace: never `MOVEFILE_REPLACE_EXISTING` / `FILE_RENAME_FLAG_REPLACE_IF_EXISTS`, never `MOVEFILE_COPY_ALLOWED`. The mechanism (rename through the checked handle) is ADR 0022. An existing target makes it fail atomically (no check-then-rename race), and a rename across volumes fails with `ERROR_NOT_SAME_DEVICE` instead of silently copying and deleting. Copies open their target with `create_new`. "Target exists" is a hard error for that group.
   - Move on the same volume: a single such rename.
   - Move across volumes: copy to `*.culla-partial` while hashing the source (xxh3/blake3), fsync, then verify: read the destination back with `FILE_FLAG_NO_BUFFERING` (so the OS cache can't answer for the disk) and **re-read the source with `FILE_FLAG_NO_BUFFERING` for a second hash (ADR 0021)**; all three hashes must match (this catches a flaky card reader, which would corrupt the copy and the first hash alike). Then rename into place and remove the source.
   - Copy: same as across volumes, but the source is kept. Modification and creation times are preserved.
   - A group's sources are removed only after every member of the group is committed. If any member fails, the group fails before any of its sources is removed; members already copied are reported as "copied, source kept". Once the cause is fixed, "Finish moves" in the report completes such a group: it re-verifies each copy from scratch and removes its source through `remove_verified` (decided by Margaux, 27 Nov 2026).
   - A same-volume rename of a user file cannot be flushed, so it stays `committed` until the end of the plan and is only then marked `done`; recovery redoes a rename the file system reverted after a crash. A rename reverted after the plan closed is detected at the next launch, shown, and redone in a small new plan once the user confirms (decided by Margaux, 27 Nov 2026; design/file-engine.md §9.3, §9.6).
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
- Hosted on Cloudflare Workers static assets at `culla.margauxfaurie.com` (Worker `culla`, Workers Builds), behind the site's Cloudflare Access policy until the portfolio is public (§2). Settings and headers: [deploy-demo.md](deploy-demo.md).

### 5.6 Visual design
Screen designs (start, swipe, already decided, review, apply) live on the design canvas https://claude.ai/artifact/CfDZRFBUCphPgXVKmTxSWh, reworked on 27 Nov 2026 after research on import tools, culling apps and plan visualisation. Culla follows the **Portfolio Design System** (https://claude.ai/artifact/WvPpsmsFesTQ7nHuMbJLms), so the app, the demo and margauxfaurie.com look like one family. In practice:
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
- `tauri-plugin-updater` with its own signing key, which works on unsigned builds. The update check is on by default, runs once a day and can be turned off in settings (decided by Margaux, 27 Nov 2026). No telemetry of any kind.
- Windows code signing is postponed. Azure Artifact Signing is not open to individuals in the EU. Once v1 is out, apply to SignPath Foundation (free for OSS). The README explains the SmartScreen warning.
- winget manifest after v1.

## 7. Engineering standards
- pnpm + Cargo workspaces; Biome (lint + format for TS); rustfmt + clippy `-D warnings`; cargo-deny.
- Tests: `cargo test` on core with tempfile fixtures (the file engine gets the most tests, including interrupted-apply scenarios); Vitest on the reducer and the UI logic; Playwright on the web demo.
- CI on PRs: lint, test, build. Release via release-please with Conventional Commits. Dependabot (grouped). Action SHAs pinned. OpenSSF Scorecard.
- GPL-3.0-only licence (ADR 0026), TRADEMARKS for the name and logo, README with GIF, CONTRIBUTING (issues only, no outside PRs), SECURITY.md, third-party licences file.
- Decisions recorded as ADRs in `docs/adr/` (these also feed the blog enablement guide).
- Agent practices, security and architecture conventions: see `docs/research/practices.md`. Key rules: removal lives only in `remove_verified` (§5.2 step 5); `std::fs::remove_*` is banned outside `remove_verified`; `std::fs::rename` is banned everywhere (the adapters rename only through the checked handle with no-replace semantics, ADR 0022), both via clippy `disallowed-methods`; copies open targets with `create_new`; Tauri commands restricted with `AppManifest::commands`; the webview gets no fs plugin permissions; cargo-deny (no cargo-audit), CodeQL (JS/TS + Rust), workflow lockfiles, pnpm supply-chain settings; tauri-specta pinned (still RC), `ts-rs` as fallback; cargo-fuzz targets on the EXIF/NEF parsers.

## 8. Scope

**v1:** everything above, Windows 11 only.

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
- Previews carry no ICC profile (v1): an AdobeRGB or Display P3 JPEG is shown as if it were sRGB, so its colours look duller or shifted. The author shoots sRGB, so this is accepted for v1 (decided 27 Nov 2026); copying the profile or converting to sRGB is a later option.
- Timings in the research notes are estimates and must be benchmarked on a real SD card (milestone 1).
- Moving 2,000 pairs across volumes is about 140 GB of files, but about 280 GB read (the source is read twice for verification), so 50+ minutes from a UHS-I card. The review screen shows an estimate that includes the verification reads.

## 10. Open questions
- `xtask`: ADR 0020 says it is outside the file rules, but it inherits the workspace lints and only creates new files. Proposed: keep it inside the rules and supersede ADR 0020 on that point.
- Minimum WebView2 version and installer mode (the bootstrapper downloads WebView2 at install time if missing).
- Which photos to publish in the demo.
- Demo deploy: pin the Wrangler version in the Workers Builds deploy command (`npx wrangler@<version> deploy`) rather than fetching the latest at each deploy, and whether to replace the default build token with one scoped to the `culla` Worker; to decide (raised 27 Nov 2026, [deploy-demo.md](deploy-demo.md)).
- The author's real sample files (Nikon Z series, see §2): 9 Z6III NEFs (8 pairs, 1 lone) used locally since 27 Nov 2026, outside the repo; the stripped Git LFS set is pending.
- Multi-scan JPEGs in previews: libjpeg-turbo holds the whole coefficient buffer of a multi-scan JPEG, progressive or non-interleaved (a first scan with fewer components than the frame, even in baseline SOF0), whatever the decode scale (about 6 bytes per pixel at 4:4:4), so a 100 MP multi-scan file could take about 600 MB in one preview call. M1 refuses multi-scan frames over 25 MP as a provisional safe default (`preview::Limits::max_multi_scan_pixels`; Nikon bodies write interleaved baseline). At that cap, four decode workers on 4:4:4 multi-scan files could still hold about 600 MB of coefficients on top of the 200–240 MB of buffers, near the 1 GB must (NFR-PERF-09), so a lower cap or one multi-scan decode at a time may be needed. Since M1 slice 7 the preview pipeline runs at most one multi-scan decode at a time and reserves its coefficients (`preview::coefficient_bytes`) in a hard working-memory budget, so the worst case stays bounded whatever the files; proposed by the agent, provisional. Still to decide: keep the 25 MP cap with one multi-scan decode at a time, lower or drop the cap, or refuse multi-scan files altogether (raised 27 Nov 2026, threat model T5, NFR-PERF-09).
- Preview chroma subsampling: previews are encoded at q88 with 4:2:0, which halves colour resolution; 4:4:4 keeps fine colour edges for judging sharpness at a larger file (est. 1.3–1.5×) and a slower encode. Keep 4:2:0 or switch to 4:4:4, once B-06 and B-07 are measured on the G14; to decide (raised 27 Nov 2026).
- Preview caps on camera JPEGs: the 64 MiB and 100 MP caps of the NEF walker also apply to camera JPEGs (`preview::Limits::DEFAULT`). A 45 MP JPEG is 15–25 MB, so they hold for Nikon Z files; keep one set of caps for both sources or set separate ones; to decide (raised 27 Nov 2026).
- Preview cache when the source drive is encrypted and the system drive is not: keep previews in memory only (default) or warn before caching to disk; to decide (raised 27 Nov 2026).
- Code of conduct: adopt one, with which text and which reporting channel.
- Incident response: confirm the procedures in [incident-response.md](incident-response.md), drafted by an agent on 27 Nov 2026 and not yet reviewed.
- `culla scan` (M1 slice 6), proposed by the agent and awaiting Margaux: exit codes 0 done, 1 usage error, 2 folder refused, 3 folder unreadable; exit 0 also when some groups have metadata errors or folders are held back (they are reported in the output, not in the code); off Windows, scan the folder anyway and report the volume identity as unavailable (rather than refuse); the `--json` field set as it stands, marked unstable until M2 (`"schema": "unstable"` in the output and a note in `culla help scan`); std-only argument parsing and a hand-written JSON writer in `culla-cli` rather than `clap` and `serde_json`.
- Preview pipeline (M1 slice 7), proposed by the agent and awaiting Margaux; the SPEC §5.1 values (5 ahead, 2 behind, about 12 previews, 1–2 readers, 2–4 workers) are kept and the rest is provisional until B-09 and B-10: 1 reader thread; decode workers half the logical CPUs, 2 to 4 (4 on the G14); priority current, then ahead nearest first, then behind; moving backward flips the window (5 earlier, 2 later); cache capped at 12 previews and 48 MiB, evicting least recently used outside the window first, then the window from its lowest priority up, and refusing a preview that would itself be evicted first; hard budgets of 128 MiB for files read and 256 MiB for decode buffers and coefficients (`prefetch::Budgets::DEFAULT`, a 432 MiB ceiling with the cache), each call reserving its estimated working bytes including the resize's intermediate image and `preview::DECODER_SLACK_BYTES` (8 MiB) for libjpeg-turbo's own memory; a job waiting for working memory is not overtaken by a lower-priority one; a preview larger than the whole cache fails its photo (`TooLargeForCache`), and one the cache refuses is reported as not admitted until the window moves; a failed photo is not retried until asked (a pulled card stays failed until a retry); a decode already running when its photo leaves the window finishes and its result is dropped; std threads with one mutex and one condition variable, no new dependency (architecture §8.3 said rayon); the disk cache is not built yet.
- Pipeline memory budgets: the 432 MiB ceiling is a hostile-file worst case, and with the other NFR-PERF-09 estimates it exceeds the 600 MB should. Lower the budgets (for example 64 MiB of input or 192 MiB of working memory, at the cost of fewer parallel decodes), or keep them until B-10 measures the real peak? To decide (raised 27 Nov 2026).
- Preview thread restarts (M1 slice 7), proposed by the agent and awaiting Margaux: after an unwind outside the per-photo isolation, a reader or worker loop is started again after a pause that doubles from 10 ms (`pipeline::RESTART_MIN`) to 1 s (`RESTART_MAX`) and goes back to 10 ms once a loop ran at least 1 s before unwinding; the pause ends at once on shutdown. Both values are provisional (architecture §8.3).
- `culla preview` (M1 slice 7), proposed by the agent and awaiting Margaux: text output only (no `--json` yet); read-only, it writes nothing but stdout and stderr; exit 0 when single previews fail (they are listed); the photos are requested one after another as fast as they come (no cadence: that is `culla-bench swipe`); a photo not ready within 120 s is reported as timed out and the command moves on (`PHOTO_TIMEOUT`, provisional), so it can never wait forever.
- Writing previews to a folder from the CLI (`--out`) was dropped to keep ADR 0020 and INV-03; it would need a new ADR if wanted.
