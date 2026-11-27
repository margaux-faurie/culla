# Threat model

Status: draft, 24 Nov 2026. Method: the four OWASP questions, with STRIDE as prompts ([pre-project-docs.md](research/pre-project-docs.md)). Updated at each milestone. File engine details are in [design/file-engine.md](design/file-engine.md); containers in [architecture.md](architecture.md).

## 1. What are we working on

A Windows desktop app that reads photos from a folder (often an SD card), shows previews in a webview and, after an explicit confirm, moves or copies them. It makes one network call, the update check. A static web demo runs the same UI with bundled photos.

```mermaid
flowchart LR
  user([User])
  subgraph pc["User's PC (trust: the logged-in user)"]
    subgraph wv["Webview: React UI (least trusted inside the app)"]
      ui[UI]
    end
    subgraph rust["Rust process"]
      cmd[Tauri commands<br/>AppManifest::commands]
      scheme[culla:// preview handler]
      core[culla-core]
      ad[culla-adapters<br/>winfs, parsers, decode, store]
    end
    db[(Session DBs, cache, logs<br/>%LOCALAPPDATA%\Culla)]
  end
  subgraph media["Removable media (untrusted content)"]
    card[(Source: SD card or drive)]
  end
  dest[(Destination folder)]
  subgraph net["Internet"]
    upd[GitHub Releases<br/>latest.json + signed bundle]
    demo[Demo site<br/>Cloudflare static assets]
  end
  subgraph sc["Build and supply chain"]
    ci[GitHub Actions] --> reg[(crates.io, npm)]
  end
  visitor([Demo visitor's browser])

  user --> ui
  ui -- "IPC: opaque IDs" --> cmd
  ui -- "GET culla://preview/id" --> scheme
  cmd --> core --> ad
  scheme --> ad
  ad -- "read; move/copy on apply" --> card
  ad --> dest
  ad --> db
  rust -- "HTTPS, signature checked" --> upd
  ci -- "builds, signs" --> upd
  ci -- "deploys" --> demo
  visitor --> demo
```

Trust boundaries: (B1) webview ↔ Rust process; (B2) file contents and names on the media ↔ parsers and the file engine; (B3) update endpoint ↔ updater; (B4) third-party packages and CI ↔ release artifacts; (B5) demo site ↔ visitor.

### Assets

| Asset | Property that matters | Why |
|---|---|---|
| User photos (JPEG, NEF) | Integrity, availability | Often irreplaceable; the card may be formatted after an apply |
| Sidecars (`.xmp`, `.pp3`, `.nksc`) | Integrity, staying with their photo | Hold the user's edits |
| Session DB (decisions, plans, journal) | Integrity | Drives resume; a wrong journal could misdirect recovery |
| Preview cache | Confidentiality (low) | Copies of the user's photos on the internal disk |
| Updater channel and signing key | Integrity, key custody | Whoever controls both can run code on every install |
| Release pipeline and dependencies | Integrity | Same reach as the updater |
| Demo site and its photos | Integrity; author privacy | Public; EXIF can reveal location |

## 2. What can go wrong, and 3. what we will do

Status: *planned* (designed, not built), *decided* (in an ADR or SPEC), *open* (to decide).

| # | STRIDE | Threat | Component | Mitigation | Status |
|---|---|---|---|---|---|
| T1 | T | Path traversal via crafted file names on a card (`..`, `\`, `:`, device names, trailing dots), made worse because `\\?\` paths skip normalization | Scan, `plan()`, `winfs` | Validate every name component; build paths only from validated components; P7 checks every target is under its root and is no source path; invalid names are listed and never touched | planned ([file-engine §3.4, §8](design/file-engine.md)) |
| T2 | T, E | Symlink, junction or mount point redirects a move, or swaps a file between check and act (cf. CVE-2022-21658) | Scan, `winfs` | Walk with `follow_links(false)`; refuse every reparse point at scan, preflight and each step; open with `FILE_FLAG_OPEN_REPARSE_POINT`; `make_dirs` refuses reparse components; renames and removals act through the same file handle that was checked, held with no write or delete sharing; the target folder is opened with `FILE_FLAG_OPEN_REPARSE_POINT`, checked to be a real directory, held without delete sharing, and the rename uses it as `RootDirectory` with a relative leaf name | planned |
| T2b | T, D | Cloud placeholders (OneDrive and other sync clients): reading one triggers a download, and the sync client can change or remove the file during an apply | Scan, preflight | Detect cloud reparse tags and the `RECALL_ON_DATA_ACCESS`, `RECALL_ON_OPEN` and `OFFLINE` attributes; placeholder files are never grouped; a source that is a placeholder is refused. The NEF preview reader (`nef_preview`, PR #18) uses a plain read-only open, so until these checks land it must only get paths from the current scan; reparse-point and placeholder refusal on open is added when it is wired to the CLI or `culla://`. A destination inside a synced folder is allowed, with a warning in the review screen (decided by Margaux, 27 Nov 2026; file-engine P17) | planned, decided (destination) |
| T3 | T | Overwrite of an existing file through a race | `winfs` | No-replace rename through a handle (`FileRenameInfoEx` without `REPLACE_IF_EXISTS`, same semantics as `MoveFileExW` without `MOVEFILE_REPLACE_EXISTING`); `create_new`; no check-then-act; Culla never opens a user file for writing | decided ([ADR 0004](adr/0004-never-delete-or-overwrite.md)), mechanism in [file-engine §9.3](design/file-engine.md) |
| T3b | S, T | Two volumes with the same serial, or a drive letter that changes mid-run, redirect operations to the wrong volume | `winfs`, apply | Each run binds the serial to one `\\?\Volume{GUID}\` path and uses it for every operation; two matching volumes pause the run and the user picks; "same volume" is decided by GUID | planned |
| T4 | T, D | A bug or a crafted state removes a photo | `winfs` | One audited `remove_verified` with unforgeable proofs; clippy `disallowed-methods` on every other removal or rename; grep check on `allow`; Margaux is a required code owner for every protected path (`.github/CODEOWNERS`, in effect once the ruleset requires code-owner review) | decided, planned |
| T5 | D, E | Malicious EXIF or NEF: out-of-bounds offsets, IFD loops, huge counts, decompression bombs | EXIF reader, IFD walker, turbojpeg | Bounds-checked parser with no `unsafe`; visited-offset set and depth and entry caps; max preview size and pixel count before decode; per-decode error isolation (a panic fails one preview, never the apply); cargo-fuzz targets for the IFD walker, embedded-JPEG extraction and EXIF, run weekly, crashes kept as regression tests | partial: the IFD walker's bounds checks, visited set, depth, entry and count caps, preview size and pixel caps and the `tiff_ifd_walk` target are in place (PR #18); decode isolation, the other fuzz targets and the first libFuzzer run (with `fuzz/Cargo.lock`) are to come |
| T6 | I, T | Text from EXIF (lens, comments) or file names injects markup into the UI (XSS) | UI | React text rendering only, no `dangerouslySetInnerHTML`; CSP `script-src 'self'`, no `eval`, no remote origins | planned |
| T7 | E | Compromised webview abuses IPC to read or move arbitrary files | Commands | `AppManifest::commands` allowlist in `build.rs`; one capability for `main` with minimal `core:*`; no fs, shell or opener-to-path plugin permissions; commands take opaque IDs, never paths; folders picked through a native dialog run from Rust; the asset protocol is off | partial: allowlist, single capability with only `allow-version`, no plugins and asset protocol off are in the M0 shell (PR #12); opaque IDs and the native folder dialog come with the first file commands |
| T8 | E | Compromised webview fabricates decisions and confirms an apply | Commands, apply | Impact bounded: only keep/reject moves inside the chosen roots, nothing removed except verified move sources. Further option: a native (Rust-side) confirm dialog stating the counts | open |
| T9 | I | `culla://` handler used to read other files | Preview scheme | Serves only IDs from the current scan, from memory or the cache; strict ID parsing, no path in the URL; `X-Content-Type-Options: nosniff`; CSP `img-src` allows only this origin | planned |
| T10 | S | Navigation to a remote page inside the webview | Shell | Block navigation off the app origin; external links open in the system browser; isolation pattern judged optional (ADR to record) | planned, open (ADR) |
| T11 | S, T | Tampered or spoofed update | Updater | `tauri-plugin-updater` signature check with the inline public key (cannot be turned off); HTTPS; private key only in a protected GitHub environment secret, with an offline backup | decided (SPEC §6) |
| T11b | T | Rollback to an older, vulnerable version through a validly signed old release | Updater | Keep the plugin's default version check (install only a version newer than the running one); no custom comparator that allows downgrades | planned |
| T11c | D | An update restarts the app in the middle of an apply, or a new version changes the journal format under an open plan | Updater, apply | No update check, download or install while any plan is open (`confirmed`, `applying`, `paused`, `interrupted`); if a new version still finds an open plan (for example after a manual install), it first recovers and finishes that plan with the old schema's journal rules, and only then migrates the database | planned |
| T12 | T | Supply-chain compromise of a crate, npm package or action | CI, build | `cargo-deny` (advisories, bans, licences, sources); pnpm `allowBuilds`, `strictDepBuilds`, `blockExoticSubdeps`, `minimumReleaseAge`, `trustPolicy: no-downgrade`; committed lockfiles; actions pinned by SHA or `actions.lock`; `permissions: {}` per workflow; Dependabot with cooldown; CodeQL (JS/TS, Rust, Actions); Scorecard | decided (SPEC §7) |
| T12b | T, E | A manually dispatched `Refresh lockfiles` workflow has `contents: write` | CI | Only repo writers can dispatch; never main; credentials not persisted while branch code runs; pnpmfile ignored; cargo config flags refused; pushes only Cargo.lock and pnpm-lock.yaml | in place (PR #17) |
| T13 | S | Another card with the same 32-bit serial and folder matches an old session | Session, reconcile | Per-file checks before every operation; recovery re-hashes before any removal; reconcile flags a mostly-different folder | planned, threshold open |
| T14 | T | Local process edits the session DB to misdirect recovery | Store, recovery | Recovery trusts the disk over the journal (re-inspects, re-hashes); removal needs a fresh in-process proof. A same-user attacker is out of scope | planned |
| T15 | R | "The app lost my photo": no record of what happened | Apply | Append-only journal (triggers) and a plain-text log per apply | planned |
| T16 | I | Preview cache exposes photos to other users of the PC | Cache | Lives in the user's `%LOCALAPPDATA%` (per-user ACL); no telemetry; the only network call is the updater | planned |
| T17 | I | Demo reveals the author's location through EXIF GPS | Demo build | Strip GPS and serial numbers when pre-extracting EXIF and encoding AVIF/WebP | planned |
| T18 | I, T | Demo "your folder" mode leaks a visitor's files, or the site serves altered code | Demo | Static assets only, no backend, no uploads; files read in the browser only; CSP `connect-src 'self'`; no third-party scripts; Cloudflare account with 2FA and a scoped deploy token | planned |
| T19 | D | Huge or deep folder trees exhaust memory or time | Scan | Streamed scan, folder by folder; caps on depth and entries per folder | planned |
| T20 | S | Unsigned installer trains users to click through SmartScreen | Distribution | README explains; checksums on releases; SignPath Foundation after v1 | decided (SPEC §6) |

## 4. Did we do enough

At each milestone, check:

- Every invariant in [file-engine §11](design/file-engine.md) has a passing test, including the crash-after-every-step suite.
- The capability file, `AppManifest::commands` list and CSP match this document (reviewed by the read-only security reviewer agent).
- Fuzz targets ran in the last week with no open crash.
- `cargo deny check`, CodeQL and Scorecard are green; no new `allow(clippy::disallowed_methods)` outside `winfs`.
- Open items (T8, T10, T13) are decided or carried forward explicitly.

## Out of scope

- An attacker running code as the same Windows user: they can already read and change every file Culla can.
- Physical attacks and theft of the PC or card.
- Storage hardware or drivers that acknowledge a flush they did not perform, and file system corruption caused by removing media during a metadata write (Culla reports it; it cannot prevent it).
- Vulnerabilities in Windows, WebView2 (evergreen, updated by the OS) or the Tauri runtime itself, beyond keeping dependencies current.
- Network shares and multi-user setups (to decide whether v1 refuses network shares).
- Lightroom catalog consistency after files move (README warning).
- margauxfaurie.com beyond the demo subdomain.
