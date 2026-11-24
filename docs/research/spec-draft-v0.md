> Historical input, written before the decisions were settled. Where it disagrees with `docs/SPEC.md` or an ADR, those win.

# Photo Swipe Sorter — Draft Technical Spec (v0)

## 1. Goal
Desktop app (Windows first) that opens a folder of Nikon photos (.jpeg + .nef pairs), shows them one by one in a swipe interface, and lets the user decide keep / reject per photo. Decisions are queued and only applied to disk after a review screen. Open source, portfolio piece, with a browser demo on the author's website.

## 2. User decisions already made
- Platform: Windows desktop first (laptop: ASUS ROG Zephyrus G14 2024).
- Decisions: Keep / Reject only (binary swipe).
- Reject = move to a `_rejected` folder (not recycle bin, not permanent delete).
- Keep = move OR copy, chosen per session; destination folder picked per session (last one remembered).
- Apply mode: queue all decisions, review screen, then apply. Nothing touches disk before apply.
- RAW pairing: per-session toggle "RAW follows JPEG" (applies to keep and reject).
- Lone .nef (no matching .jpeg): shown via embedded JPEG preview, sorted like any other photo.
- Subfolders included; destination mirrors source subfolder structure.
- Typical volume: thousands of photos, often on SD card / external drive.
- v1 extras: zoom to 100%, EXIF overlay, resume later (decisions persisted per folder).
- Animations matter (swipe physics).

## 3. Interaction model
### Keyboard map
| Key | Action |
|---|---|
| → | Keep current photo (only if undecided), advance to next |
| ← | Reject current photo (only if undecided), advance to next |
| Shift+→ / Shift+← | Navigate next/previous without deciding |
| Ctrl+Z / U | Undo decision on current photo (back to undecided) |
| Z or Space (hold?) | Zoom 100% |
| I | Toggle EXIF overlay |
| Enter | Open review screen |
| Ctrl+Enter (in review) | Apply queue |
| Esc | Close review / zoom |

Mouse/trackpad: drag card left/right with threshold + velocity → swipe. Touch later.

### Photo states
`undecided → kept | rejected` ; undo returns to `undecided`. Plain ←/→ on an already-decided photo does nothing (must undo first). Decided photos show a badge.

### Screens
1. Start: pick source folder, options (include subfolders, RAW follows JPEG, keep = move/copy, destination folder), resume prompt if prior session exists.
2. Swipe: full-screen photo card, progress (n / total, kept, rejected), badge, EXIF overlay, zoom.
3. Review: counts, grid of kept/rejected thumbnails, list of file operations, warnings (collisions, missing files, disk space for copies), Apply button.
4. Apply progress + result report; errors listed, retry.

## 4. Architecture (proposed)
- Tauri 2 desktop shell; UI in React + TypeScript + Vite; animations with Motion (motion.dev).
- Rust backend (Tauri commands):
  - Folder scan (recursive), pairing by basename (case-insensitive: DSC_0001.JPG + DSC_0001.NEF), ordering by capture time (EXIF DateTimeOriginal) fallback filename.
  - Preview pipeline: decode JPEG → downscaled display image (screen-sized) + small thumbnail; for lone NEF, extract embedded JpgFromRaw / PreviewImage. Prefetch N ahead / M behind; LRU memory cache; on-disk thumbnail cache (app data dir, keyed by path+size+mtime).
  - EXIF read (shutter, aperture, ISO, focal length, lens, date).
  - Session store: JSON or SQLite per source folder (in app data dir, keyed by folder path), autosaved on each decision.
  - Apply engine: builds an operation plan (move/copy src→dst, with paired NEF + sidecars?), checks collisions/disk space, executes with a journal (write-ahead log) so an interrupted apply can be resumed/rolled back; copy verifies size (hash optional) before considering done; move across volumes = copy+verify+delete.
- Images served to the webview via Tauri asset protocol / custom protocol, not base64 over IPC.
- Web demo: same React UI with a "mock backend" adapter (sample photos bundled, operations simulated), deployed as static site. Possible real-folder mode via File System Access API (Chromium only).

## 5. Repo / engineering
- Monorepo: `apps/desktop` (Tauri), `apps/web-demo`, `packages/ui` (shared React), `crates/core` (pure Rust logic: scan, pairing, plan, journal — no Tauri dependency, unit-testable).
- pnpm workspaces; ESLint + Prettier; rustfmt + clippy; Vitest; Playwright on web demo; cargo test on core.
- GitHub Actions: CI on PR (lint, test, build), release workflow with tauri-action producing Windows installer; release-please; Conventional Commits.
- MIT license, README with GIF, CONTRIBUTING, CODE_OF_CONDUCT, issue/PR templates, CHANGELOG, SECURITY.md.
- Code signing: postponed for v1; README explains SmartScreen warning.

## 6. Scope
v1: everything in §2–§5 for Windows. v2 ideas: Mac/Linux builds, multiple destinations on number keys, star ratings / XMP, burst comparison, touch, signing.

## 7. Open questions
- Sidecar files (.xmp, .pp3) — move along?
- Same-basename collisions across subfolders when flattened (not applicable since mirroring).
- What if the user modifies the folder between sessions (files added/removed)?
