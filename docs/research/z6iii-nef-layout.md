# Nikon Z6III NEF layout: embedded previews (Nov 2026)

Measured on 27 Nov 2026 with the IFD walker (`culla_core::ifd`) and a throwaway script, on 9 of the author's NEFs: 8 JPEG + NEF pairs and 1 lone NEF, Z6III, 24 MP, NIKKOR Z 24-200, firmware as shipped in Nov 2026. The files are not in the repository. Re-run with `CULLA_REAL_SAMPLES=<folder> cargo test -p culla-adapters --test real_samples -- --ignored --nocapture`.

## Structure

Every file had the same layout. The EXIF IFD (35,368) and the MakerNote preview (104,680) were at the same offset in all 9; the other offsets vary with the JPEG sizes.

| Where | What | Size | Offset in file | Bytes |
|---|---|---|---|---|
| IFD0, 0x0201/0x0202 | JPEG thumbnail, NewSubfileType 1, Compression 6 | 160x120 | ~225–265 KB | 8–13 KB |
| SubIFD0, 0x0201/0x0202 | Full-size JPEG ("JpgFromRaw"), NewSubfileType 1, Compression 6 | **6048x4032** | ~1.1–1.6 MB | 1.2–3.4 MB |
| SubIFD1 | Raw data, NewSubfileType 0, Compression 34713, 14 bits, one strip | 6064x4040 | ~2.4–4.7 MB | 15,311,360 in every file |
| SubIFD2, 0x0201/0x0202 | JPEG, NewSubfileType 1, Compression 6 | 1620x1080 | ~485–525 KB | 0.6–1.1 MB |
| SubIFD3 | Uncompressed 8-bit image (Compression 1) | 384x256 | ~240–280 KB | 98,304 |
| SubIFD4 | Small raw (Compression 34713, 12 bits) | 384x256 | ~335–380 KB | 147,456 |
| SubIFD5 | Empty (0x0, offsets 0) | – | – | 0 |
| MakerNote PreviewIFD (0x0011), 0x0201/0x0202 | JPEG, Compression 6 | 640x424 | 104,680 | 120–160 KB |

- Main TIFF: little-endian (`II`). The EXIF IFD sits at ~35 KB; the MakerNote follows at ~36 KB and is ~200 KB long.
- MakerNote: Nikon type 3, header `Nikon\0` + version `02 11 00 00`, embedded TIFF little-endian, 81 entries. Its PreviewIFD JPEG lies inside the MakerNote.
- All four JPEGs are baseline (SOF0), 8-bit, RGB once decoded, and end exactly at FF D9 (no padding); all four decode with PIL (checked on the lone NEF). The full-size JPEG has the same size as the camera's own JPEG (6048x4032); the raw is 16x8 px larger.
- No SubIFD uses a JPEG strip (Compression 6/7 with StripOffsets); every JPEG is reached through 0x0201/0x0202.
- The last IFD ends near 250 KB, so a 512 KiB head read covers all IFDs. A full walk, including the check of each JPEG (SOI, frame header, last bytes), took 5 positioned reads and ~0.8 MB in total (the 512 KiB head plus 64 KiB windows); 7 reads in one file.

## Raw compression

The NEFCompression tag (MakerNote 0x0093) is absent, and Compression is the generic 34713 (Nikon NEF compressed), so the standard tags do not say which codec is used. The raw strip is exactly 15,311,360 bytes in all 9 files, about 5.0 bits per pixel, across different scenes. A lossless codec gives sizes that vary with content, so these are fixed-rate files, consistent with High Efficiency (HE or HE\*); which of the two cannot be told from these tags. Either way no open-source decoder exists (SPEC §9), so the embedded JPEG is the only image Culla can show for these files.

## Consequences for Culla

- The largest valid candidate is always SubIFD0's 6048x4032 JPEG. It is the only one above the G14's 2880 px long edge; the next one (1620x1080) is too small for a screen preview.
- The MakerNote preview (640x424) and the 1620x1080 JPEG could serve thumbnails later. The EXIF pass reads ~128 KB, which reaches the MakerNote preview's start but not its end.
- These are one body and one lens. Z7/Z8/Z9 (45 MP) and lossless or lossy (non-HE) NEFs are still to measure (testing.md §9).
