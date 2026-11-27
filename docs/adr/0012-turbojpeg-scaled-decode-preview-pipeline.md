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

Source: [research-raw-perf.md §2, §4, §5](../research/research-raw-perf.md), [SPEC.md §5.1](../SPEC.md).
