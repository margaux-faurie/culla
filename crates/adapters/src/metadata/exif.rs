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
//! # kamadak-exif API used
//!
//! `Reader::new().continue_on_error(true).read_from_container(&mut Cursor<Vec<u8>>)`, then
//! `Error::distill_partial_result`; `Exif::get_field(Tag, In::PRIMARY)` and the field's
//! `Value` (`Ascii`, `Rational`, `get_uint`). Fields of the Exif IFD carry `In::PRIMARY`, the
//! number of the IFD that points to them. Stable across 0.6.x.
//!
//! # What becomes an error
//!
//! Only failing to open or read the file is an error. A file without EXIF, or whose EXIF
//! cannot be parsed, still yields a [`PhotoMeta`] with the file time and a [`MetaIssue`]. A
//! panic inside the parser is caught and treated as unreadable EXIF (per-file isolation,
//! threat model T5).

use std::fs::File;
use std::io::{self, Cursor, Read};
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

/// The tags Culla reads, and how many fields the parser skipped.
fn raw_exif(head: Vec<u8>) -> Result<(RawExif, u32), MetaIssue> {
    let mut cursor = Cursor::new(head);
    let mut skipped = 0u32;
    let parsed = ::exif::Reader::new()
        .continue_on_error(true)
        .read_from_container(&mut cursor)
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
