# Benchmark plan

Status: draft, 24 Nov 2026. The scan and preview benchmarks run in milestone M1, before any UI is built; the apply benchmarks (B-12 to B-14) run in M2, once the file engine exists ([roadmap.md](roadmap.md)). Targets are owned by [nfr.md](nfr.md) (`NFR-PERF`); the key ones are repeated below, marked "(est.)", so exit criteria can be checked from this page. This document says what to measure, how, and how a result maps to pass or fail. Numbers quoted here as "research estimate" come from [research/research-raw-perf.md](research/research-raw-perf.md) and are not targets.

## 1. Why before the UI

The timings in the research are estimates ([SPEC.md](SPEC.md) section 9). Storage, not the CPU, is expected to be the bottleneck on SD cards. If the pipeline cannot meet the budgets headless, no UI work will fix it. M1 ends only when every M1 metric below has a recorded result on the reference hardware, and each one either passes or has a decision (design change or revised budget, recorded in an ADR). M2 ends under the same rule for the apply metrics.

Key targets (all "(est.)", canonical values in [nfr.md](nfr.md)):

| Target | Value | Metric |
|---|---|---|
| First photo shown, from a UHS-I SD card | ≤ 1.5 s (est.) | B-01 (+ preview of photo 1) |
| Scan of 3,000 files including the EXIF pass, on SD | ≤ 15 s (est.) | B-02 + B-03 |
| Next photo when prefetched | ≤ 50 ms at p95 (est.) | B-09 |
| Memory (peak working set) | ≤ 1 GB (est.) | B-15 |

## 2. What to measure

| ID | Metric | Definition | Tool | Research estimate | Target | Milestone |
|---|---|---|---|---|---|---|
| B-01 | Scan, first group | Time from `scan()` start to the first group yielded, 3,000-file folder | `culla-bench scan` | about 1 s | ≤ 1.5 s to first photo (est.), `NFR-PERF` | M1 |
| B-02 | Scan, full listing | Time to list and group all 3,000 files (no EXIF) | `culla-bench scan` | — | part of ≤ 15 s for 3,000 files (est.) | M1 |
| B-03 | EXIF pass | Time to read the first ~128 KB and parse EXIF for all 3,000 files, then sort by capture time | `culla-bench scan --exif` | 5–15 s on SD | part of ≤ 15 s for 3,000 files (est.) | M1 |
| B-04 | EXIF header parse (CPU only) | Parse time per file from an in-memory buffer, JPEG and NEF | criterion | — | informational | M1 |
| B-05 | NEF preview extraction | Walk IFDs, locate the largest embedded JPEG, read it. Report bytes read and time, per NEF variant (lossless, HE, HE\*) | criterion (in memory) + `culla-bench preview --source nef` (from media) | — | `NFR-PERF`, feeds the 1.5 s and 50 ms targets | M1 |
| B-06 | Scaled decode | turbojpeg decode at 1/2 for 24 MP, at 3/8 and 1/2 for 45 MP, single thread | criterion | 50–60 % of a full decode (100–180 ms at 24 MP, 200–350 ms at 45 MP) | informational, feeds B-09 | M1 |
| B-07 | Resize + encode | `fast_image_resize` to fit 2880×1800, orientation applied, JPEG q85–90 | criterion | resize < 15 ms, encode 15–25 ms | informational, feeds B-09 | M1 |
| B-08 | Preview cache | Disk cache hit (read from `%LOCALAPPDATA%\Culla\cache`) and miss (full pipeline from media), per photo | `culla-bench preview` | — | `NFR-PERF` | M1 |
| B-09 | Next-photo latency | Time from "advance to photo i+1" to its encoded preview being in the in-memory LRU, under a simulated swipe cadence (section 4.3). Report p50, p95, p99 | `culla-bench swipe` | < 50 ms from cache | ≤ 50 ms at p95 (est.), `NFR-PERF` | M1 (headless), M3 (webview) |
| B-10 | Sustained swipe rate | Highest cadence where p95 of B-09 stays within budget | `culla-bench swipe --ramp` | 3–4 photos/s on UHS-I for 45 MP fine JPEGs | ≥ 3/s on SD UHS-I for 45 MP (est.), NFR-PERF-04 | M1 |
| B-11 | Preview source choice | B-09 using the camera JPEG versus the NEF's full-size embedded JPEG (JpgFromRaw) for pairs | `culla-bench swipe --preview-source jpeg\|nef` | NEF preview may be faster | decision input | M1 |
| B-12 | Apply, same volume | Time for 1,000 groups (about 3,000 files), journal included, on SD and NVMe | `culla-bench apply --same-volume` | near instant | ≤ 30 s for 1,000 groups (est.), NFR-PERF-10 | M2 |
| B-13 | Apply, cross volume | MB/s and total time for copy (hashing the source) + fsync + destination read-back and source re-read, both with `FILE_FLAG_NO_BUFFERING`, + rename + source removal ([SPEC.md](SPEC.md) section 5.2), SD → NVMe, and NVMe → external | `culla-bench apply --cross-volume` | about 90 MB/s on UHS-I; 140 GB in 25+ min for a single read (research), about 52+ min with the second source read | ≥ 85 % of the slower device's sequential throughput over the bytes read (the source is read twice) (est.), NFR-PERF-11; also checks the review-screen ETA (NFR-PERF-12) | M2 |
| B-14 | Verify overhead | B-13 with xxh3 versus blake3 versus no verify (bench only); the source re-read costs a second read of the card, so report it separately | `culla-bench apply` | — | recorded, no pass/fail; decision input for the hash choice | M2 |
| B-15 | Memory | Peak working set during B-09 and B-13 | `culla-bench` (reports peak RSS) | ~12 previews in LRU | ≤ 1 GB (est.), `NFR-PERF` | M1 (swipe), M2 (apply) |
| B-16 | Medium baseline | Raw sequential read speed of the medium, 1 GB file, buffered reads | `culla-bench io-baseline` | about 90 MB/s UHS-I | reference for B-03, B-10, B-13 | M1 |

Notes:
- B-06 and B-07 are CPU-only and run in criterion with the image already in memory. Everything that touches the medium runs in `culla-bench`.
- B-09 is a headless proxy. The real UI latency (key press to the new card's `img.decode()` resolving) is measured again in M3 with `performance.mark` in the webview, against the same target (≤ 50 ms at p95, est.).
- Tuning knobs to sweep during B-09 and B-10: number of reader threads (1, 2), decode workers (2, 4), prefetch window (ahead 3–6, behind 2), LRU size (12, 24, 50). SPEC says 5 ahead, 2 behind, about 12 previews. The research suggested about 50 in RAM. The benchmark decides.

## 3. Hardware and media

| ID | Machine | Medium | Connection | Notes |
|---|---|---|---|---|
| H-1 | ROG Zephyrus G14 2024 (Ryzen 9 8945HS) | internal NVMe | — | reference for CPU and cache-hit numbers |
| H-2 | same | UHS-I SD card (record brand, size, speed class) | USB card reader (record model and USB port) | reference for cold numbers; the main target |
| H-3 | same | microSD | built-in UHS-II reader | secondary |
| H-4 | same | external SSD or HDD (if available) | USB | cross-volume apply destination, optional |

Record for each run: Windows build, power source (plugged in), ASUS power mode (fixed, for example "Performance"), Defender real-time scanning state (left on, since users have it on), free RAM, file system of the medium (exFAT on most cards).

## 4. Method

### 4.1 Datasets

| ID | Content | Use |
|---|---|---|
| D-1 | 3,000 synthetic small files (from `cargo xtask fixtures --bench`) in a DCIM-like tree | scan overhead without I/O weight (B-01, B-02) |
| D-2 | A real card: about 3,000 files, JPEG + NEF pairs, some lone NEFs, some sidecars, as they come off the camera | B-01 to B-03, B-05, B-08 to B-11 (M1); B-12 to B-14 on a copy (M2) |
| D-3 | 24 MP and 45 MP sets (about 200 photos each) | B-06, B-07, B-09 to B-11 per resolution |

D-2 and D-3 depend on the author's Nikon body or bodies (open question in [SPEC.md](SPEC.md) section 10). If only one resolution is available from her bodies, the other one can come from public sample files (for example raw.pixls.us), used locally and never committed; check each file's licence. Benchmark data is never committed.

### 4.2 Cold and warm runs

- **Cold:** nothing from the medium in the Windows file cache, and Culla's disk cache empty.
  - For SD and USB media: safely eject and reinsert the card before each run. Removing the volume drops its cached pages. This is also the realistic case (a card just plugged in).
  - For the NVMe: empty the standby list with Sysinternals RAMMap (`RAMMap.exe -Et`, admin) before each run. A reboot is the fallback.
  - Culla's disk cache: `culla-bench` points the cache at a fresh temporary directory for each cold run, so there is nothing to clear and the app's own cache is never touched.
- **Warm:** the same run repeated right after, without ejecting. Reported, but pass/fail uses cold numbers unless the NFR says otherwise.
- **Cache hit (B-08):** run once to fill the disk cache, eject and reinsert the card, run again. This is the resume case.
- Scan and preview reads are buffered, as in the app. The apply verify step reads the destination back with `FILE_FLAG_NO_BUFFERING`, as [SPEC.md](SPEC.md) section 5.2 requires, so B-13 and B-14 measure it that way. The source re-read for hash 3 also uses `FILE_FLAG_NO_BUFFERING`, so it is a real second read of the card even on a warm run (otherwise the standby cache would answer and hash 3 would only repeat hash 1). B-13 and B-14 measure both verify reads that way.

### 4.3 Swipe cadence for B-09 and B-10

`culla-bench swipe` replays a decision sequence against the real prefetch scheduler:
- steady: one advance every 400 ms (a careful cull);
- fast: one advance every 200 ms (5 photos/s);
- burst: 10 advances at 100 ms, then a 2 s pause;
- ramp (B-10): start at 1 photo/s and step up by 0.5 photo/s every 50 photos until p95 exceeds the budget.

The sequence includes 10 % back-steps (Shift+←) to exercise the "2 behind" window.

### 4.4 Repetitions and statistics

- Criterion defaults for micro-benchmarks (B-04 to B-07); report the median and confidence interval.
- `culla-bench`: 5 cold runs per metric per medium; report the median of the 5 runs. For per-photo latencies, report p50, p95 and p99 over all photos of all runs.
- A result counts as a pass only if the median of the cold runs meets the target.

### 4.5 Tooling

- **criterion** in `crates/adapters/benches/` (parsers and decode live in `culla-adapters`) for CPU-bound pieces. Runs in CI on PRs that touch the pipeline, as a smoke run only (CI timings are not compared to targets).
- **`culla-bench`**: `crates/bench`, package `culla-bench` ([ADR 0020](adr/0020-headless-cli-and-bench-crates.md)), built on `culla-core` with `culla-adapters`. It takes a folder path, runs one scenario, and writes a JSON result (machine info, medium info, parameters, raw timings) to `bench-results/` (gitignored). A summary command turns the JSON into the result table below.

```
culla-bench scan    <folder> [--exif]
culla-bench preview <folder> [--source jpeg|nef] [--cache cold|hit]
culla-bench swipe   <folder> --cadence steady|fast|burst|ramp [--preview-source jpeg|nef] [--readers N] [--workers N] [--ahead N] [--behind N] [--lru N]
culla-bench apply   <src> <dst> --mode move|copy [--hash xxh3|blake3|none]
culla-bench io-baseline <folder>
culla-bench report  bench-results/*.json
```

`culla-bench apply` first copies the dataset into a temporary directory (`tempfile::TempDir`) on the source medium, and applies from there into another temporary directory on the destination medium. It never runs on the original card contents. The bench is outside the clippy removal rule (it cleans up its own temp dirs); that rule is enforced on `culla-core`, `culla-adapters`, `culla-cli` and `src-tauri` only ([ADR 0020](adr/0020-headless-cli-and-bench-crates.md)).

## 5. Result table template

One file per session: `docs/benchmarks/results/YYYY-MM-DD-<hardware>-<medium>.md`, with the raw JSON attached to the PR (not committed).

```markdown
# Benchmark results — <date> — <H-x> — <medium>

Commit: <sha> · Windows <build> · power mode <mode> · Defender <on/off> · medium <brand, model, FS> · reader <model, port>
Dataset: <D-x> (<n files>, <n pairs>, <MP>)

| ID | Metric | Cold median | Warm median | p95 | p99 | Target (NFR) | Result |
|---|---|---|---|---|---|---|---|
| B-01 | First photo | | | — | — | ≤ 1.5 s (est.) | pass / fail |
| B-02 + B-03 | Scan + EXIF, 3,000 files | | | — | — | ≤ 15 s (est.) | |
| B-09 | Next-photo latency, steady | | | | | p95 ≤ 50 ms (est.) | |
| B-09 | Next-photo latency, fast | | | | | p95 ≤ 50 ms (est.) | |
| B-10 | Sustained swipe rate | | — | — | — | ≥ 3/s on SD (est.), NFR-PERF-04 | |
| B-12 | Apply same volume, 1,000 groups, M2 | | — | — | — | ≤ 30 s (est.), NFR-PERF-10 | |
| B-13 | Apply cross volume (MB/s), M2 | | — | — | — | ≥ 85 % of device throughput (est.), NFR-PERF-11 | |
| B-14 | Verify overhead, M2 | | — | — | — | recorded | — |
| B-15 | Peak memory | | | — | — | ≤ 1 GB (est.) | |
| ... | | | | | | | |

Parameters chosen: readers <n>, workers <n>, prefetch <ahead>/<behind>, LRU <n>, preview source <jpeg|nef>, hash <xxh3|blake3>.

Findings and decisions: <one line each, with ADR link if a decision was made>
```

## 6. Pass, fail and what follows

- **Pass:** cold median meets the `NFR-PERF` target on H-2 (UHS-I SD). H-1 numbers must also pass but are not enough on their own.
- **Fail:** in order, try
  1. tuning (section 2 knobs);
  2. design changes already listed as options: NEF embedded preview as the preview source (B-11), smaller preview size, a larger prefetch window, thumbnails from the NEF's small PreviewImage;
  3. revising the budget in [nfr.md](nfr.md) with an ADR that records the measured numbers and why the new budget is still acceptable.
- Decisions taken from B-11 (preview source), B-14 (hash) and the tuning sweep (thread counts, prefetch, LRU size) are recorded as ADRs, and SPEC section 5.1 values are updated through them.
- B-13 results calibrate the time and size estimate on the review screen ([SPEC.md](SPEC.md) section 4.2).
- Re-run the suite on H-2 before v1.0 and after any change to the scan, preview or apply pipeline.
