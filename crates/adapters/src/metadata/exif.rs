//! EXIF of a JPEG or NEF through `kamadak-exif` (ADR 0011), mapped to
//! [`culla_core::exif::PhotoMeta`] (stories US-06, US-07).
//!
//! # Bounded, read-only reads
//!
//! The file is opened read-only and only its first [`EXIF_HEAD_BYTES`] are read, into memory
//! (never memory-mapped, ADR 0012). The parser sees that buffer, not the file, so a crafted
//! offset can never make it read further (threat model T5).
//!
//! - JPEG: the Z6III writes APP1 Exif right after SOI (about 41 KB with its MakerNote); an
//!   APP1 segment is at most 64 KiB, so the cap covers it with room for segments before it.
//! - NEF: a TIFF. `kamadak-exif` reads IFD0 (Orientation, Model) and follows its Exif IFD
//!   pointer like any TIFF, so the NEF's own walker (`culla_core::ifd`) is not needed here. On
//!   the Z6III every tag Culla reads ends before byte 36,200, but the MakerNote, also in the
//!   Exif IFD, ends between 225 and 266 KB (docs/research/z6iii-nef-layout.md), past the cap.
//!   The parser therefore runs with `continue_on_error`: a field that runs past the buffer is
//!   skipped (counted in [`MetaIssue::SkippedFields`]) and the others are kept.
//!
//! # Only TIFF data reaches the parser
//!
//! The container is recognised here, from its magic bytes, so a crafted file never reaches
//! `kamadak-exif`'s PNG, WebP or HEIF code (threat model T5):
//!
//! - `II*\0` or `MM\0*` (TIFF, NEF): the whole buffer is the TIFF.
//! - `FF D8` (JPEG): [`jpeg_exif_payload`] walks the marker segments inside the buffer and
//!   returns the payload of the first APP1 `Exif\0\0` segment before SOS, which is a TIFF.
//! - Anything else: [`MetaIssue::NoExif`].
//!
//! # kamadak-exif API used
//!
//! `Reader::new().continue_on_error(true).read_raw(Vec<u8>)` on that TIFF, then
//! `Error::distill_partial_result`; `Exif::get_field(Tag, In::PRIMARY)` and the field's
//! `Value` (`Ascii`, `Rational`, `get_uint`). Fields of the Exif IFD carry `In::PRIMARY`, the
//! number of the IFD that points to them. Stable across 0.6.x. `read_from_container` is not
//! used: it would dispatch on the format itself.
//!
//! # What becomes an error
//!
//! Only failing to open or read the file is an error. A file without EXIF, or whose EXIF
//! cannot be parsed, still yields a [`PhotoMeta`] with the file time and a [`MetaIssue`]. A
//! panic inside the parser is caught and treated as unreadable EXIF (per-file isolation,
//! threat model T5).

use std::fs::File;
use std::io::{self, Read};
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};

use culla_core::exif::{MetaIssue, PhotoMeta, Rational, RawExif};
use culla_core::grouping::Group;

/// Bytes read from the start of a file for its EXIF. SPEC §5.1 budgets ~128 KB per file for
/// the background pass; see the module docs for why it is enough on the Z6III.
pub const EXIF_HEAD_BYTES: u64 = 128 * 1024;

/// Most values kept of a multi-valued field (`LensSpecification` has 4).
const MAX_VALUES: usize = 8;

/// Reads at most `cap` bytes from the start of `path`, opened read-only.
///
/// A plain open, with the same path rule as
/// [`FileSource::open`](super::nef_preview::FileSource::open): pass only paths from the
/// current scan.
///
/// # Errors
///
/// When the file cannot be opened or read.
pub fn read_head(path: &Path, cap: u64) -> io::Result<Vec<u8>> {
    let file = File::open(path)?;
    let mut head = Vec::new();
    file.take(cap).read_to_end(&mut head)?;
    Ok(head)
}

/// The metadata of the JPEG or NEF at `path`, from its first [`EXIF_HEAD_BYTES`].
/// `mtime_ns` (the listing's modification time) is the last capture-time fallback.
///
/// # Errors
///
/// Only when the file cannot be opened or read; EXIF problems are [`PhotoMeta::issues`].
pub fn read_photo_meta(path: &Path, mtime_ns: Option<i64>) -> io::Result<PhotoMeta> {
    read_photo_meta_capped(path, mtime_ns, EXIF_HEAD_BYTES)
}

/// As [`read_photo_meta`], reading at most `cap` bytes (for tests and benchmarks).
///
/// # Errors
///
/// As [`read_photo_meta`].
pub fn read_photo_meta_capped(
    path: &Path,
    mtime_ns: Option<i64>,
    cap: u64,
) -> io::Result<PhotoMeta> {
    let head = read_head(path, cap)?;
    Ok(meta_from_head(head, mtime_ns))
}

/// The metadata of a group: that of its primary (the JPEG, or the NEF of a lone NEF), with
/// the primary's listed modification time as the fallback. `root` is the scanned source.
///
/// # Errors
///
/// As [`read_photo_meta`].
pub fn read_group_meta(root: &Path, group: &Group) -> io::Result<PhotoMeta> {
    let entry = &group.primary.entry;
    read_photo_meta(&entry_path(root, &entry.components()), entry.mtime_ns)
}

fn entry_path(root: &Path, components: &[String]) -> PathBuf {
    let mut path = root.to_path_buf();
    for c in components {
        path.push(c);
    }
    path
}

/// Parses the first bytes of a file (any length) into metadata. Never fails and never panics
/// out: see the module docs.
#[must_use]
pub fn meta_from_head(head: Vec<u8>, mtime_ns: Option<i64>) -> PhotoMeta {
    let parsed = panic::catch_unwind(AssertUnwindSafe(|| raw_exif(head)))
        .unwrap_or(Err(MetaIssue::UnreadableExif));
    match parsed {
        Ok((raw, skipped)) => {
            let mut meta = PhotoMeta::from_raw(&raw, mtime_ns);
            if skipped > 0 {
                meta.issues.insert(0, MetaIssue::SkippedFields(skipped));
            }
            meta
        }
        Err(issue) => PhotoMeta::without_exif(issue, mtime_ns),
    }
}

/// TIFF magic: little-endian, big-endian.
const TIFF_MAGIC: [&[u8; 4]; 2] = [b"II*\0", b"MM\0*"];
/// Identifier at the start of an APP1 Exif segment's payload.
const EXIF_ID: &[u8; 6] = b"Exif\0\0";

/// The TIFF data of `head`: see the module docs.
fn tiff_data(head: Vec<u8>) -> Result<Vec<u8>, MetaIssue> {
    if TIFF_MAGIC.iter().any(|m| head.starts_with(*m)) {
        Ok(head)
    } else if head.starts_with(&[0xFF, 0xD8]) {
        jpeg_exif_payload(&head).map(<[u8]>::to_vec)
    } else {
        Err(MetaIssue::NoExif)
    }
}

/// The TIFF payload of the first APP1 `Exif\0\0` segment of the JPEG in `jpeg`, which starts
/// with SOI. Every read stays inside `jpeg`.
///
/// # Errors
///
/// [`MetaIssue::NoExif`] when SOS or EOI comes first; [`MetaIssue::UnreadableExif`] when the
/// segments are malformed or run past the end of `jpeg` (a truncated head) before one is found.
pub fn jpeg_exif_payload(jpeg: &[u8]) -> Result<&[u8], MetaIssue> {
    let mut p = 2usize;
    loop {
        // A marker: one or more 0xFF fill bytes, then the code.
        if *jpeg.get(p).ok_or(MetaIssue::UnreadableExif)? != 0xFF {
            return Err(MetaIssue::UnreadableExif);
        }
        while jpeg.get(p) == Some(&0xFF) {
            p += 1;
        }
        let code = *jpeg.get(p).ok_or(MetaIssue::UnreadableExif)?;
        p += 1;
        match code {
            // SOS: the entropy-coded data starts; Exif must come before it. EOI: the end.
            0xDA | 0xD9 => return Err(MetaIssue::NoExif),
            // Stand-alone markers without a length: TEM, RST0-7.
            0x01 | 0xD0..=0xD7 => continue,
            // A second SOI or a zero code is not a valid segment.
            0x00 | 0xD8 => return Err(MetaIssue::UnreadableExif),
            _ => {}
        }
        let len = match jpeg.get(p..p + 2) {
            Some(&[hi, lo]) => usize::from(u16::from_be_bytes([hi, lo])),
            _ => return Err(MetaIssue::UnreadableExif),
        };
        // The length counts its own two bytes.
        let payload = len
            .checked_sub(2)
            .and_then(|n| jpeg.get(p + 2..p + 2 + n))
            .ok_or(MetaIssue::UnreadableExif)?;
        if code == 0xE1
            && let Some(tiff) = payload.strip_prefix(EXIF_ID)
        {
            return Ok(tiff);
        }
        p += len;
    }
}

/// The tags Culla reads, and how many fields the parser skipped.
fn raw_exif(head: Vec<u8>) -> Result<(RawExif, u32), MetaIssue> {
    let tiff = tiff_data(head)?;
    let mut skipped = 0u32;
    let parsed = ::exif::Reader::new()
        .continue_on_error(true)
        .read_raw(tiff)
        .or_else(|e| {
            e.distill_partial_result(|errors| {
                skipped = u32::try_from(errors.len()).unwrap_or(u32::MAX);
            })
        });
    match parsed {
        Ok(exif) => Ok((to_raw(&exif), skipped)),
        Err(::exif::Error::NotFound(_)) => Err(MetaIssue::NoExif),
        Err(_) => Err(MetaIssue::UnreadableExif),
    }
}

fn to_raw(exif: &::exif::Exif) -> RawExif {
    use ::exif::Tag;
    RawExif {
        date_time_original: ascii(exif, Tag::DateTimeOriginal),
        sub_sec_time_original: ascii(exif, Tag::SubSecTimeOriginal),
        offset_time_original: ascii(exif, Tag::OffsetTimeOriginal),
        create_date: ascii(exif, Tag::DateTimeDigitized),
        sub_sec_time_digitized: ascii(exif, Tag::SubSecTimeDigitized),
        offset_time_digitized: ascii(exif, Tag::OffsetTimeDigitized),
        orientation: uint(exif, Tag::Orientation),
        model: ascii(exif, Tag::Model),
        lens_model: ascii(exif, Tag::LensModel),
        lens_specification: rationals(exif, Tag::LensSpecification),
        exposure_time: rationals(exif, Tag::ExposureTime).first().copied(),
        f_number: rationals(exif, Tag::FNumber).first().copied(),
        iso: uint(exif, Tag::PhotographicSensitivity),
        focal_length: rationals(exif, Tag::FocalLength).first().copied(),
        pixel_x: uint(exif, Tag::PixelXDimension),
        pixel_y: uint(exif, Tag::PixelYDimension),
    }
}

fn value(exif: &::exif::Exif, tag: ::exif::Tag) -> Option<&::exif::Value> {
    exif.get_field(tag, ::exif::In::PRIMARY).map(|f| &f.value)
}

/// The first string of an ASCII field.
fn ascii(exif: &::exif::Exif, tag: ::exif::Tag) -> Option<Vec<u8>> {
    match value(exif, tag)? {
        ::exif::Value::Ascii(strings) => strings.first().cloned(),
        _ => None,
    }
}

/// The first value of a BYTE, SHORT or LONG field.
fn uint(exif: &::exif::Exif, tag: ::exif::Tag) -> Option<u32> {
    value(exif, tag)?.get_uint(0)
}

/// The first [`MAX_VALUES`] values of a RATIONAL field; empty for any other type.
fn rationals(exif: &::exif::Exif, tag: ::exif::Tag) -> Vec<Rational> {
    match value(exif, tag) {
        Some(::exif::Value::Rational(values)) => values
            .iter()
            .take(MAX_VALUES)
            .map(|r| Rational::new(r.num, r.denom))
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_path_joins_components() {
        let p = entry_path(
            Path::new("root"),
            &["100NIKON".into(), "DSC_0001.JPG".into()],
        );
        assert_eq!(p, Path::new("root").join("100NIKON").join("DSC_0001.JPG"));
    }

    /// A JPEG: SOI, then `segments` as (marker code, payload), then SOS and EOI.
    fn jpeg(segments: &[(u8, &[u8])]) -> Vec<u8> {
        let mut out = vec![0xFF, 0xD8];
        for (code, payload) in segments {
            out.extend_from_slice(&[0xFF, *code]);
            out.extend_from_slice(&u16::try_from(payload.len() + 2).unwrap().to_be_bytes());
            out.extend_from_slice(payload);
        }
        out.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x02, 0x12, 0x34, 0xFF, 0xD9]);
        out
    }

    #[test]
    fn jpeg_payload_is_the_first_exif_app1_before_sos() {
        let exif = b"Exif\0\0II*\0rest";
        let j = jpeg(&[
            (0xE0, b"JFIF\0"),
            (0xE1, b"http://ns.adobe.com/xap/1.0/\0"),
            (0xE1, exif),
        ]);
        assert_eq!(jpeg_exif_payload(&j), Ok(&b"II*\0rest"[..]));
        // Fill bytes before a marker and stand-alone markers are skipped.
        let mut j = vec![0xFF, 0xD8, 0xFF, 0xFF, 0xD0, 0xFF, 0x01];
        j.extend_from_slice(&jpeg(&[(0xE1, exif)])[2..]);
        assert_eq!(jpeg_exif_payload(&j), Ok(&b"II*\0rest"[..]));
        // Nothing before SOS, or an Exif APP1 after it: none.
        assert_eq!(
            jpeg_exif_payload(&jpeg(&[(0xDB, &[0; 65])])),
            Err(MetaIssue::NoExif)
        );
        let mut late = jpeg(&[]);
        late.extend_from_slice(&jpeg(&[(0xE1, exif)])[2..]);
        assert_eq!(jpeg_exif_payload(&late), Err(MetaIssue::NoExif));
    }

    #[test]
    fn jpeg_payload_rejects_malformed_or_truncated_segments() {
        let exif = b"Exif\0\0II*\0rest";
        let full = jpeg(&[(0xE0, b"JFIF\0"), (0xE1, exif)]);
        // Cut anywhere before the Exif payload ends: unreadable, never a read past the end.
        let app1_end = 2 + 9 + 4 + exif.len();
        for cut in 0..app1_end {
            assert_eq!(
                jpeg_exif_payload(&full[..cut]),
                Err(MetaIssue::UnreadableExif),
                "{cut}"
            );
        }
        assert!(jpeg_exif_payload(&full[..app1_end]).is_ok());
        for bad in [
            vec![0xFF, 0xD8, 0x00, 0xE1],             // not a marker
            vec![0xFF, 0xD8, 0xFF, 0xE1, 0x00, 0x01], // length below 2
            vec![0xFF, 0xD8, 0xFF, 0xE1, 0xFF, 0xFF], // length past the end
            vec![0xFF, 0xD8, 0xFF, 0xD8],             // a second SOI
            vec![0xFF, 0xD8, 0xFF, 0x00],             // a stuffed zero
        ] {
            assert_eq!(
                jpeg_exif_payload(&bad),
                Err(MetaIssue::UnreadableExif),
                "{bad:x?}"
            );
        }
    }

    #[test]
    fn only_tiff_and_jpeg_reach_the_parser() {
        let png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        let mut heif = b"\0\0\0\x18ftypheic".to_vec();
        heif.extend_from_slice(&[0; 16]);
        let webp = b"RIFF\x10\0\0\0WEBPVP8 ".to_vec();
        for head in [png, heif, webp, Vec::new(), b"II*".to_vec()] {
            assert_eq!(tiff_data(head.clone()), Err(MetaIssue::NoExif), "{head:x?}");
            let meta = meta_from_head(head, None);
            assert_eq!(meta.issues, [MetaIssue::NoExif]);
        }
        let tiff = b"MM\0*\0\0\0\x08".to_vec();
        assert_eq!(tiff_data(tiff.clone()), Ok(tiff));
        let j = jpeg(&[(0xE1, b"Exif\0\0MM\0*")]);
        assert_eq!(tiff_data(j), Ok(b"MM\0*".to_vec()));
    }

    #[test]
    fn heads_that_are_not_images_yield_only_the_file_time() {
        for head in [
            Vec::new(),
            vec![0xFF, 0xD8],
            b"not an image at all".to_vec(),
        ] {
            let meta = meta_from_head(head, Some(1_000_000));
            assert_eq!(meta.captured.unwrap().local_ms, 1);
            assert!(
                matches!(
                    meta.issues.first(),
                    Some(MetaIssue::NoExif | MetaIssue::UnreadableExif)
                ),
                "{:?}",
                meta.issues
            );
        }
    }
}
