---
status: proposed
date: 2026-11-27
---

# A lone NEF's preview is its largest valid embedded JPEG, chosen by pixel count

Refines [ADR 0011](0011-own-ifd-walker-for-nef-previews-and-kamadak-exif.md).

## Context and problem statement

ADR 0011 says the IFD walker "collects every `JPEGInterchangeFormat`/`Length` pair and picks the largest", without saying largest by what, what counts as valid, or whether one IFD should be preferred. A Z6III NEF embeds four JPEGs: 160x120 (IFD0), 6048x4032 (SubIFD0), 1620x1080 (SubIFD2) and 640x424 (MakerNote PreviewIFD) ([z6iii-nef-layout.md](../research/z6iii-nef-layout.md)). Its raw data appears to be HE/HE\* (fixed-size strips; the tags cannot tell) and Culla does not decode raw data, so the embedded JPEG is the only image for a lone NEF (US-03). Which candidate is the preview, and how is it checked?

## Considered options

* Largest valid candidate by pixel count (frame header), then by bytes; no fixed IFD preference
* Largest by byte length, as the research note put it
* A fixed source: SubIFD0 ("JpgFromRaw") first, then the MakerNote PreviewIFD, then IFD0

## Decision outcome

Chosen option: "Largest valid candidate by pixel count", because it picks the 6048x4032 JPEG on the Z6III without hard-coding Nikon's layout, and a corrupt or padded candidate cannot win on byte length.

* A candidate is valid when it lies inside the file, is at most 64 MiB, starts with SOI, ends with EOI (zero padding trimmed), and has an 8-bit baseline, extended or progressive frame header with a non-zero size. Lossless or 12-bit JPEG (raw data stored as JPEG) is not a preview.
* Ties on pixels go to the larger byte length, then to the first found in walk order (IFD0, its SubIFDs, the MakerNote, then IFD1).
* Sources walked: the IFD0 chain, SubIFDs (0x014A), and the Nikon type 3 MakerNote PreviewIFD; both 0x0201/0x0202 pairs and single JPEG strips (Compression 6/7).
* Paired groups are unchanged: their primary is the camera JPEG (SPEC §3.1). Whether the NEF's embedded JPEG would be a faster source for pairs (architecture risk table) is left to the M1 benchmark; on the Z6III it has the same pixel size as the camera JPEG.
* The smaller candidates (640x424, 1620x1080) are not used for screen previews; they may serve thumbnails later.
* A candidate declaring more than 100 MP in its frame header is rejected, so a small JPEG claiming a huge frame cannot win and reach the decoder (threat model T5). The decoder still re-checks the frame header of the bytes it gets.
* Placement: the walker is pure parsing logic, so it lives in `culla-core` (`culla_core::ifd`) behind a `ReadAt` port (length plus positioned reads), and `culla-adapters` feeds it from the file. This keeps it I/O-free and fuzzable on bytes, in line with ADR 0002, and departs from the roadmap's first sketch ("parsers in `culla-adapters`"). A port is used instead of a byte slice so a walk reads ~0.8 MB of a 20 MB NEF without loading it whole or using mmap (ADR 0012).

### Consequences

* Good, because the same rule works for any body that embeds a full-size JPEG, wherever it puts it.
* Good, because a truncated file falls back to the next largest preview that is still whole.
* Bad, because a body whose largest embedded JPEG is below screen size shows a soft preview; Culla does not upscale or decode the raw.
* Bad, because checking each candidate costs a few small reads (frame header and tail): about 5 positioned reads and 0.8 MB per Z6III NEF in total.

Source: [z6iii-nef-layout.md](../research/z6iii-nef-layout.md), [research-raw-perf.md §1](../research/research-raw-perf.md), `crates/core/src/ifd/`.
