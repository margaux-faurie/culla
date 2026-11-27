# Architecture decision records

One decision per file, in [MADR 4](https://adr.github.io/madr/) minimal format. How we write them: [ADR 0000](0000-record-architecture-decisions.md). A changed decision gets a new ADR that supersedes the old one; accepted ADRs are not rewritten.

| # | Title | Status |
|---|---|---|
| [0000](0000-record-architecture-decisions.md) | Record architecture decisions with MADR | accepted |
| [0001](0001-use-tauri-2-rust-and-react.md) | Use Tauri 2 with a Rust backend and a React UI | accepted |
| [0002](0002-pure-rust-core-crate-without-tauri.md) | Keep the domain in a pure Rust core crate, with adapters in a separate crate and no Tauri in either | accepted |
| [0003](0003-stage-decisions-in-a-queue-until-explicit-apply.md) | Stage decisions in a queue; nothing touches the disk until an explicit apply | accepted |
| [0004](0004-never-delete-or-overwrite.md) | Never delete or overwrite: rejects go to `_rejected/`, collisions get a suffix per group | accepted (refined by 0021, 0022) |
| [0005](0005-decision-state-as-append-only-event-log.md) | Store decision state as an append-only event log | accepted (refined by 0024) |
| [0006](0006-arrow-keys-do-not-change-an-existing-decision.md) | Arrow keys don't change an existing decision (undo first) | accepted |
| [0007](0007-pair-files-by-basename-within-one-folder.md) | Pair files by case-insensitive basename within the same folder only | accepted (refined by 0023) |
| [0008](0008-key-sessions-and-caches-by-volume-serial-and-relative-path.md) | Key sessions and caches by volume serial + relative path | accepted |
| [0009](0009-sqlite-per-source-for-sessions-and-journal.md) | Use one SQLite database per source for sessions and the journal | accepted |
| [0010](0010-journaled-apply-with-roll-forward-only.md) | Journaled apply: copy, hash, fsync, verify, rename; roll forward only | accepted (refined by 0021, 0022) |
| [0011](0011-own-ifd-walker-for-nef-previews-and-kamadak-exif.md) | Own TIFF/IFD walker for NEF previews instead of rawler; kamadak-exif for EXIF | accepted, refined by 0025 |
| [0012](0012-turbojpeg-scaled-decode-preview-pipeline.md) | Preview pipeline: turbojpeg scaled decode + fast_image_resize; no mmap on removable media | accepted |
| [0013](0013-deliver-images-through-a-custom-uri-scheme.md) | Deliver images through a custom URI scheme, not IPC or the asset protocol | accepted |
| [0014](0014-backend-interface-with-build-time-implementations.md) | A `Backend` interface with build-time Tauri and Demo implementations | accepted |
| [0015](0015-motion-for-swipe-physics.md) | Motion for swipe physics; commit first, animation catches up, at most 3 cards mounted | accepted |
| [0016](0016-per-user-nsis-and-signed-updater-code-signing-deferred.md) | Distribution: per-user NSIS + signed updater; Windows code signing deferred | accepted |
| [0017](0017-mit-licence-and-cargo-deny-licence-allowlist.md) | MIT licence + cargo-deny licence allowlist | accepted; licence superseded by 0026 |
| [0018](0018-conventional-commits-and-release-please.md) | Conventional Commits + release-please for versioning | accepted |
| [0019](0019-generate-typescript-bindings-with-tauri-specta.md) | Generate TypeScript bindings with tauri-specta, pinned RC, ts-rs as fallback | accepted |
| [0020](0020-headless-cli-and-bench-crates.md) | Headless CLI and bench crates | accepted |
| [0021](0021-unbuffered-source-re-read-for-hash-3.md) | Unbuffered source re-read for hash 3 | accepted |
| [0022](0022-rename-through-the-checked-handle.md) | Rename through the checked handle, with no-replace semantics | accepted |
| [0023](0023-bare-sidecars-follow-the-nef.md) | Bare `X.xmp` and `X.pp3` follow the NEF, and stay with it when "RAW follows JPEG" is off | accepted |
| [0024](0024-reconcile-event-for-a-file-joining-a-decided-group.md) | A new file joining a decided group makes it undecided, through a reconcile event | accepted |
| [0025](0025-lone-nef-preview-is-the-largest-valid-embedded-jpeg.md) | A lone NEF's preview is its largest valid embedded JPEG, chosen by pixel count (refines 0011) | accepted |
| [0026](0026-gpl-3-0-only-licence-and-reserved-name.md) | GPL-3.0-only licence, reserved name and logo, no outside pull requests (supersedes the licence in 0017) | accepted |
