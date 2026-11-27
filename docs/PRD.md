# Culla: product pitch (v1)

Status: draft, 24 Nov 2026. Format: Shape Up pitch (problem, appetite, solution, rabbit holes, no-gos), plus success criteria. Source of truth for decisions: [SPEC.md](SPEC.md) and the [ADRs](adr/). The research notes in `research/` are historical input; where they disagree, the SPEC and ADRs win.

This document owns the problem, the users, the scope, the no-gos and the success criteria. Performance, reliability, accessibility, privacy and compatibility targets live in [nfr.md](nfr.md).

## 1. Problem

A photographer comes home with one or more SD cards from a Nikon body set to RAW+JPEG. A day of shooting is often 1,000 to 3,000 frames, so 2,000 to 6,000 files, spread over several `DCIM` subfolders. Most of them will be thrown away. The job is to look at each frame once, decide keep or reject, and end up with the keepers (JPEG, NEF and any sidecars together) in a destination folder, without losing a single file by mistake.

Today that job is either slow, risky or expensive:

| Tool | What it does well | Why it falls short for this job |
|---|---|---|
| [QuickRawPicker](https://github.com/RawLabo/QuickRawPicker) | Open source, cross-platform, fast keyboard culling. Copies the picks with their JPEGs and `.xmp` files. | Copies picks rather than sorting the card in place. As far as we know, no full operation plan to review before files move, and no journal to resume an interrupted copy. |
| [FastRawViewer](https://fastrawviewer.com/node/318) | Very fast RAW rendering, treats RAW+JPEG as one unit. | Paid and closed. Dense UI built for technical RAW inspection rather than a quick yes/no pass. To check: whether it stages moves for review or applies them as you go. |
| Photo Mechanic | Widely used for fast ingest and culling. | Paid and closed. Aimed at professional ingest, captioning and metadata workflows, which is far more than a keep/reject pass needs. |
| Lightroom Classic | Full catalogue and editor. | Needs an import into a catalogue first, previews are slow to build on thousands of files, and it is a subscription. Files moved outside Lightroom later show as missing in its catalogue. |
| Explorer / Photos | Free and already there. | No RAW+JPEG pairing, no sidecars, no batch decisions. Every delete or move is immediate. |

Prior art also includes [Louppe](https://github.com/alexander-markin-meow/louppe/releases) (open source, macOS only), which recovers interrupted moves and protects sidecars. Nothing equivalent exists on Windows.

The gap Culla fills: on Windows, a **pair- and sidecar-aware file engine that is provably safe**: it never deletes a photo without a verified copy at the destination, never overwrites (a rename fails if the target exists), journals every operation and resumes an interrupted apply. It sits behind a **review step** where nothing touches the disk until the user confirms, and a **swipe-first interface** that makes the yes/no pass fast and pleasant. See [SPEC.md §1](SPEC.md#1-what-culla-is).

## 2. Target users

1. **The author first.** A Nikon shooter on a ROG Zephyrus G14 2024 (Windows 11, 2880×1800 OLED, 120 Hz), culling thousands of RAW+JPEG files straight from SD cards or an external drive. Every v1 decision is checked against this setup.
2. **Photographers on Windows who shoot RAW+JPEG**, mostly Nikon, who want a free, open-source culling step before their editor, and who care more about not losing files than about a long feature list.

Not targeted in v1: macOS and Linux users, studios with multi-user or catalogue workflows, people who need ratings, colour labels or metadata editing.

## 3. Appetite

Solo project, one author with Claude Code agents, open source (GPL-3.0-only). The appetite is the v1 scope in [SPEC.md §8](SPEC.md#8-scope), Windows only, and nothing more. When a feature threatens the appetite, it moves to the v2 list rather than stretching v1.

To decide: a calendar time budget for v1 (weeks per milestone). Milestones are in [roadmap.md](roadmap.md) (M0 skeleton, M1 scan and preview benchmarks, M2 swipe, M3 apply engine, M4 web demo, then v1).

## 4. Solution outline

The details live in other documents. This is the shape only.

1. **Start.** Pick a source folder (subfolders included), the "RAW follows JPEG" toggle, keep = move or copy, and a destination. A session that already exists for that folder can be resumed, even if the SD card comes back under another drive letter.
2. **Swipe.** One photo at a time, full screen on a dark (Night) surround. Right = keep, left = reject, by keyboard, drag or the Keep and Reject buttons. A JPEG, its NEF and its sidecars are one group, decided once. Sidecars follow the file they describe: `X.NEF.xmp` follows the NEF, so it stays put when "RAW follows JPEG" is off. A lone NEF is shown through its embedded JPEG. Zoom to 100%, EXIF overlay, filters, jump to next undecided, global undo/redo.
3. **Review.** Counts, kept and rejected grids, the full operation plan, preflight results, estimated time and size. Apply needs an explicit confirmation that states the counts. Undecided photos are never touched.
4. **Apply.** Rejects move to `<source>/_rejected/`, keeps move or copy to the destination, subfolders mirrored. Every operation is journalled. Culla never overwrites: a rename fails if the target exists. It never deletes a photo without a verified copy at the destination: the only removals are the source of a cross-volume move that passed verification (fsync, destination read back without the OS cache, source re-read, all three hashes matching), and Culla's own temporary `*.culla-partial` files. An interrupted apply rolls forward on the next launch. Ends with a report and a log file.
5. **Web demo.** The same UI in a browser, with about 25 sample photos and a simulated apply, plus an optional read-only "your folder" mode.

Where to read more:

| Topic | Document |
|---|---|
| Interaction, keyboard map, screens | [SPEC.md §4](SPEC.md#4-interaction), later [ux.md](ux.md) and [stories.md](stories.md) |
| Group model, event log, apply algorithm, resume | [design/file-engine.md](design/file-engine.md) |
| Containers, components, runtime flows, risks | [architecture.md](architecture.md) |
| Budgets and quality targets | [nfr.md](nfr.md) |
| Security | [threat-model.md](threat-model.md) |
| Decisions and alternatives | [adr/](adr/) |

## 5. Rabbit holes

Known traps, and how v1 stays out of them:

- **RAW decoding.** v1 never decodes RAW pixels. Previews come from the JPEG or the NEF's largest embedded JPEG. HE/HE\* NEFs have no open-source decoder, so 100% zoom on a lone HE NEF shows the embedded JPEG ([SPEC §9](SPEC.md#9-known-limits-and-risks)).
- **SD card I/O.** Storage, not CPU, is the bottleneck (about 200–280 ms to read one 45 MP JPEG on UHS-I). Do not try to make the first pass instant for every photo; prefetch in swipe order and benchmark early (M1, [benchmarks.md](benchmarks.md)).
- **Sorting by capture time.** Reading every header before showing photo 1 is slow. Start in filename order, stream EXIF in, never re-sort under the user once swiping has started.
- **Colour management.** The G14's wide-gamut OLED and WebView2's partial profile support can make previews look wrong. v1 tests sRGB, AdobeRGB and Display P3 JPEGs; it does not build its own colour pipeline.
- **Renamed files.** v1 does not detect renames between sessions. They show as one missing plus one new file.
- **Rollback.** Undoing an apply means copying everything back after a cross-volume move. v1 promises roll-forward resume only, and writes the log that a later undo would need.
- **Lightroom catalogues.** Culla does not try to keep a Lightroom catalogue in sync. The README warns that moved files show as missing there.
- **Web demo file access.** The File System Access API is Chromium-only. The demo stays read-only with `<input webkitdirectory>`.

## 6. No-gos (v1)

Out of v1, listed as v2 candidates in [SPEC §8](SPEC.md#8-scope):

- Undo an apply (reverse plan). The v1 log file makes it possible later.
- Dual-slot detection (RAW and JPEG on two cards) and multiple sources.
- A/B toggle against the previous photo, then a burst comparison view.
- Zoom on the AF point (Nikon MakerNotes).
- macOS and Linux builds.
- Multiple destinations on number keys.
- Star ratings and XMP output.
- Code signing, winget and the Microsoft Store.

Also out, by design rather than for later: more than two choices (keep / reject only), any removal other than the source of a verified cross-volume move and Culla's own `*.culla-partial` files, editing, a catalogue, cloud sync, accounts, telemetry.

## 7. Success criteria

v1 is a success when all of the following hold. The targets were confirmed by Margaux on 27 Nov 2026; the benchmark method is in [benchmarks.md](benchmarks.md) and the matching requirements in [nfr.md](nfr.md).

| # | Criterion | Target | How it is checked |
|---|---|---|---|
| S1 | Zero lost files | Cull and apply 1,000 Nikon RAW+JPEG pairs straight from an SD card (move, cross-volume) with zero lost, overwritten or corrupted files | Before/after file inventory with hashes on the real card; [nfr.md](nfr.md) NFR-REL-01 to 07 |
| S2 | Interrupted apply recovers | Pulling the card or killing the app mid-apply, then relaunching, ends with every file either at its source or verified at its target, never both missing | Fault-injection tests in [testing.md](testing.md), plus one manual card-pull run |
| S3 | Fast decisions | The author culls 1,000 pairs from SD in 30 minutes or less of swiping time (about 1.8 s per photo, decision time included) | Timed session log on the G14 |
| S4 | The app never makes the user wait between photos once prefetch has caught up | Next photo visible within the budget of NFR-PERF-03 for at least 95% of decisions | Instrumented benchmark run |
| S5 | Keyboard only | Every flow (start, swipe, review, apply) can be done without a mouse | Manual pass + Playwright on the demo |
| S6 | Web demo live | Demo deployed at `culla.margauxfaurie.com` (behind the site's Cloudflare Access policy until the portfolio opens, public after), loads and runs the full swipe-review-apply flow on sample photos | Playwright in CI green against the deployed URL, gated or public |
| S7 | Portfolio-ready repository | README with GIF, ADRs for the main decisions, green CI, OpenSSF Scorecard published | Repository check before the v1 release |

## 8. Portfolio and blog goals

Culla is also a portfolio piece. v1 ships two outputs beyond the app:

- **Web demo** at `culla.margauxfaurie.com`, hidden behind the site's Cloudflare Access policy for now and public when the portfolio opens (decided by Margaux, 27 Nov 2026), in the same visual family as the site through the Portfolio Design System ([SPEC §5.5–5.6](SPEC.md#55-web-demo)). It lets a visitor try the swipe, review and simulated apply in under a minute, without installing anything.
- **Blog enablement guide**: a write-up of how Culla was built with coding agents, using the ADRs and the material listed in [research/practices.md §4](research/practices.md#4-material-for-the-blog-enablement-guide): guideline vs guarantee (how "never delete" is enforced by lints and CI, not prompts), agent-sized slices, the hexagonal core, event-sourced undo, the journalled apply, Tauri hardening, fuzzing the NEF parser, the supply-chain baseline and honest caveats.

To decide: whether the guide is one post or a series, and whether it ships with v1 or after.

## 9. Open questions

From [SPEC §10](SPEC.md#10-open-questions):

- Minimum WebView2 version and installer mode (the bootstrapper downloads WebView2 at install time if missing).
- Which photos to publish in the demo.
- The author's real sample files (Nikon Z series, SPEC §2) and their licence: pending.

Raised by this pitch:

- Calendar time budget for v1 (section 3).
- Form and timing of the blog guide (section 8).
