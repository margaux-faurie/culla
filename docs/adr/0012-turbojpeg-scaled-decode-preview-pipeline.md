---
status: accepted
date: 2026-11-24
---

# Preview pipeline: turbojpeg scaled decode + fast_image_resize; no mmap on removable media

## Context and problem statement

A 45 MP JPEG is 15–25 MB and takes ~200–350 ms to decode in full. The screen is 2880×1800. Reading from a UHS-I card runs at ~90 MB/s, so storage is the bottleneck. How are screen-sized previews produced fast enough for swiping?

## Considered options

* Let the webview decode the originals
* `image` crate (zune-jpeg) full decode + resize
* `jpeg-decoder` with `scale()`
* `turbojpeg` scaled (DCT-domain) decode + `fast_image_resize` + JPEG encode

## Decision outcome

Chosen option: "`turbojpeg` scaled decode + `fast_image_resize` + JPEG encode", because DCT scaling cuts decode time and memory, and the result is a small JPEG the webview decodes cheaply.

* [turbojpeg](https://docs.rs/turbojpeg/) 1.x (MIT/Unlicense) with `set_scaling_factor`: 1/2 for 24 MP, 3/8 or 1/2 for 45 MP (1/4 is too small for the screen). Then [fast_image_resize](https://docs.rs/crate/fast_image_resize/latest) 5.x for the final fit (13 ms against 190 ms for `image`). Then encode at q85–90. EXIF orientation is applied here.
* zune-jpeg is about as fast but has no scaled decode. `jpeg-decoder` is in maintenance mode and ~30–50% slower.
* I/O: 1–2 sequential reader threads, separate from the decode workers (proposed addition, to confirm: 2–4 workers). Prefetch 5 ahead and 2 behind. Memory LRU of about 12 screen previews. Disk cache on the internal disk, keyed by volume serial + path + size + mtime.
* Files on removable media are read into buffers, never memory-mapped: a mapped file cannot be moved on Windows, and a pulled card under a mapping crashes with `EXCEPTION_IN_PAGE_ERROR`.

### Consequences

* Good, because previews are about 0.5–1 MB and the swipe never waits on a full decode.
* Good, because the disk cache makes a resumed session instant.
* Bad, because `turbojpeg` builds libjpeg-turbo, which needs CMake (and NASM for SIMD) in CI.
* Bad, because all timings are estimates until the M1 benchmarks on a real card. The NEF's embedded JpgFromRaw may be a faster source even for pairs; to measure. The research suggested a larger LRU (~50); SPEC chose ~12, to revisit after benchmarks.

## Implementation notes

M1 slice 5 (27 Nov 2026), the decode step only; reader threads, decode pool, prefetch, LRU and disk cache come in the next slice. No decision changes.

* **Sizes** (`culla_core::preview`, pure). The preview fits a box given as long × short edge (`PreviewBox::G14`, 2880×1800) in the photo's own orientation and is never upscaled: a 3:2 photo becomes 2700×1800, or 1800×2700 for a portrait, so it fills the G14's screen height. The DCT scale is the smallest `M/8` (passed reduced, as libjpeg-turbo matches factors exactly) whose output covers that size on both sides, so the resize only ever shrinks: 1/2 for 24 MP (3024×2016) and 3/8 for 45 MP (3096×2064). A JPEG already inside the box is decoded whole and not resized. US-07 says "long edge just above the screen size"; reading it as "fills the box" is to be confirmed by Margaux.
* **Guard before the decoder** (threat model T5). Culla's own frame-header parser (`ifd::check_jpeg`, the one the NEF walker uses) checks the bytes first: at most 64 MiB, SOI, EOI (so a truncated file is refused), an 8-bit baseline, extended or progressive frame, and at most 100 MP declared. turbojpeg's header must then report the same size. No pixel buffer is allocated before both checks, and buffers are reserved with `try_reserve_exact`.
* **Pipeline** (`culla_adapters::preview::build_preview`): RGB decode with `set_scaling_factor`, `fast_image_resize` Lanczos3 to the planned size, orientation applied after the resize (`culla_core::preview::orient`, all eight EXIF values), JPEG at q88 with 4:2:0. The orientation comes from the EXIF reader; the output carries no EXIF and no ICC profile. One call holds the input, a decode buffer (about 6 MP, 18 MB for 24 and 45 MP sources on the G14), the resized and turned buffers (5 MP, 15 MB each) and the output; a progressive JPEG also makes libjpeg-turbo hold the whole image's coefficients (Nikon bodies write baseline).
* **Isolation.** Each call creates its own turbojpeg handles and runs under `catch_unwind`, so a panic fails that preview only (`PreviewError::Panicked`). libjpeg-turbo reports its errors as return codes, which come back as `PreviewError::Decode`; a memory-safety bug inside the C library cannot be caught in-process.
* **Crates.** `turbojpeg` 1.x (MIT OR Unlicense) with `default-features = false, features = ["cmake"]` (no `image` integration), and `fast_image_resize` 5.x (MIT OR Apache-2.0) with `default-features = false`. `turbojpeg-sys` builds the bundled libjpeg-turbo (IJG, BSD-3-Clause and Zlib) with CMake; NASM is used for SIMD when it is on `PATH`, and without it the library builds as plain C, which decodes several times slower. Release and benchmark builds need NASM; CI needs CMake (on both GitHub runners) and should install NASM so the same code is tested. libjpeg-turbo's licence asks binary distributions to credit the Independent JPEG Group and reproduce its BSD notice; the installer's third-party notices must include them.
* **Measured.** Timings come from B-06 and B-07 on the G14 ([benchmarks.md](../benchmarks.md)); the real-sample test prints per-step timings but runs wherever the samples are.

Source: [research-raw-perf.md §2, §4, §5](../research/research-raw-perf.md), [SPEC.md §5.1](../SPEC.md).
