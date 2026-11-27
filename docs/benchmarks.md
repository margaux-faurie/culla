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
| B-10 | Sustained swipe rate | Highest cadence where p95 of B-09 stays within budget | `culla-bench swipe --cadence ramp` | 3–4 photos/s on UHS-I for 45 MP fine JPEGs | ≥ 3/s on SD UHS-I for 45 MP (est.), NFR-PERF-04 | M1 |
| B-11 | Preview source choice | B-09 using the camera JPEG versus the NEF's full-size embedded JPEG (JpgFromRaw) for pairs | `culla-bench swipe --preview-source jpeg\|nef` | NEF preview may be faster | decision input | M1 |
| B-12 | Apply, same volume | Time for 1,000 groups (about 3,000 files), journal included, on SD and NVMe | `culla-bench apply --same-volume` | near instant | ≤ 30 s for 1,000 groups (est.), NFR-PERF-10 | M2 |
| B-13 | Apply, cross volume | MB/s and total time for copy (hashing the source) + fsync + destination read-back and source re-read, both with `FILE_FLAG_NO_BUFFERING`, + rename + source removal ([SPEC.md](SPEC.md) section 5.2), SD → NVMe, and NVMe → external | `culla-bench apply --cross-volume` | about 90 MB/s on UHS-I; 140 GB in 25+ min for a single read (research), about 52+ min with the second source read | ≥ 85 % of the slower device's sequential throughput over the bytes read (the source is read twice) (est.), NFR-PERF-11; also checks the review-screen ETA (NFR-PERF-12) | M2 |
| B-14 | Verify overhead | B-13 with xxh3 versus blake3 versus no verify (bench only); the source re-read costs a second read of the card, so report it separately | `culla-bench apply` | — | recorded, no pass/fail; decision input for the hash choice | M2 |
| B-15 | Memory | Peak working set during B-09 and B-13 | `culla-bench` (reports the process's peak working set on Windows, `VmHWM` on Linux) | ~12 previews in LRU | ≤ 1 GB (est.), `NFR-PERF` | M1 (swipe), M2 (apply) |
| B-16 | Medium baseline | Raw sequential read speed of the medium: buffered 1 MiB reads of the photo files already on it, up to 1000 MB (no test file is written) | `culla-bench io-baseline` | about 90 MB/s UHS-I | reference for B-03, B-10, B-13 | M1 |

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
| D-3 | 24 MP (Z6 class) and 45 MP (Z7, Z8, Z9 class) sets (about 200 photos each) | B-06, B-07, B-09 to B-11 per resolution |

D-2 and D-3 come from the author's Nikon Z bodies ([SPEC.md](SPEC.md) section 2): the 24 MP set is Z6 class, the 45 MP set Z7, Z8 or Z9 class; include HE/HE\* NEFs where the body writes them. Her sample files are still pending. If only one resolution is available from her bodies, the other one can come from public sample files (for example raw.pixls.us), used locally and never committed; check each file's licence. Benchmark data is never committed.

### 4.2 Cold and warm runs

- **Cold:** nothing from the medium in the Windows file cache, and Culla's disk cache empty.
  - For SD and USB media: safely eject and reinsert the card before each run. Removing the volume drops its cached pages. This is also the realistic case (a card just plugged in).
  - For the NVMe: empty the standby list with Sysinternals RAMMap (`RAMMap.exe -Et`, admin) before each run. A reboot is the fallback.
  - Culla's disk cache: there is none in M1 (the pipeline keeps previews in memory only), so there is nothing to clear. Once it exists, `culla-bench` will point it at a fresh temporary directory for each cold run, so the app's own cache is never touched.
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

- **criterion** in `crates/adapters/benches/` (parsers and decode live in `culla-adapters`) for CPU-bound pieces. Not added yet: criterion would be a new dependency, so it is a proposal (SPEC §10). Until then, `culla-bench preview` reports the decode, resize, orient and encode steps per photo (B-06, B-07), measured from the medium rather than in isolation.
- **`culla-bench`**: `crates/bench`, package `culla-bench` ([ADR 0020](adr/0020-headless-cli-and-bench-crates.md)), built on `culla-core` with `culla-adapters`, std only. It takes a folder, runs one scenario, and prints the result to stdout: text by default, or with `--json` one JSON object (`"schema": "unstable"`: build profile, machine, medium, parameters, metrics, per-item samples, notes). It is read-only on the folder and writes nothing else: no file, no cache, no `bench-results/` folder. A result is kept by redirecting stdout to a file outside the card (section 7); `bench-results/` is gitignored for that use. `culla-bench report` turns saved results into the medians and pooled quantiles of the result table below.

In M1 (`culla-bench help` lists every option):

```
culla-bench scan        <folder> [--exif] [--no-first-preview]
culla-bench preview     <folder> [--source jpeg|nef] [--limit N]
culla-bench swipe       <folder> [--cadence steady|fast|burst|ramp|max] [--preview-source jpeg|nef]
                        [--readers N] [--workers N] [--ahead N] [--behind N] [--lru N] [--lru-mb N]
                        [--steps N] [--back-percent P] [--seed N] [--budget-ms N]
culla-bench io-baseline <folder> [--mb N]
culla-bench report      <result.json>...
```

Every folder benchmark also takes `--no-subfolders`, `--label TEXT` (kept in the result: medium, run number, cold or warm, commit) and `--json`. Exit codes as `culla`: 0 done, 1 usage error, 2 folder refused, 3 folder or file unreadable. `--lru-mb` is at most 65536. `report` refuses a file over 256 MiB and skips an empty one with a note on stderr.

Peak memory on Windows goes through a child PowerShell for now, since the bench has no `windows-sys` dependency. The proposed long-term route is a `GetProcessMemoryInfo` lookup in `culla-adapters::winfs`, which is a protected path, so it waits for a session of Margaux's (SPEC §10).

What each one measures:

| Command | Benchmarks | Metrics (JSON key) |
|---|---|---|
| `scan` | B-01, B-02, B-03 with `--exif`; NFR-PERF-01, NFR-PERF-08 | `resolve_ms` (the folder's canonical path and volume lookups, before the walk), `first_group_ms` (walk start to the first group, B-01), `first_photo_ms` (NFR-PERF-01: `resolve_ms` plus `first_group_ms` plus that group's preview, made inside the walk, upright, from the camera JPEG), `listing_ms` (walk and grouping, the first preview left out), `exif_ms`, `sort_ms`, `scan_total_ms`, the first photo's steps, counts |
| `preview` | B-05, B-08 (miss), B-06 and B-07 per photo; NFR-PERF-05 | per photo, in walk order, outside the pipeline: `exif_ms` (read just before, for the orientation), `nef_walk_ms` and `nef_walk_read_kb` (NEF sources), `read_ms`, `check_ms`, `decode_ms`, `resize_ms`, `orient_ms`, `encode_ms`, `miss_total_ms`; `read_mb_per_s` |
| `swipe` | B-09, B-10 (`ramp`, and `max` for NFR-PERF-04), B-11 (`--preview-source`), B-15 | `next_photo_ms` per advance, `prefetched` (share of advances already in the cache), `advances_per_s`, `sustained_rate_per_s` (ramp), `first_preview_ms` (the pipeline's first preview after the scan and EXIF pass: not B-01), `failed` and `not_admitted_advances` (advances whose photo was not shown), the pipeline's peaks and counters, `advance_late_ms` (how late the bench made each advance: the timer's error, should stay near 0) |
| `io-baseline` | B-16 | `read_mb_per_s`: buffered 1 MiB reads of the photo files already on the medium (primaries and NEFs, scan order) until `--mb` MB; no test file is written |
| every command | B-15 | `peak_memory_bytes`: the process's peak working set (Windows, from Windows PowerShell's `Get-Process` at the end of the run, started by its absolute path under `%SystemRoot%`; unavailable, with a note, when `SystemRoot` is unset or that file is missing) or peak resident set (`VmHWM`, Linux) |

`swipe` runs the scan and the EXIF pass first, then replays the sequence in capture-time order against the real pipeline (`culla_adapters::pipeline`). Latency is from the advance to the preview being in the in-memory cache. The next advance comes the cadence's gap after the previous one but never before the current photo is ready, since a user cannot decide on a photo they have not seen. The first photo is reported apart (`first_preview_ms`), not as a next-photo latency. `--steps` caps the advances; by default the sequence runs once to the last photo, so a ramp on a small folder can end before its p95 goes over budget (the result says so).

- Next-photo latencies (`next_photo_ms`, and so every p95 of B-09 and of the ramp) cover the advances whose photo was shown only. An advance whose photo failed, timed out (120 s) or was refused by the cache (`not_admitted_advances`: the pipeline does not make it again until the window moves, so the bench does not wait for it) has no latency; it is counted, and a ramp block with any such advance fails whatever its p95.
- The swipe starts after the scan and the full EXIF pass, so the prefetch never competes with the EXIF reads. In the app the user can start swiping while EXIF is still being read; on an SD card the first seconds of B-09 and B-10 here are therefore optimistic. The M3 webview measurement covers the real overlap.
- First photo (NFR-PERF-01): the figure is `scan`'s `first_photo_ms`, which starts before the folder is resolved (`resolve_ms`, reported apart) and so covers what the app does from "open folder" to the first preview. `first_group_ms` (B-01) still starts at the walk.

Not in M1: `--cache hit` for B-08 (the disk cache does not exist yet), `apply` (B-12 to B-14, M2), and B-04 (criterion).

`culla-bench apply` (M2) will first copy the dataset into a temporary directory on the source medium, and apply from there into another temporary directory on the destination medium. It never runs on the original card contents. The bench is outside the clippy removal rule so it can clean up its own temp dirs ([ADR 0020](adr/0020-headless-cli-and-bench-crates.md)); in M1 it writes nothing, so the lint stays on in the crate.

## 5. Result table template

One file per session: `docs/benchmarks/results/YYYY-MM-DD-<hardware>-<medium>.md`, with the raw JSON attached to the PR (not committed). The JSON holds no folder path or file name, only the medium's mount point, label and file system; check the labels you typed before attaching it.

```markdown
# Benchmark results — <date> — <H-x> — <medium>

Commit: <short sha> · NASM <`nasm -v` at build time: libjpeg-turbo built with SIMD> · Windows <build> · power mode <mode> · Defender <on/off> · medium <brand, model, FS> · reader <model, port>
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

## 7. Running the campaign on Windows

For Margaux, on the G14. All defaults below (runs per metric, which cadences, the folders) are proposals (SPEC §10).

### 7.1 Build once per commit

In PowerShell 7 (`pwsh`), at the repository root:

```powershell
winget install Kitware.CMake NASM.NASM     # once; open a new terminal afterwards
nasm -v                                    # must print a version: without NASM, libjpeg-turbo builds as plain C, several times slower
git switch main; git pull
cargo build --release -p culla-bench       # if NASM was installed after an earlier build: cargo clean -p turbojpeg-sys first
$bench = "$PWD\target\release\culla-bench.exe"
& $bench --version
$commit = git rev-parse --short HEAD       # the commit: goes into every label and the result file
$nasm = nasm -v                            # for the result file's NASM line (SIMD on)
```

A debug build (`cargo run` without `--release`) prints a warning and its timings mean nothing; `report` flags it.

### 7.2 Set up the session

```powershell
$card = "E:\DCIM"                                    # the card's photo folder (read only)
$out  = "$HOME\culla-bench-results\2026-12-01-H2-sd"  # anywhere off the card; one folder per session
New-Item -ItemType Directory -Path $out | Out-Null

# A failed run's file is renamed to .failed (never deleted), so a rerun can write the run again
function Save-Run([string]$file, [string[]]$benchArgs, [string]$label) {
  & $bench @benchArgs --json --label "$label $commit" | Out-File -Encoding utf8 -NoClobber $file
  if ($LASTEXITCODE -ne 0) {
    $kept = "$file.$(Get-Date -Format yyyyMMdd-HHmmss).failed"
    Rename-Item -LiteralPath $file -NewName (Split-Path $kept -Leaf)
    Write-Warning "exit code $LASTEXITCODE, kept as $kept; run the same line again to redo it"
  }
}

function Run-Cold([string]$name, [string[]]$benchArgs, [int]$runs = 5) {
  for ($i = 1; $i -le $runs; $i++) {
    $file = "$out\$name-cold-$i.json"
    if (Test-Path -LiteralPath $file) { continue }  # done in an earlier attempt
    Read-Host "Safely eject the card, reinsert it, then press Enter ($name, cold run $i of $runs)"
    Save-Run $file $benchArgs "H-2 $name cold $i"
  }
}

function Run-Warm([string]$name, [string[]]$benchArgs) {
  $file = "$out\$name-warm.json"
  if (-not (Test-Path -LiteralPath $file)) { Save-Run $file $benchArgs "H-2 $name warm" }
}
```

`-NoClobber` refuses to replace an existing result, and a run whose file already exists is skipped, so after a failure (or an interrupted session) running the same line again does only the missing runs. A failed run (exit code other than 0) keeps its file, renamed with a time stamp and `.failed`, so `report` (which reads `*.json`) never sees it; `report` also skips an empty file with a note. Every label ends with the commit (`$commit`), and the session file records the NASM version (`$nasm`), so a result says which code and which libjpeg-turbo build produced it. Write results with `Out-File -Encoding utf8` (or `>` in PowerShell 7 only): `>` in Windows PowerShell 5.1 writes UTF-16, which `report` refuses. Progress goes to stderr and stays on screen.

Before the first run: plugged in, ASUS power mode fixed (for example Performance), Defender left on, nothing else heavy running. Note the card (brand, size, speed class), the reader (model, USB port) and the Windows build (`winver`) for the result file.

### 7.3 Runs on the SD card (H-2)

Each cold run needs the card ejected and reinserted first (section 4.2), so `Run-Cold` asks for it every time. A warm run follows a cold one without ejecting.

```powershell
Run-Cold io-baseline @('io-baseline', $card)                              # B-16, first: the medium's reference
Run-Cold scan        @('scan', $card, '--exif')                           # B-01, B-02, B-03
Run-Warm scan        @('scan', $card, '--exif')
Run-Cold preview-jpeg @('preview', $card, '--source', 'jpeg', '--limit', '300') 3   # B-05, B-08 miss
Run-Cold preview-nef  @('preview', $card, '--source', 'nef', '--limit', '300') 3    # B-05 (NEF), B-11 input
Run-Cold swipe-steady @('swipe', $card, '--cadence', 'steady', '--steps', '500')    # B-09, B-15
Run-Cold swipe-fast   @('swipe', $card, '--cadence', 'fast', '--steps', '500')      # B-09
Run-Cold swipe-burst  @('swipe', $card, '--cadence', 'burst', '--steps', '500') 3   # B-09
Run-Cold swipe-ramp   @('swipe', $card, '--cadence', 'ramp') 3                      # B-10
Run-Cold swipe-max    @('swipe', $card, '--cadence', 'max', '--back-percent', '0', '--steps', '300') 3   # NFR-PERF-04
Run-Cold swipe-nef    @('swipe', $card, '--cadence', 'fast', '--preview-source', 'nef', '--steps', '500') 3   # B-11
```

For B-05 per NEF variant (lossless, HE, HE\*), run `preview --source nef` on a folder holding one variant only, with the variant in the label.

Tuning sweep (section 2), on the cadence that failed or came closest, one knob at a time, 3 cold runs each:

```powershell
Run-Cold swipe-fast-r2  @('swipe', $card, '--cadence', 'fast', '--steps', '500', '--readers', '2') 3
Run-Cold swipe-fast-w2  @('swipe', $card, '--cadence', 'fast', '--steps', '500', '--workers', '2') 3
Run-Cold swipe-fast-a3  @('swipe', $card, '--cadence', 'fast', '--steps', '500', '--ahead', '3') 3
Run-Cold swipe-fast-a6  @('swipe', $card, '--cadence', 'fast', '--steps', '500', '--ahead', '6') 3
Run-Cold swipe-fast-l24 @('swipe', $card, '--cadence', 'fast', '--steps', '500', '--lru', '24', '--lru-mb', '96') 3
Run-Cold swipe-fast-l50 @('swipe', $card, '--cadence', 'fast', '--steps', '500', '--lru', '50', '--lru-mb', '192') 3
```

### 7.4 Runs on the NVMe (H-1)

Copy the card's folder to the internal drive first (for example `C:\bench\D-2`), then set `$card = "C:\bench\D-2"` and a new `$out`. For cold runs, empty the standby list before each run instead of ejecting: in an admin terminal, `RAMMap.exe -Et` (Sysinternals). Run the same list as 7.3, answering the prompt once RAMMap is done.

### 7.5 Summarise

```powershell
& $bench report (Get-ChildItem "$out\*-cold-*.json").FullName | Out-File -Encoding utf8 -NoClobber "$out\report-cold.txt"
& $bench report (Get-ChildItem "$out\*-warm.json").FullName   | Out-File -Encoding utf8 -NoClobber "$out\report-warm.txt"
```

`report` groups runs with the same benchmark, parameters, medium (drive kind and file system), build profile and culla version, and prints for each metric the median, min and max across runs (a result passes on the median of the cold runs, section 4.4) and for each per-item sample the p50, p95 and p99 pooled over the runs. Runs of a debug build are listed and flagged but left out of the figures. Cold and warm runs are summarised apart, since `report` groups by parameters and not by label. Copy the numbers into `docs/benchmarks/results/<date>-<hardware>-<medium>.md` (section 5) and attach the JSON files to the PR.

Cross-check B-15 once with Task Manager (Details, "Peak working set" column) during a swipe run.
