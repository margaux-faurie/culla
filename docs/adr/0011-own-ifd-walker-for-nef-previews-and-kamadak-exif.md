---
status: accepted
date: 2026-11-24
---

# Own TIFF/IFD walker for NEF previews instead of rawler; kamadak-exif for EXIF

## Context and problem statement

Lone NEFs are shown through their embedded JPEG, and every photo needs EXIF (capture time, orientation, lens, exposure). The app is MIT and Rust links statically. Which libraries read NEF previews and EXIF?

## Considered options

* `rawler` (dnglab) for previews and metadata
* `nom-exif`
* `little_exif`
* `rexiv2`/exiv2, `exiftool-rs`, libraw bindings
* `quickraw`
* Own ~200-line TIFF/IFD walker for previews + `kamadak-exif` for EXIF

## Decision outcome

Chosen option: "Own IFD walker + `kamadak-exif`", because it keeps the binary's licensing clean and the code small.

* `kamadak-exif` 0.6.x is BSD-2-Clause, pure Rust, and reads both JPEG and NEF (NEF is TIFF-structured) ([docs.rs](https://docs.rs/crate/kamadak-exif/latest)). The lens comes from `LensModel`, falling back to `LensSpecification`; naming Nikon's MakerNote LensID needs ExifTool's GPL table.
* The walker goes through IFD0, the SubIFDs and the MakerNote PreviewIFD, collects every `JPEGInterchangeFormat`/`Length` pair and picks the largest. This also works for HE/HE\* NEFs, whose embedded previews are ordinary JPEGs.
* `rawler` 0.8.0 has the best Nikon coverage but is LGPL-2.1, which makes an MIT static binary murky, and it uses `memmap2` ([docs.rs](https://docs.rs/crate/rawler/latest)). It stays a reference only.
* `nom-exif` does not list NEF. `little_exif` is JPEG/PNG only. exiv2, exiftool-rs and libraw are GPL/LGPL or need a C++ build. `quickraw` has been dead since 2022.

### Consequences

* Good, because every dependency is permissive and pure Rust.
* Good, because the walker is small enough to fuzz well (`cargo-fuzz` targets with decode size limits).
* Bad, because we own a parser of untrusted binary input; fuzzing and bounds checks are mandatory.
* Bad, because there is no RAW decode: 100% zoom on a lone NEF, and on any HE/HE\* NEF, shows the embedded JPEG ([pixls.us](https://discuss.pixls.us/t/nikon-he-decoder/56955?page=3)).

Source: [research-raw-perf.md §1](../research/research-raw-perf.md), [practices.md §2 threat model (5)](../research/practices.md).
