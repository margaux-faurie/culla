> Historical input, written before the decisions were settled. Where it disagrees with `docs/SPEC.md` or an ADR, those win.

# Research: RAW previews, decode performance, serving, I/O, file ops (Nov 2026)

## 1. NEF previews + EXIF
**Recommendation:** use [kamadak-exif](https://docs.rs/crate/kamadak-exif/latest) 0.6.x (BSD-2-Clause, pure Rust, last release Nov 2024) for EXIF from both JPEG and NEF. NEF is TIFF-structured, so it reads the same way. For previews, write a small (~200 LOC) IFD walker of your own: go through IFD0 → SubIFDs, collect every `JPEGInterchangeFormat`/`Length` pair (plus the MakerNote PreviewIFD), and pick the largest. Keep [rawler](https://docs.rs/crate/rawler/latest) 0.8.0 (Aug 2026, which has `full_image()`/`preview_image()`/`thumbnail_image()` on its NEF decoder) as a reference or a fallback behind a feature flag.

Options compared:
- **rawler** (dnglab): actively maintained and has the best Nikon coverage. It is **LGPL-2.1**, though, and Rust links statically, so the MIT app's binary has to meet LGPL relinking terms. You can manage that because the source is open, but it makes licensing murkier. It also uses `memmap2` (see risks).
- **[nom-exif](https://docs.rs/crate/nom-exif/latest/source/README.md)** 3.8 (MIT, active, lazy/async). It supports TIFF but doesn't list NEF, so test it on your files first.
- **little_exif**: covers JPEG/PNG only. Not usable here.
- **rexiv2/exiv2** (GPL, C++ build on Windows), **[exiftool-rs](https://docs.rs/crate/exiftool-rs/0.7.5)** (GPL-3), **libraw bindings** (LGPL/CDDL, C build): all ruled out for MIT, or too much toolchain pain.
- **quickraw**: dead since 2022.

**HE/HE\* NEF:** no open-source decoder works. [The pixls.us effort stalled in May 2026](https://discuss.pixls.us/t/nikon-he-decoder/56955?page=3), and LibRaw/RawTherapee fail too. The embedded JPEGs are still ordinary JPEGs, so preview extraction works. Just never plan on decoding the raw data.

**Lens field:** Z bodies usually fill `LensModel` (0xA434). F-mount DSLRs often don't, and putting a name on Nikon's MakerNote LensID needs ExifTool's (GPL) table. Fall back to `LensSpecification` (e.g. "24-70mm f/2.8").

## 2. Decode + resize
**Recommendation:** use [turbojpeg](https://docs.rs/turbojpeg/) 1.x (MIT/Unlicense, libjpeg-turbo) with `set_scaling_factor` for DCT-domain downscaling, then [fast_image_resize](https://docs.rs/crate/fast_image_resize/latest) 5.x (MIT/Apache, AVX2) for the final fit, then re-encode to JPEG q85–90 (~0.5–1 MB).

The G14 screen is **2880×1800**:
- 24 MP (6048 px wide): decode at 1/2 → 3024 px.
- 45 MP (8256 px wide): 1/4 gives only 2064 px, too small. Use 3/8 or 1/2 instead.

Pure-Rust alternative: [zune-jpeg](https://docs.rs/crate/zune-jpeg/latest) 0.5.x (what `image` 0.25 uses). It is roughly as fast as libjpeg-turbo but has no scaled decode. `jpeg-decoder` does have `scale()`, but it is in maintenance mode and [~30–50% slower](https://github.com/google/decoder-benchmarks-for-rust). The turbojpeg crate builds libjpeg-turbo itself, which needs CMake (and NASM for SIMD) in CI.

Expected times on a Ryzen 8945HS, single thread. **These are estimates, so benchmark them:**

| Step | 24 MP | 45 MP |
|---|---|---|
| Full decode | ~100–180 ms | ~200–350 ms |
| Scaled decode | ~50–60% of full (Huffman decoding still dominates) | same |
| Resize | <15 ms ([fir Lanczos3: 13 ms vs `image` 190 ms](https://docs.rs/crate/fast_image_resize/5.2.2/source/README.md)) | <15 ms |
| Encode | ~15–25 ms | ~15–25 ms |

Run 2–4 decode workers in parallel on rayon.

## 3. Serving to the WebView2 webview
**Recommendation:** use a custom URI scheme via `register_asynchronous_uri_scheme_protocol`, e.g. `preview://<id>/<size>`. Serve the **pre-encoded screen-sized JPEG** from the in-memory LRU, with `Cache-Control: immutable`.

On Windows it is exposed as `http://preview.localhost`, so the CSP needs `img-src 'self' http://preview.localhost preview:`.

Don't use:
- the **asset protocol** for previews: it would make Chromium decode the 20 MB originals itself (180 MB RGBA for 45 MP), and its scope has to be widened at runtime for any folder the user picks ([scope docs](https://v2.tauri.app/security/asset-protocol/));
- IPC, which is string-serialized ([tauri#5690](https://github.com/tauri-apps/tauri/discussions/5690));
- raw RGBA, which is ~20 MB per frame through WebView2's request interception.

On the front end, call `await img.decode()` before the swipe animation brings in the next card. For 100% zoom only, serve the original JPEG or full-res tiles on demand.

## 4. SD/USB performance
**Storage is the bottleneck, not the CPU.**
- A fine 45 MP JPEG is 15–25 MB. At ~90 MB/s on a UHS-I card that is ~200–280 ms of I/O per photo, so pipelining tops out at about 3–4 photos/s. The G14 has a UHS-II microSD reader ([spec](https://rog.asus.com/us/laptops/rog-zephyrus/rog-zephyrus-g14-2024/spec)), but a full-size SD card goes through a USB reader.
- **Benchmark this:** the full-size JpgFromRaw in a Z-body NEF is usually lower quality and much smaller than the camera JPEG. It may be the faster preview source even for pairs.

Strategy:
- Use 1–2 sequential I/O threads, kept separate from the decode pool. High parallelism hurts SD cards and HDDs.
- Prefetch 4–6 photos ahead and 2 behind, in swipe order.
- Hold ~50 encoded previews in RAM.
- Keep a disk cache on the internal NVMe so resume is instant.
- Make review-grid thumbnails while generating the previews (no extra pass), or from the NEF's 640 px PreviewImage.

**Upfront scan:** read only the first ~128 KB of each JPEG (or of each lone NEF) for EXIF. Expect 3,000 files in about 5–15 s on SD. Show a progress bar, and start the swipe screen in filename order while the scan finishes. Don't re-sort under the user once they have started.

## 5. Windows file operations
- **Moves:** same volume → `std::fs::rename` (atomic, instant, also on exFAT). Across volumes, `rename` fails with `ERROR_NOT_SAME_DEVICE`. Then:
  1. copy to `name.partial`;
  2. call `sync_all`;
  3. verify;
  4. rename to the final name;
  5. delete the source.
- **Copies:** `std::fs::copy` uses `CopyFileExW`, which keeps mtime and attributes. Creation time gets reset; set it with `File::set_times`.
- **Verification:**
  - **Size check:** catches truncation only.
  - **Hash check:** hash the source while you copy, using blake3 or xxh3. Re-reading the destination may come from the OS cache, so use `FILE_FLAG_NO_BUFFERING` or accept that.
  - **mtime check:** FAT allows ±2 s.
- **Long paths:** Rust std adds `\\?\` internally. Still test deep mirrored destinations.
- **Locks:** Explorer thumbnailing and Defender briefly hold handles (`ERROR_SHARING_VIOLATION`/32), so retry with backoff (e.g. 5× from 100 ms). **Release your own handles and mmaps first:** a memory-mapped file can't be moved or deleted on Windows, and pulling out a card under an mmap crashes with `EXCEPTION_IN_PAGE_ERROR`. Read into buffers, never mmap removable media.
- **Journal:** use SQLite (`rusqlite` bundled, WAL, `synchronous=FULL`) in the same DB as the session. One row per op, with state `planned → copied → verified → committed → src_deleted`. On restart, **roll forward**. Rollback after a cross-volume source delete means copying everything back.

## Things in the spec that are wrong or naive
1. **Cache/session keyed by path:** an SD card's drive letter changes between plug-ins. Key by volume serial + relative path + size + mtime.
2. **`_rejected` inside the source:** exclude it from the recursive scan, and decide whether it sits at the root (mirrored) or per subfolder.
3. **Cross-volume "keep = move":** 2,000 pairs × ~70 MB ≈ 140 GB, about 25+ minutes at 90 MB/s. The review screen should show bytes and ETA, and check free space for moves as well as copies.
4. **Sidecars:** NX Studio stores edits in `NKSC_PARAM\*.nksc` next to the NEF, not in `.xmp` files ([source](https://www.fastrawviewer.com/node/1005)). Mirror that folder too.
5. **Lightroom:** files moved outside LR show up as "missing" in its catalog. Put a warning in the README.
6. **Lone HE NEF:** 100% zoom depends entirely on the embedded JPEG being full-size. There is no raw fallback.
7. **Read-only media:** check the SD write-protect switch and the read-only state before apply, not during it.
8. **Rollback:** promise resume (roll forward), not rollback.
