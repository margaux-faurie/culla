//! Synthetic test files (docs/testing.md §9), written by our own minimal writers, never by the
//! parsers under test, so a parser bug cannot hide in the fixtures.
//!
//! Output is a pure function of this code: same bytes on every run and every OS.
//!
//! Format limits:
//! - JPEG: flat grey greyscale baseline images of a chosen size (2 bits per 8x8 block). The
//!   paired JPEG is 8x8 with an APP1 Exif block holding IFD0 with Orientation = 1 only, no Exif
//!   IFD (the EXIF variants are under `exif/`); no JFIF APP0, no colour, no thumbnail.
//! - NEF (`pair/` big-endian, `lone/` little-endian): IFD0 with baseline tags (ImageWidth/Length,
//!   BitsPerSample, Compression 6, Photometric) pointing at an 8x8 thumbnail, Make and Model;
//!   SubIFD0 with a 48x32 JPEG (JPEGInterchangeFormat, the largest, as on a Z6III), SubIFD1 with
//!   a 24x16 JPEG strip, SubIFD2 with stand-in raw data (Compression 34713, not a JPEG); an Exif
//!   IFD with a Nikon type 3 MakerNote in the other byte order whose PreviewIFD (0x0011) points
//!   at a 32x24 JPEG inside the MakerNote. Not decodable as RAW.
//! - `broken/`: `ifd-loop.NEF` (IFD0 and a SubIFD point back at themselves and each other),
//!   `truncated.NEF` (the pair NEF cut in the middle of its full-size JPEG).
//! - `exif/`: EXIF variants for the EXIF slice (US-06, US-07). `DSC_0101.JPG` (little-endian)
//!   has IFD0 Orientation 6 and Model, and an Exif IFD with DateTimeOriginal, SubSecTimeOriginal,
//!   OffsetTimeOriginal, DateTimeDigitized, LensModel, LensSpecification, ExposureTime, FNumber,
//!   PhotographicSensitivity, FocalLength and PixelX/YDimension. `DSC_0102.JPG` (big-endian)
//!   has Orientation 8, no DateTimeOriginal (DateTimeDigitized with its sub-seconds and offset
//!   instead) and no LensModel (LensSpecification only). `DSC_0103.JPG` has no APP1.
//!   `DSC_0104.JPG` has Orientation 9, an all-zero DateTimeOriginal and an unreadable
//!   DateTimeDigitized. `DSC_0105.NEF` is a lone little-endian NEF like `lone/` with
//!   Orientation 8 and an Exif IFD holding the times and lens, whose MakerNote is stored at the
//!   end of the file (as if past the EXIF read cap). Values are those of a Z6III.
//! - The pair shares a basename in one folder (ADR 0007), so it doubles as a pairing case.
//!
//! Writing: the target files must not exist. xtask follows the workspace rule (no truncate, no
//! overwrite, no removal), so it cannot refresh files in place. To regenerate, delete
//! `testdata/synthetic/` by hand and run `fixtures` again. CI does not regenerate: it runs
//! `fixtures --check`, which builds the files in memory and compares them byte for byte with the
//! committed ones, so drift fails without writing anything.

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Folder under the repository root, as in docs/testing.md §9.
pub(crate) const DIR: &str = "testdata/synthetic";

/// Every fixture as (path relative to `DIR`, bytes), in a fixed order.
pub(crate) fn all() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("pair/DSC_0001.JPG", jpeg_with_exif()),
        ("pair/DSC_0001.NEF", nef(true)),
        ("lone/DSC_0002.NEF", nef(false)),
        ("broken/ifd-loop.NEF", nef_ifd_loop()),
        ("broken/truncated.NEF", nef_truncated()),
        ("exif/DSC_0101.JPG", jpeg_exif_full()),
        ("exif/DSC_0102.JPG", jpeg_exif_create_date()),
        ("exif/DSC_0103.JPG", jpeg(8, 8, None)),
        ("exif/DSC_0104.JPG", jpeg_exif_bad()),
        ("exif/DSC_0105.NEF", nef_with_exif()),
    ]
}

pub(crate) fn write(root: &Path) -> Result<(), String> {
    let dir = root.join(DIR);
    let files = all();
    // Refuse before writing anything, so a run never leaves a half-refreshed folder.
    let existing: Vec<PathBuf> = files
        .iter()
        .map(|(rel, _)| dir.join(rel))
        .filter(|p| p.exists())
        .collect();
    if !existing.is_empty() {
        return Err(format!(
            "{} file(s) already exist, first {}. xtask never overwrites or removes files: \
             delete {DIR}/ by hand to regenerate, or run `fixtures --check` to compare",
            existing.len(),
            existing[0].display()
        ));
    }
    for (rel, bytes) in &files {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| format!("cannot create {}: {e}", path.display()))?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        println!("wrote {DIR}/{rel} ({} bytes)", bytes.len());
    }
    Ok(())
}

pub(crate) fn check(root: &Path) -> Result<(), String> {
    let dir = root.join(DIR);
    let files = all();
    let mut problems = Vec::new();
    for (rel, expected) in &files {
        match fs::read(dir.join(rel)) {
            Ok(actual) if actual == *expected => {}
            Ok(_) => problems.push(format!("{DIR}/{rel} differs from the generator output")),
            Err(e) => problems.push(format!("{DIR}/{rel}: {e}")),
        }
    }
    let known: BTreeSet<String> = files.iter().map(|(rel, _)| (*rel).to_owned()).collect();
    for rel in list_files(&dir, &dir) {
        if !known.contains(&rel) {
            problems.push(format!(
                "{DIR}/{rel} is not produced by the generator (stale or hand-made)"
            ));
        }
    }
    if problems.is_empty() {
        println!("{DIR}: {} fixture(s) match the generator", files.len());
        Ok(())
    } else {
        Err(format!(
            "fixtures out of date:\n  {}\nDelete {DIR}/ by hand and run `fixtures` to regenerate",
            problems.join("\n  ")
        ))
    }
}

/// Relative paths with `/` separators of every file under `dir`; empty if `dir` is missing.
fn list_files(base: &Path, dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // file_type() does not follow symlinks, so a link loop can't recurse forever.
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            out.extend(list_files(base, &path));
        } else if let Ok(rel) = path.strip_prefix(base) {
            let parts: Vec<String> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            out.push(parts.join("/"));
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// JPEG
// ---------------------------------------------------------------------------------------------

fn segment(out: &mut Vec<u8>, marker: u8, payload: &[u8]) {
    let len = u16::try_from(payload.len() + 2).expect("JPEG segment under 64 KiB");
    out.extend_from_slice(&[0xFF, marker]);
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(payload);
}

/// A `width` x `height` greyscale baseline JPEG, flat grey. `app1` is inserted right after
/// SOI when given.
fn jpeg(width: u16, height: u16, app1: Option<&[u8]>) -> Vec<u8> {
    let mut out = vec![0xFF, 0xD8]; // SOI
    if let Some(app1) = app1 {
        segment(&mut out, 0xE1, app1);
    }
    // DQT: table 0, 8-bit, all ones.
    let mut dqt = vec![0x00];
    dqt.extend_from_slice(&[1; 64]);
    segment(&mut out, 0xDB, &dqt);
    // SOF0: 8-bit, height, width, one component (id 1, 1x1 sampling, table 0).
    let mut sof = vec![0x08];
    sof.extend_from_slice(&height.to_be_bytes());
    sof.extend_from_slice(&width.to_be_bytes());
    sof.extend_from_slice(&[0x01, 0x01, 0x11, 0x00]);
    segment(&mut out, 0xC0, &sof);
    // DHT: one code of length 1 ("0") per table. DC: symbol 0 (difference 0). AC: symbol 0 (EOB).
    for class_and_id in [0x00, 0x10] {
        let mut dht = vec![class_and_id, 1];
        dht.extend_from_slice(&[0; 15]);
        dht.push(0x00);
        segment(&mut out, 0xC4, &dht);
    }
    // SOS: one component, tables 0/0, spectral 0..63, no approximation.
    segment(&mut out, 0xDA, &[0x01, 0x01, 0x00, 0x00, 0x3F, 0x00]);
    // Entropy data: per 8x8 block DC "0" + EOB "0", so 2 zero bits; padded with ones. Zero
    // bytes and a final byte below 0xFF need no byte stuffing.
    let blocks = usize::from(width.div_ceil(8)) * usize::from(height.div_ceil(8));
    let bits = 2 * blocks;
    out.extend(std::iter::repeat_n(0u8, bits / 8));
    if bits % 8 != 0 {
        out.push(0xFF >> (bits % 8));
    }
    out.extend_from_slice(&[0xFF, 0xD9]); // EOI
    out
}

fn jpeg_with_exif() -> Vec<u8> {
    // Little-endian TIFF inside Exif, IFD0 at offset 8 with Orientation = 1 (top-left).
    let mut app1 = b"Exif\0\0".to_vec();
    app1.extend_from_slice(b"II*\0");
    app1.extend_from_slice(&8u32.to_le_bytes());
    app1.extend_from_slice(&1u16.to_le_bytes()); // entry count
    app1.extend_from_slice(&0x0112u16.to_le_bytes()); // Orientation
    app1.extend_from_slice(&3u16.to_le_bytes()); // SHORT
    app1.extend_from_slice(&1u32.to_le_bytes()); // count
    app1.extend_from_slice(&[1, 0, 0, 0]); // value, left-justified
    app1.extend_from_slice(&0u32.to_le_bytes()); // no next IFD
    jpeg(8, 8, Some(&app1))
}

/// A JPEG whose APP1 Exif holds a TIFF in byte order `big` with `ifd0` entries plus, when
/// `exif` is not empty, an Exif IFD (0x8769) with those entries.
fn jpeg_exif(big: bool, ifd0: Vec<(u16, Val)>, exif: Vec<(u16, Val)>) -> Vec<u8> {
    let mut t = Tiff::new(big);
    let mut ifd0 = ifd0;
    if !exif.is_empty() {
        let exif_at = t.ifd(exif, 0);
        ifd0.push((0x8769, Val::Long(exif_at)));
    }
    let at = t.ifd(ifd0, 0);
    t.set_ifd0(at);
    let mut app1 = b"Exif\0\0".to_vec();
    app1.extend_from_slice(&t.out);
    jpeg(8, 8, Some(&app1))
}

/// The Exif IFD entries of a Z6III shot (values from the author's samples).
fn z6iii_exif() -> Vec<(u16, Val)> {
    vec![
        (0x829A, Val::Rationals(vec![(10, 1250)])), // ExposureTime
        (0x829D, Val::Rationals(vec![(560, 100)])), // FNumber
        (0x8827, Val::Short(450)),                  // PhotographicSensitivity
        (0x9003, Val::Ascii(b"2025:02:15 03:59:50\0")), // DateTimeOriginal
        (0x9004, Val::Ascii(b"2025:02:15 03:59:50\0")), // DateTimeDigitized
        (0x9011, Val::Ascii(b"+11:00\0")),          // OffsetTimeOriginal
        (0x920A, Val::Rationals(vec![(480, 10)])),  // FocalLength
        (0x9291, Val::Ascii(b"55\0")),              // SubSecTimeOriginal
        (
            0xA432,
            Val::Rationals(vec![(240, 10), (2000, 10), (400, 100), (630, 100)]),
        ), // LensSpecification
        (0xA434, Val::Ascii(b"NIKKOR Z 24-200mm f/4-6.3 VR\0")), // LensModel
    ]
}

fn jpeg_exif_full() -> Vec<u8> {
    let mut exif = z6iii_exif();
    exif.push((0xA002, Val::Short(8))); // PixelXDimension
    exif.push((0xA003, Val::Short(8))); // PixelYDimension
    jpeg_exif(
        false,
        vec![
            (0x0110, Val::Ascii(b"NIKON Z6_3\0")),
            (0x0112, Val::Short(6)),
        ],
        exif,
    )
}

fn jpeg_exif_create_date() -> Vec<u8> {
    jpeg_exif(
        true,
        vec![
            (0x0110, Val::Ascii(b"NIKON Z6_3\0")),
            (0x0112, Val::Short(8)),
        ],
        vec![
            (0x9004, Val::Ascii(b"2025:05:11 01:12:27\0")), // DateTimeDigitized
            (0x9012, Val::Ascii(b"-03:30\0")),              // OffsetTimeDigitized
            (0x9292, Val::Ascii(b"37\0")),                  // SubSecTimeDigitized
            (
                0xA432,
                Val::Rationals(vec![(240, 10), (2000, 10), (400, 100), (630, 100)]),
            ), // LensSpecification
        ],
    )
}

fn jpeg_exif_bad() -> Vec<u8> {
    jpeg_exif(
        false,
        vec![(0x0112, Val::Short(9))],
        vec![
            (0x9003, Val::Ascii(b"0000:00:00 00:00:00\0")), // DateTimeOriginal, unset clock
            (0x9004, Val::Ascii(b"2025:02:30 25:00:00\0")), // DateTimeDigitized, invalid
        ],
    )
}

// ---------------------------------------------------------------------------------------------
// TIFF writer
// ---------------------------------------------------------------------------------------------

const ASCII: u16 = 2;
const SHORT: u16 = 3;
const LONG: u16 = 4;
const RATIONAL: u16 = 5;
const UNDEFINED: u16 = 7;

/// A value of an IFD entry. Values over 4 bytes are written before the IFD.
#[derive(Clone)]
enum Val {
    Short(u16),
    Long(u32),
    Longs(Vec<u32>),
    Ascii(&'static [u8]),
    Rationals(Vec<(u32, u32)>),
    Undefined(Vec<u8>),
    /// `count` values of type `kind` already written at `offset`.
    At(u16, u32, u32),
}

/// TIFF bytes whose offsets are relative to the start of `out`.
struct Tiff {
    big: bool,
    out: Vec<u8>,
}

impl Tiff {
    /// A header whose IFD0 offset is set by `set_ifd0`.
    fn new(big: bool) -> Self {
        let mut out = if big {
            b"MM\0*".to_vec()
        } else {
            b"II*\0".to_vec()
        };
        out.extend_from_slice(&[0; 4]);
        Self { big, out }
    }

    fn u16(&self, v: u16) -> [u8; 2] {
        if self.big {
            v.to_be_bytes()
        } else {
            v.to_le_bytes()
        }
    }

    fn u32(&self, v: u32) -> [u8; 4] {
        if self.big {
            v.to_be_bytes()
        } else {
            v.to_le_bytes()
        }
    }

    /// Appends `data` at a word-aligned offset (TIFF 6.0) and returns that offset.
    fn push(&mut self, data: &[u8]) -> u32 {
        if self.out.len() % 2 == 1 {
            self.out.push(0);
        }
        let at = len32(&self.out);
        self.out.extend_from_slice(data);
        at
    }

    /// Writes an IFD with `entries` (sorted by tag here) and returns its offset.
    fn ifd(&mut self, entries: Vec<(u16, Val)>, next: u32) -> u32 {
        let mut entries = entries;
        entries.sort_by_key(|(tag, _)| *tag);
        let mut fields = Vec::new();
        for (tag, val) in &entries {
            let (kind, count, bytes) = match val {
                Val::Short(v) => (SHORT, 1, self.u16(*v).to_vec()),
                Val::Long(v) => (LONG, 1, self.u32(*v).to_vec()),
                Val::Longs(vs) => (
                    LONG,
                    vs.len(),
                    vs.iter().flat_map(|v| self.u32(*v)).collect(),
                ),
                Val::Ascii(s) => (ASCII, s.len(), s.to_vec()),
                Val::Rationals(rs) => (
                    RATIONAL,
                    rs.len(),
                    rs.iter()
                        .flat_map(|&(n, d)| [self.u32(n), self.u32(d)])
                        .flatten()
                        .collect(),
                ),
                Val::Undefined(b) => (UNDEFINED, b.len(), b.clone()),
                Val::At(kind, count, offset) => {
                    fields.push((*tag, *kind, *count, self.u32(*offset).to_vec()));
                    continue;
                }
            };
            let value = if bytes.len() <= 4 {
                let mut v = bytes;
                v.resize(4, 0); // left-justified
                v
            } else {
                let at = self.push(&bytes);
                self.u32(at).to_vec()
            };
            let count = u32::try_from(count).expect("few values");
            fields.push((*tag, kind, count, value));
        }
        let n = u16::try_from(fields.len()).expect("few entries");
        let mut ifd = self.u16(n).to_vec();
        for (tag, kind, count, value) in fields {
            ifd.extend_from_slice(&self.u16(tag));
            ifd.extend_from_slice(&self.u16(kind));
            ifd.extend_from_slice(&self.u32(count));
            ifd.extend_from_slice(&value);
        }
        ifd.extend_from_slice(&self.u32(next));
        self.push(&ifd)
    }

    fn patch_u32(&mut self, at: u32, v: u32) {
        let at = at as usize;
        let b = self.u32(v);
        self.out[at..at + 4].copy_from_slice(&b);
    }

    fn set_ifd0(&mut self, at: u32) {
        self.patch_u32(4, at);
    }

    /// Offset of the value field of entry `index` of the IFD at `ifd`.
    fn entry_value(ifd: u32, index: u32) -> u32 {
        ifd + 2 + 12 * index + 8
    }

    /// Offset of the next-IFD field of the IFD at `ifd` with `entries` entries.
    fn next_field(ifd: u32, entries: u32) -> u32 {
        ifd + 2 + 12 * entries
    }
}

// ---------------------------------------------------------------------------------------------
// NEF-shaped TIFF
// ---------------------------------------------------------------------------------------------

/// Where the image data starts: the IFDs, strings and MakerNote fit before it.
const NEF_DATA_START: u32 = 1024;

/// The embedded images of the synthetic NEF, by where they are referenced.
struct NefImages {
    /// IFD0 thumbnail, JPEGInterchangeFormat.
    thumb: Vec<u8>,
    /// SubIFD0, JPEGInterchangeFormat: the full-size preview (the largest).
    full: Vec<u8>,
    /// SubIFD1, a single JPEG strip (Compression 6).
    strip: Vec<u8>,
    /// SubIFD2, stand-in raw data (Compression 34713), not a JPEG.
    raw: Vec<u8>,
    /// MakerNote PreviewIFD, JPEGInterchangeFormat, stored inside the MakerNote.
    preview: Vec<u8>,
}

fn nef_images() -> NefImages {
    NefImages {
        thumb: jpeg(8, 8, None),
        full: jpeg(48, 32, None),
        strip: jpeg(24, 16, None),
        raw: (0..256u32)
            .map(|i| (i.wrapping_mul(37) % 251) as u8)
            .collect(),
        preview: jpeg(32, 24, None),
    }
}

/// A Nikon type 3 MakerNote: "Nikon\0", version 2.11, then a TIFF in its own byte order
/// whose offsets are relative to that TIFF header. Its IFD holds MakerNoteVersion and the
/// PreviewIFD (0x0011), which points at `preview` stored inside the MakerNote.
fn nikon_maker_note(big: bool, preview: &[u8]) -> Vec<u8> {
    let mut mn = Tiff::new(big);
    let jpeg_at = mn.push(preview);
    let preview_ifd = mn.ifd(
        vec![
            (0x0103, Val::Short(6)),
            (0x0201, Val::Long(jpeg_at)),
            (0x0202, Val::Long(len32(preview))),
        ],
        0,
    );
    let ifd = mn.ifd(
        vec![
            (0x0001, Val::Undefined(b"0211".to_vec())), // MakerNoteVersion
            (0x0011, Val::Long(preview_ifd)),
        ],
        0,
    );
    mn.set_ifd0(ifd);
    let mut out = b"Nikon\0\x02\x11\0\0".to_vec();
    out.extend_from_slice(&mn.out);
    out
}

/// A NEF-shaped TIFF in the main byte order `big`, with the MakerNote in the other one.
///
/// Layout: header, MakerNote, IFDs and strings below `NEF_DATA_START`, then the thumbnail,
/// the strip JPEG, the full-size JPEG and the raw data, in that order (as on a Z6III, the
/// raw data comes last).
fn nef(big: bool) -> Vec<u8> {
    nef_with(big, &NefExtras::default())
}

/// Extra tags for a NEF variant; the default gives the `pair/` and `lone/` NEFs.
#[derive(Default)]
struct NefExtras {
    /// More IFD0 entries.
    ifd0: Vec<(u16, Val)>,
    /// More Exif IFD entries.
    exif: Vec<(u16, Val)>,
    /// Store the MakerNote at the end of the file, after the raw data, instead of before the
    /// IFDs. Its offsets are relative to its own TIFF header, so it can sit anywhere.
    maker_note_last: bool,
}

/// The lone NEF with EXIF times and lens, its MakerNote last (see the module docs).
fn nef_with_exif() -> Vec<u8> {
    nef_with(
        false,
        &NefExtras {
            ifd0: vec![(0x0112, Val::Short(8))],
            exif: z6iii_exif(),
            maker_note_last: true,
        },
    )
}

fn nef_with(big: bool, extras: &NefExtras) -> Vec<u8> {
    let img = nef_images();
    let mut t = Tiff::new(big);

    // Image data offsets, laid out after the structures.
    let mut at = NEF_DATA_START;
    let mut place = |data: &[u8]| {
        let here = at;
        at += len32(data).next_multiple_of(2);
        here
    };
    let thumb_at = place(&img.thumb);
    let strip_at = place(&img.strip);
    let full_at = place(&img.full);
    let raw_at = place(&img.raw);

    let mn = nikon_maker_note(!big, &img.preview);
    let mn_at = if extras.maker_note_last {
        place(&mn)
    } else {
        t.push(&mn)
    };
    let mut exif_entries = vec![
        (0x9000, Val::Undefined(b"0230".to_vec())), // ExifVersion
        (0x927C, Val::At(UNDEFINED, len32(&mn), mn_at)), // MakerNote
    ];
    exif_entries.extend(extras.exif.iter().map(|(tag, v)| (*tag, v.clone())));
    let exif = t.ifd(exif_entries, 0);

    let sub0 = t.ifd(
        vec![
            (0x00FE, Val::Long(1)), // NewSubfileType: reduced resolution
            (0x0103, Val::Short(6)),
            (0x0201, Val::Long(full_at)),
            (0x0202, Val::Long(len32(&img.full))),
        ],
        0,
    );
    let sub1 = t.ifd(
        vec![
            (0x00FE, Val::Long(1)),
            (0x0100, Val::Long(24)),
            (0x0101, Val::Long(16)),
            (0x0102, Val::Short(8)),
            (0x0103, Val::Short(6)),
            (0x0106, Val::Short(1)), // BlackIsZero
            (0x0111, Val::Long(strip_at)),
            (0x0116, Val::Long(16)),
            (0x0117, Val::Long(len32(&img.strip))),
        ],
        0,
    );
    let sub2 = t.ifd(
        vec![
            (0x00FE, Val::Long(0)), // full resolution
            (0x0100, Val::Long(16)),
            (0x0101, Val::Long(8)),
            (0x0102, Val::Short(14)),
            (0x0103, Val::Short(34713)), // Nikon NEF compressed
            (0x0106, Val::Short(32803)), // CFA
            (0x0111, Val::Long(raw_at)),
            (0x0116, Val::Long(8)),
            (0x0117, Val::Long(len32(&img.raw))),
        ],
        0,
    );
    let mut ifd0_entries = vec![
        (0x00FE, Val::Long(1)),
        // Baseline tags for strict readers; the image is the 8x8 thumbnail.
        (0x0100, Val::Long(8)),
        (0x0101, Val::Long(8)),
        (0x0102, Val::Short(8)),
        (0x0103, Val::Short(6)), // old-style JPEG
        (0x0106, Val::Short(1)), // BlackIsZero
        (0x010F, Val::Ascii(b"NIKON CORPORATION\0")),
        (0x0110, Val::Ascii(b"CULLA SYNTHETIC\0")),
        (0x0115, Val::Short(1)),
        (0x014A, Val::Longs(vec![sub0, sub1, sub2])),
        (0x0201, Val::Long(thumb_at)),
        (0x0202, Val::Long(len32(&img.thumb))),
        (0x8769, Val::Long(exif)),
    ];
    ifd0_entries.extend(extras.ifd0.iter().map(|(tag, v)| (*tag, v.clone())));
    let ifd0 = t.ifd(ifd0_entries, 0);
    t.set_ifd0(ifd0);

    assert!(len32(&t.out) <= NEF_DATA_START, "NEF structures overflow");
    t.out.resize(NEF_DATA_START as usize, 0);
    let mut data = vec![
        (&img.thumb, thumb_at),
        (&img.strip, strip_at),
        (&img.full, full_at),
        (&img.raw, raw_at),
    ];
    if extras.maker_note_last {
        data.push((&mn, mn_at));
    }
    for (data, offset) in data {
        t.out.resize(offset as usize, 0);
        t.out.extend_from_slice(data);
    }
    t.out
}

/// IFD0 with a thumbnail and loops: IFD0's next IFD is itself, its SubIFD lists itself as a
/// SubIFD and has IFD0 as its next IFD. A walker must stop and still find the thumbnail.
fn nef_ifd_loop() -> Vec<u8> {
    let thumb = jpeg(8, 8, None);
    let mut t = Tiff::new(false);
    let thumb_at = t.push(&thumb);
    let sub = t.ifd(vec![(0x0103, Val::Short(1)), (0x014A, Val::Long(0))], 0);
    let ifd0 = t.ifd(
        vec![
            (0x0103, Val::Short(6)),
            (0x014A, Val::Long(sub)),
            (0x0201, Val::Long(thumb_at)),
            (0x0202, Val::Long(len32(&thumb))),
        ],
        0,
    );
    t.patch_u32(Tiff::entry_value(sub, 1), sub);
    t.patch_u32(Tiff::next_field(sub, 2), ifd0);
    t.patch_u32(Tiff::next_field(ifd0, 4), ifd0);
    t.set_ifd0(ifd0);
    t.out
}

/// The pair NEF cut in the middle of its full-size JPEG: that JPEG and the raw data are
/// missing, the IFDs, thumbnail, strip JPEG and MakerNote preview are intact.
fn nef_truncated() -> Vec<u8> {
    let mut n = nef(true);
    let img = nef_images();
    let full_at = NEF_DATA_START as usize
        + img.thumb.len().next_multiple_of(2)
        + img.strip.len().next_multiple_of(2);
    debug_assert_eq!(&n[full_at..full_at + img.full.len()], &img.full[..]);
    n.truncate(full_at + img.full.len() / 2);
    n
}

fn len32(bytes: &[u8]) -> u32 {
    u32::try_from(bytes.len()).expect("fixture under 4 GiB")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn be16(b: &[u8], at: usize) -> u16 {
        u16::from_be_bytes([b[at], b[at + 1]])
    }
    fn be32(b: &[u8], at: usize) -> u32 {
        u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
    }

    #[test]
    fn committed_fixtures_match_generator() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        check(&root).unwrap();
    }

    #[test]
    fn output_is_deterministic() {
        assert_eq!(all(), all());
    }

    #[test]
    fn jpeg_has_markers_and_exif_orientation() {
        let j = jpeg_with_exif();
        assert_eq!(&j[..4], &[0xFF, 0xD8, 0xFF, 0xE1]);
        assert_eq!(&j[j.len() - 2..], &[0xFF, 0xD9]);
        let app1_len = usize::from(be16(&j, 4));
        let app1 = &j[6..4 + app1_len];
        assert_eq!(&app1[..6], b"Exif\0\0");
        let tiff = &app1[6..];
        assert_eq!(&tiff[..4], b"II*\0");
        assert_eq!(u16::from_le_bytes([tiff[10], tiff[11]]), 0x0112);
        assert_eq!(u16::from_le_bytes([tiff[18], tiff[19]]), 1);
        // The next segment starts right after APP1.
        assert_eq!(j[4 + app1_len], 0xFF);
    }

    #[test]
    fn jpeg_entropy_data_has_two_bits_per_block() {
        // 8x8: one block, 2 bits, one padded byte (the M0 bytes).
        let j = jpeg(8, 8, None);
        assert_eq!(&j[j.len() - 3..], &[0b0011_1111, 0xFF, 0xD9]);
        // 32x24: 12 blocks, 24 bits, three zero bytes and no padding.
        let j = jpeg(32, 24, None);
        assert_eq!(&j[j.len() - 7..], &[0x3F, 0x00, 0, 0, 0, 0xFF, 0xD9]);
    }

    /// (tag, value or offset) of the IFD at `ifd` in a big-endian TIFF.
    fn entries(n: &[u8], ifd: usize) -> Vec<(u16, u32)> {
        let count = usize::from(be16(n, ifd));
        (0..count)
            .map(|i| {
                let e = ifd + 2 + 12 * i;
                let kind = be16(n, e + 2);
                let v = if kind == SHORT {
                    u32::from(be16(n, e + 8))
                } else {
                    be32(n, e + 8)
                };
                (be16(n, e), v)
            })
            .collect()
    }

    fn tag(e: &[(u16, u32)], t: u16) -> u32 {
        e.iter()
            .find(|(tag, _)| *tag == t)
            .map(|(_, v)| *v)
            .unwrap()
    }

    #[test]
    fn nef_layout_matches_its_description() {
        let n = nef(true);
        let img = nef_images();
        assert_eq!(&n[..4], b"MM\0*");
        let ifd0 = entries(&n, be32(&n, 4) as usize);
        assert!(ifd0.windows(2).all(|w| w[0].0 < w[1].0), "tags ascending");
        for baseline in [0x0100, 0x0101, 0x0102, 0x0103, 0x0106] {
            assert!(ifd0.iter().any(|(t, _)| *t == baseline), "{baseline:#06x}");
        }
        let jpeg_at = |e: &[(u16, u32)], base: usize| base + tag(e, 0x0201) as usize;
        let thumb = jpeg_at(&ifd0, 0);
        assert_eq!(&n[thumb..thumb + img.thumb.len()], &img.thumb[..]);

        let subs_at = tag(&ifd0, 0x014A) as usize;
        let subs: Vec<usize> = (0..3).map(|i| be32(&n, subs_at + 4 * i) as usize).collect();
        let sub0 = entries(&n, subs[0]);
        let full = jpeg_at(&sub0, 0);
        assert_eq!(&n[full..full + img.full.len()], &img.full[..]);
        let sub1 = entries(&n, subs[1]);
        let strip = tag(&sub1, 0x0111) as usize;
        assert_eq!(&n[strip..strip + img.strip.len()], &img.strip[..]);
        let sub2 = entries(&n, subs[2]);
        assert_eq!(tag(&sub2, 0x0103), 34713);
        assert_eq!(
            tag(&sub2, 0x0111) as usize + img.raw.len(),
            n.len(),
            "raw data last"
        );

        // MakerNote: Nikon header, little-endian TIFF, PreviewIFD relative to that TIFF.
        let exif = entries(&n, tag(&ifd0, 0x8769) as usize);
        let mn = tag(&exif, 0x927C) as usize;
        assert_eq!(&n[mn..mn + 10], b"Nikon\0\x02\x11\0\0");
        let base = mn + 10;
        assert_eq!(&n[base..base + 4], b"II*\0");
        let le32 = |at: usize| u32::from_le_bytes([n[at], n[at + 1], n[at + 2], n[at + 3]]);
        let le16 = |at: usize| u16::from_le_bytes([n[at], n[at + 1]]);
        let mn_ifd = base + le32(base + 4) as usize;
        let count = usize::from(le16(mn_ifd));
        let preview_ifd = (0..count)
            .map(|i| mn_ifd + 2 + 12 * i)
            .find(|&e| le16(e) == 0x0011)
            .map(|e| base + le32(e + 8) as usize)
            .unwrap();
        let pcount = usize::from(le16(preview_ifd));
        let jpeg = (0..pcount)
            .map(|i| preview_ifd + 2 + 12 * i)
            .find(|&e| le16(e) == 0x0201)
            .map(|e| base + le32(e + 8) as usize)
            .unwrap();
        assert_eq!(&n[jpeg..jpeg + img.preview.len()], &img.preview[..]);
    }

    #[test]
    fn nef_with_exif_keeps_the_structures_and_puts_the_maker_note_last() {
        let n = nef_with_exif();
        let plain = nef(false);
        assert_eq!(&n[..4], b"II*\0");
        // Same image data at the same offsets, then the MakerNote appended.
        let start = NEF_DATA_START as usize;
        assert_eq!(&n[start..plain.len()], &plain[start..]);
        let mn = nikon_maker_note(true, &nef_images().preview);
        assert!(n.len() >= plain.len() + mn.len());
        assert_eq!(&n[n.len() - mn.len()..], &mn[..]);
    }

    #[test]
    fn exif_jpegs_carry_an_exif_ifd() {
        for j in [jpeg_exif_full(), jpeg_exif_create_date(), jpeg_exif_bad()] {
            assert_eq!(&j[..4], &[0xFF, 0xD8, 0xFF, 0xE1]);
            let app1_len = usize::from(be16(&j, 4));
            assert_eq!(&j[6..12], b"Exif\0\0");
            let tiff = &j[12..4 + app1_len];
            let big = &tiff[..2] == b"MM";
            let rd16 = |at: usize| {
                let b = [tiff[at], tiff[at + 1]];
                if big {
                    u16::from_be_bytes(b)
                } else {
                    u16::from_le_bytes(b)
                }
            };
            let rd32 = |at: usize| {
                let b = [tiff[at], tiff[at + 1], tiff[at + 2], tiff[at + 3]];
                if big {
                    u32::from_be_bytes(b)
                } else {
                    u32::from_le_bytes(b)
                }
            };
            let ifd0 = rd32(4) as usize;
            let tags: Vec<u16> = (0..usize::from(rd16(ifd0)))
                .map(|i| rd16(ifd0 + 2 + 12 * i))
                .collect();
            assert!(
                tags.contains(&0x0112) && tags.contains(&0x8769),
                "{tags:x?}"
            );
        }
        assert_eq!(jpeg(8, 8, None)[2..4], [0xFF, 0xDB]);
    }

    #[test]
    fn little_endian_nef_is_the_mirror() {
        let n = nef(false);
        assert_eq!(&n[..4], b"II*\0");
        assert_eq!(n.len(), nef(true).len());
    }

    #[test]
    fn broken_variants() {
        let l = nef_ifd_loop();
        let ifd0 = u32::from_le_bytes([l[4], l[5], l[6], l[7]]) as usize;
        let next = ifd0 + 2 + 12 * 4;
        assert_eq!(
            u32::from_le_bytes([l[next], l[next + 1], l[next + 2], l[next + 3]]) as usize,
            ifd0
        );
        let t = nef_truncated();
        assert!(t.len() < nef(true).len());
        assert_eq!(&t[..], &nef(true)[..t.len()]);
    }
}
