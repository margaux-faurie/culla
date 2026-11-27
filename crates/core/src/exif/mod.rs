//! EXIF metadata as Culla uses it: capture time, orientation, lens, camera and exposure
//! (SPEC §5.1, ADR 0011, stories US-06 and US-07), and the capture-time order (ADR 0028).
//!
//! The adapter reads the tags with `kamadak-exif` and hands them over as plain data
//! ([`RawExif`]: ASCII values as bytes, numbers as numbers). Everything that turns those
//! values into typed ones happens here, without I/O: [`PhotoMeta::from_raw`]. Every parser
//! takes any bytes and never panics; a value it cannot read is treated as absent and recorded
//! as a [`MetaIssue`].
//!
//! # Capture time
//!
//! [`CaptureTime`] comes from the first source that holds a valid date and time:
//!
//! 1. `DateTimeOriginal` (0x9003), with `SubSecTimeOriginal` (0x9291) and
//!    `OffsetTimeOriginal` (0x9011);
//! 2. `CreateDate`, EXIF's `DateTimeDigitized` (0x9004), with `SubSecTimeDigitized` (0x9292)
//!    and `OffsetTimeDigitized` (0x9012);
//! 3. the file's modification time, supplied by the caller from the scan listing, read in the
//!    PC's local time zone (ADR 0028): the caller passes the PC's UTC offset at that instant
//!    with it ([`FileTime`]), so this module stays free of I/O and of the clock.
//!
//! A date and time is `YYYY:MM:DD HH:MM:SS` (a `-` is accepted in the date and a `T` between
//! the date and the time). A blank value (spaces, colons, NULs) or the all-zero
//! `0000:00:00 00:00:00` that cameras write for an unset clock means "unknown" and is not an
//! issue. A sub-second or offset value that cannot be read is dropped and recorded, and the
//! date and time is kept.
//!
//! The time is kept as the camera wrote it: [`CaptureTime::local_ms`], milliseconds since
//! 1970-01-01 00:00:00 *in the camera's local time*, the `photo_group.capture_time` column
//! (design/file-engine.md §2.3). The offset, when the camera wrote one, gives the instant
//! ([`CaptureTime::utc_ms`]).
//!
//! # Orientation
//!
//! `Orientation` (0x0112) in IFD0, 1 to 8. Absent means 1; any other value is treated as 1
//! and recorded. It is applied when the preview is built (ADR 0012).
//!
//! # Lens
//!
//! `LensModel` (0xA434), else a name built from `LensSpecification` (0xA432), as
//! `24-200mm f/4-6.3`, else none (SPEC §5.1, ADR 0011). Nikon's MakerNote lens ID is not
//! used: naming it needs ExifTool's GPL table.
//!
//! # Text
//!
//! Text values (model, lens) are untrusted (threat model T5, T6): bytes that are not UTF-8 are
//! replaced, control characters (Cc), format characters (Cf: bidi overrides and isolates,
//! zero-width characters, the BOM, tags) and the line and paragraph separators are dropped, surrounding spaces and NULs are trimmed, and the
//! result is capped at [`MAX_TEXT_CHARS`] characters. An empty result is `None`. The UI still
//! renders them as text only.
//!
//! # Order
//!
//! [`capture_order`] sorts groups by the capture time of their primary (the JPEG, or the NEF
//! of a lone NEF): stable and deterministic, see its docs and ADR 0028.

mod order;
#[cfg(test)]
mod tests;

use std::fmt;

pub use order::{capture_order, filename_order};

/// Longest text value kept, in characters. The Z6III's `LensModel` is 28.
pub const MAX_TEXT_CHARS: usize = 128;

/// An unsigned EXIF rational. A zero denominator is allowed here and means "unknown".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rational {
    /// Numerator.
    pub num: u32,
    /// Denominator.
    pub den: u32,
}

impl Rational {
    /// `num / den`.
    #[must_use]
    pub const fn new(num: u32, den: u32) -> Self {
        Self { num, den }
    }

    /// The value in tenths, rounded half up; `None` when the denominator is zero.
    #[must_use]
    pub fn tenths(self) -> Option<u64> {
        if self.den == 0 {
            return None;
        }
        let num = u64::from(self.num) * 10;
        let den = u64::from(self.den);
        Some((num + den / 2) / den)
    }

    /// `Some(self)` when both parts are non-zero: a usable positive value.
    #[must_use]
    pub fn positive(self) -> Option<Self> {
        (self.num != 0 && self.den != 0).then_some(self)
    }
}

/// EXIF tags as read by the adapter, before any interpretation. Plain data.
///
/// ASCII values are the bytes of the first string of the field, without its NUL. Numbers are
/// the first value of the field, widened to `u32`. `None` (or empty) means the tag is absent
/// or had an unexpected type.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RawExif {
    /// `DateTimeOriginal` (0x9003).
    pub date_time_original: Option<Vec<u8>>,
    /// `SubSecTimeOriginal` (0x9291).
    pub sub_sec_time_original: Option<Vec<u8>>,
    /// `OffsetTimeOriginal` (0x9011).
    pub offset_time_original: Option<Vec<u8>>,
    /// `DateTimeDigitized` (0x9004), ExifTool's `CreateDate`.
    pub create_date: Option<Vec<u8>>,
    /// `SubSecTimeDigitized` (0x9292).
    pub sub_sec_time_digitized: Option<Vec<u8>>,
    /// `OffsetTimeDigitized` (0x9012).
    pub offset_time_digitized: Option<Vec<u8>>,
    /// `Orientation` (0x0112), IFD0.
    pub orientation: Option<u32>,
    /// `Model` (0x0110), IFD0.
    pub model: Option<Vec<u8>>,
    /// `LensModel` (0xA434).
    pub lens_model: Option<Vec<u8>>,
    /// `LensSpecification` (0xA432): min and max focal length, min f-number at each.
    pub lens_specification: Vec<Rational>,
    /// `ExposureTime` (0x829A), seconds.
    pub exposure_time: Option<Rational>,
    /// `FNumber` (0x829D).
    pub f_number: Option<Rational>,
    /// `PhotographicSensitivity` (0x8827), ISO.
    pub iso: Option<u32>,
    /// `FocalLength` (0x920A), mm.
    pub focal_length: Option<Rational>,
    /// `PixelXDimension` (0xA002).
    pub pixel_x: Option<u32>,
    /// `PixelYDimension` (0xA003).
    pub pixel_y: Option<u32>,
}

/// EXIF orientation (0x0112): how to turn the stored image to show it upright.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Orientation {
    /// 1: upright as stored.
    #[default]
    Normal = 1,
    /// 2: mirrored left to right.
    MirrorHorizontal = 2,
    /// 3: rotated 180°.
    Rotate180 = 3,
    /// 4: mirrored top to bottom.
    MirrorVertical = 4,
    /// 5: mirrored along the top-left to bottom-right diagonal.
    Transpose = 5,
    /// 6: rotate 90° clockwise to show (camera turned right).
    Rotate90 = 6,
    /// 7: mirrored along the top-right to bottom-left diagonal.
    Transverse = 7,
    /// 8: rotate 270° clockwise (90° counter-clockwise) to show (camera turned left).
    Rotate270 = 8,
}

impl Orientation {
    /// The orientation for an EXIF value, `None` outside 1..=8.
    #[must_use]
    pub fn from_exif(value: u32) -> Option<Self> {
        Some(match value {
            1 => Self::Normal,
            2 => Self::MirrorHorizontal,
            3 => Self::Rotate180,
            4 => Self::MirrorVertical,
            5 => Self::Transpose,
            6 => Self::Rotate90,
            7 => Self::Transverse,
            8 => Self::Rotate270,
            _ => return None,
        })
    }

    /// The EXIF value, 1 to 8.
    #[must_use]
    pub fn exif_value(self) -> u8 {
        self as u8
    }

    /// True when the upright image has its width and height swapped (5 to 8): a portrait shot
    /// stored as landscape.
    #[must_use]
    pub fn swaps_dimensions(self) -> bool {
        matches!(
            self,
            Self::Transpose | Self::Rotate90 | Self::Transverse | Self::Rotate270
        )
    }
}

/// Where a [`CaptureTime`] came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TimeSource {
    /// `DateTimeOriginal`.
    DateTimeOriginal,
    /// `DateTimeDigitized` (`CreateDate`).
    CreateDate,
    /// The file's modification time from the listing, read in the PC's local time zone
    /// ([`FileTime`], ADR 0028). Cameras write it from their clock, so it can still be off
    /// against the EXIF times of other photos when the camera's zone is not the PC's.
    FileModified,
}

/// When a photo was taken, as the camera recorded it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CaptureTime {
    /// Milliseconds since 1970-01-01 00:00:00 in the camera's local time (the date and time
    /// as written, read as if it were UTC). For [`TimeSource::FileModified`] it is the file
    /// time in the PC's local time zone, at the offset the caller gave ([`FileTime`]).
    pub local_ms: i64,
    /// The camera's offset from UTC in minutes, when known.
    pub offset_minutes: Option<i16>,
    /// The tag (or file time) it came from.
    pub source: TimeSource,
}

impl CaptureTime {
    /// The instant in milliseconds since the Unix epoch, when the offset is known.
    #[must_use]
    pub fn utc_ms(&self) -> Option<i64> {
        let offset = i64::from(self.offset_minutes?);
        self.local_ms.checked_sub(offset.checked_mul(60_000)?)
    }

    /// The local time as ISO 8601 text, `2025-02-15T03:59:50.550`, followed by the offset
    /// (`+11:00`, `Z` for 0) when it is known. Years outside 0 to 9999, which only a file time
    /// can give, are written with a sign (`+12345`, `-0001`). For display: what the camera
    /// wrote, at millisecond precision.
    #[must_use]
    pub fn to_iso8601(&self) -> String {
        let days = self.local_ms.div_euclid(86_400_000);
        let ms_of_day = self.local_ms.rem_euclid(86_400_000);
        let (year, month, day) = civil_from_days(days);
        let secs = ms_of_day / 1000;
        let mut s = if (0..=9999).contains(&year) {
            format!("{year:04}")
        } else {
            format!("{year:+05}")
        };
        s.push_str(&format!(
            "-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}",
            secs / 3600,
            secs / 60 % 60,
            secs % 60,
            ms_of_day % 1000
        ));
        match self.offset_minutes {
            None => {}
            Some(0) => s.push('Z'),
            Some(m) => {
                let sign = if m < 0 { '-' } else { '+' };
                let m = m.unsigned_abs();
                s.push_str(&format!("{sign}{:02}:{:02}", m / 60, m % 60));
            }
        }
        s
    }

    /// The time from a file modification time, in the PC's local time at `file`'s offset.
    /// Cannot overflow: nanoseconds become milliseconds before the offset is added (the add
    /// saturates all the same).
    #[must_use]
    pub fn from_file_time(file: FileTime) -> Self {
        let offset_ms = i64::from(file.utc_offset_minutes) * 60_000;
        Self {
            local_ms: file
                .mtime_ns
                .div_euclid(1_000_000)
                .saturating_add(offset_ms),
            offset_minutes: Some(file.utc_offset_minutes),
            source: TimeSource::FileModified,
        }
    }
}

/// A file's modification time from the listing and the PC's offset from UTC at that instant
/// (ADR 0028). The adapter looks the offset up in the PC's time zone, with the daylight saving
/// rule in force on that date (`culla_adapters::winfs::local_utc_offset_minutes`); the core
/// only applies it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FileTime {
    /// Nanoseconds since the Unix epoch (UTC).
    pub mtime_ns: i64,
    /// The PC's offset from UTC at `mtime_ns`, in minutes east of UTC (`+60` for Paris in
    /// winter, `+120` in summer).
    pub utc_offset_minutes: i16,
}

impl FileTime {
    /// A file time at the given offset.
    #[must_use]
    pub const fn new(mtime_ns: i64, utc_offset_minutes: i16) -> Self {
        Self {
            mtime_ns,
            utc_offset_minutes,
        }
    }

    /// A file time read in UTC (offset 0): for tests, and where no time zone is known.
    #[must_use]
    pub const fn utc(mtime_ns: i64) -> Self {
        Self::new(mtime_ns, 0)
    }
}

/// Which time tag an issue is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TimeTag {
    /// `DateTimeOriginal` and its sub-second and offset tags.
    Original,
    /// `DateTimeDigitized` and its sub-second and offset tags.
    Digitized,
}

/// Something in the metadata that could not be used. None of them fails the read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MetaIssue {
    /// The file has no EXIF block.
    NoExif,
    /// The EXIF block could not be parsed at all.
    UnreadableExif,
    /// The EXIF parser skipped this many broken or truncated fields (for example a MakerNote
    /// that ends past the bytes read).
    SkippedFields(u32),
    /// A date and time that is not blank but cannot be read.
    BadDateTime(TimeTag),
    /// A sub-second value that is not digits.
    BadSubSec(TimeTag),
    /// An offset that is not `+HH:MM` or `-HH:MM`.
    BadOffset(TimeTag),
    /// An orientation outside 1..=8, treated as 1.
    BadOrientation(u32),
}

/// What Culla shows and sorts by for one photo.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PhotoMeta {
    /// Capture time, see the module docs for the fallbacks. `None` when no source had one.
    pub captured: Option<CaptureTime>,
    /// Orientation, 1 when absent or invalid.
    pub orientation: Orientation,
    /// Lens name: `LensModel`, else built from `LensSpecification`.
    pub lens: Option<String>,
    /// Camera model (`Model`), such as `NIKON Z6_3`.
    pub camera_model: Option<String>,
    /// Shutter time in seconds (`ExposureTime`).
    pub shutter: Option<Rational>,
    /// Aperture as an f-number (`FNumber`).
    pub aperture: Option<Rational>,
    /// ISO (`PhotographicSensitivity`).
    pub iso: Option<u32>,
    /// Focal length in mm (`FocalLength`).
    pub focal_length: Option<Rational>,
    /// Width of the stored image (`PixelXDimension`), before orientation. JPEGs have it; a
    /// NEF's IFD0 size is its thumbnail's, so it is not used.
    pub width: Option<u32>,
    /// Height of the stored image (`PixelYDimension`), before orientation.
    pub height: Option<u32>,
    /// What could not be used, in the order found.
    pub issues: Vec<MetaIssue>,
}

impl PhotoMeta {
    /// Interprets `raw`. `file_time`, the file's modification time from the listing with the
    /// PC's offset at that instant, is the last capture-time fallback.
    #[must_use]
    pub fn from_raw(raw: &RawExif, file_time: Option<FileTime>) -> Self {
        let mut issues = Vec::new();
        let captured = capture_time(raw, file_time, &mut issues);
        let orientation = match raw.orientation {
            None => Orientation::Normal,
            Some(v) => Orientation::from_exif(v).unwrap_or_else(|| {
                issues.push(MetaIssue::BadOrientation(v));
                Orientation::Normal
            }),
        };
        Self {
            captured,
            orientation,
            lens: lens(raw.lens_model.as_deref(), &raw.lens_specification),
            camera_model: raw.model.as_deref().and_then(clean_text),
            shutter: raw.exposure_time.and_then(Rational::positive),
            aperture: raw.f_number.and_then(Rational::positive),
            iso: raw.iso.filter(|&v| v != 0),
            focal_length: raw.focal_length.and_then(Rational::positive),
            width: raw.pixel_x.filter(|&v| v != 0),
            height: raw.pixel_y.filter(|&v| v != 0),
            issues,
        }
    }

    /// The metadata of a file whose EXIF could not be used: only the file time, and `issue`.
    #[must_use]
    pub fn without_exif(issue: MetaIssue, file_time: Option<FileTime>) -> Self {
        let mut meta = Self::from_raw(&RawExif::default(), file_time);
        meta.issues.insert(0, issue);
        meta
    }
}

fn capture_time(
    raw: &RawExif,
    file_time: Option<FileTime>,
    issues: &mut Vec<MetaIssue>,
) -> Option<CaptureTime> {
    let candidates = [
        (
            TimeTag::Original,
            TimeSource::DateTimeOriginal,
            &raw.date_time_original,
            &raw.sub_sec_time_original,
            &raw.offset_time_original,
        ),
        (
            TimeTag::Digitized,
            TimeSource::CreateDate,
            &raw.create_date,
            &raw.sub_sec_time_digitized,
            &raw.offset_time_digitized,
        ),
    ];
    for (tag, source, date_time, sub_sec, offset) in candidates {
        let Some(date_time) = date_time else { continue };
        let seconds = match parse_date_time(date_time) {
            Ok(Some(s)) => s,
            Ok(None) => continue,
            Err(InvalidValue) => {
                issues.push(MetaIssue::BadDateTime(tag));
                continue;
            }
        };
        let millis = match sub_sec.as_deref().map(parse_sub_sec) {
            None | Some(Ok(None)) => 0,
            Some(Ok(Some(ms))) => ms,
            Some(Err(InvalidValue)) => {
                issues.push(MetaIssue::BadSubSec(tag));
                0
            }
        };
        let offset_minutes = match offset.as_deref().map(parse_offset) {
            None | Some(Ok(None)) => None,
            Some(Ok(Some(m))) => Some(m),
            Some(Err(InvalidValue)) => {
                issues.push(MetaIssue::BadOffset(tag));
                None
            }
        };
        return Some(CaptureTime {
            // At most ~2.5e14 for year 9999: no overflow.
            local_ms: seconds * 1000 + i64::from(millis),
            offset_minutes,
            source,
        });
    }
    file_time.map(CaptureTime::from_file_time)
}

/// A value that is present but cannot be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidValue;

impl fmt::Display for InvalidValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid EXIF value")
    }
}

impl std::error::Error for InvalidValue {}

/// Trims trailing NULs and spaces (EXIF pads ASCII values with both).
fn trim_end(s: &[u8]) -> &[u8] {
    let end = s
        .iter()
        .rposition(|&b| b != 0 && b != b' ')
        .map_or(0, |i| i + 1);
    &s[..end]
}

/// Parses an EXIF date and time into seconds since 1970-01-01 00:00:00 of the same clock.
///
/// `Ok(None)` for a blank or all-zero value (unknown), `Err` for anything else that is not a
/// valid `YYYY:MM:DD HH:MM:SS` (years 1 to 9999).
///
/// # Errors
///
/// [`InvalidValue`] when the value is neither blank nor a valid date and time.
pub fn parse_date_time(s: &[u8]) -> Result<Option<i64>, InvalidValue> {
    let s = trim_end(s);
    if s.iter().all(|&b| matches!(b, b' ' | b':' | b'0' | 0)) {
        return Ok(None);
    }
    let [
        y0,
        y1,
        y2,
        y3,
        d0,
        mo0,
        mo1,
        d1,
        da0,
        da1,
        sep,
        h0,
        h1,
        c0,
        mi0,
        mi1,
        c1,
        s0,
        s1,
    ] = *s
    else {
        return Err(InvalidValue);
    };
    let date_sep = |b: u8| b == b':' || b == b'-';
    if !date_sep(d0) || d0 != d1 || !(sep == b' ' || sep == b'T') || c0 != b':' || c1 != b':' {
        return Err(InvalidValue);
    }
    let year = digits(&[y0, y1, y2, y3])?;
    let month = digits(&[mo0, mo1])?;
    let day = digits(&[da0, da1])?;
    let hour = digits(&[h0, h1])?;
    let minute = digits(&[mi0, mi1])?;
    let second = digits(&[s0, s1])?;
    if year == 0
        || !(1..=12).contains(&month)
        || day == 0
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return Err(InvalidValue);
    }
    let days = days_from_civil(i64::from(year), month, day);
    Ok(Some(
        days * 86_400 + i64::from(hour * 3600 + minute * 60 + second),
    ))
}

/// Parses `SubSecTime*` into milliseconds: the first three digits, right-padded (`"5"` is
/// 500 ms, `"52"` is 520 ms, `"123456"` is 123 ms).
///
/// # Errors
///
/// [`InvalidValue`] when the trimmed value holds anything but ASCII digits.
pub fn parse_sub_sec(s: &[u8]) -> Result<Option<u16>, InvalidValue> {
    let s = trim_end(s);
    let start = s.iter().position(|&b| b != b' ').unwrap_or(s.len());
    let s = &s[start..];
    if s.is_empty() {
        return Ok(None);
    }
    if !s.iter().all(u8::is_ascii_digit) {
        return Err(InvalidValue);
    }
    let ms = (0..3).fold(0u16, |acc, i| {
        acc * 10 + s.get(i).map_or(0, |&b| u16::from(b - b'0'))
    });
    Ok(Some(ms))
}

/// Parses `OffsetTime*` (`+HH:MM` or `-HH:MM`, up to ±14:00) into minutes east of UTC.
///
/// # Errors
///
/// [`InvalidValue`] when the value is neither blank nor a valid offset.
pub fn parse_offset(s: &[u8]) -> Result<Option<i16>, InvalidValue> {
    let s = trim_end(s);
    if s.iter().all(|&b| matches!(b, b' ' | b':')) {
        return Ok(None);
    }
    let [sign, h0, h1, b':', m0, m1] = *s else {
        return Err(InvalidValue);
    };
    let sign: i16 = match sign {
        b'+' => 1,
        b'-' => -1,
        _ => return Err(InvalidValue),
    };
    let hours = digits(&[h0, h1])?;
    let minutes = digits(&[m0, m1])?;
    if minutes > 59 || hours * 60 + minutes > 14 * 60 {
        return Err(InvalidValue);
    }
    // At most 840: fits.
    let total = i16::try_from(hours * 60 + minutes).map_err(|_| InvalidValue)?;
    Ok(Some(sign * total))
}

fn digits(b: &[u8]) -> Result<u32, InvalidValue> {
    b.iter().try_fold(0u32, |acc, &d| {
        if d.is_ascii_digit() {
            Ok(acc * 10 + u32::from(d - b'0'))
        } else {
            Err(InvalidValue)
        }
    })
}

fn is_leap(year: u32) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days from 1970-01-01 to the given proleptic Gregorian date (H. Hinnant's algorithm).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month);
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The proleptic Gregorian date of a day count from 1970-01-01, the inverse of
/// [`days_from_civil`] (H. Hinnant's algorithm). Any `i64` a millisecond time divides into
/// stays in range.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    // Both are in range by construction (1..=31, 1..=12).
    (
        year,
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}

/// Cleans an untrusted EXIF text value; see the module docs.
#[must_use]
pub fn clean_text(s: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(s);
    let cleaned: String = text
        .chars()
        .filter(|&c| !c.is_control() && !is_format_or_separator(c))
        .collect::<String>()
        .trim()
        .chars()
        .take(MAX_TEXT_CHARS)
        .collect();
    let cleaned = cleaned.trim_end().to_owned();
    (!cleaned.is_empty()).then_some(cleaned)
}

/// True for the Unicode format characters (general category Cf, Unicode 16) and the line and
/// paragraph separators (Zl, Zp). std has no category table, so the ranges are listed here.
fn is_format_or_separator(c: char) -> bool {
    matches!(
        u32::from(c),
        0x00AD
            | 0x0600..=0x0605
            | 0x061C
            | 0x06DD
            | 0x070F
            | 0x0890..=0x0891
            | 0x08E2
            | 0x180E
            | 0x200B..=0x200F
            | 0x2028..=0x202E
            | 0x2060..=0x2064
            | 0x2066..=0x206F
            | 0xFEFF
            | 0xFFF9..=0xFFFB
            | 0x110BD
            | 0x110CD
            | 0x13430..=0x1343F
            | 0x1BCA0..=0x1BCA3
            | 0x1D173..=0x1D17A
            | 0xE0001
            | 0xE0020..=0xE007F
    )
}

/// The lens name: `lens_model` when it holds text, else the name built from
/// `lens_specification`, else `None`.
#[must_use]
pub fn lens(lens_model: Option<&[u8]>, lens_specification: &[Rational]) -> Option<String> {
    lens_model
        .and_then(clean_text)
        .or_else(|| lens_from_specification(lens_specification))
}

/// Builds `24-200mm f/4-6.3` (or `50mm f/1.8`, or `24-70mm` when the f-numbers are unknown)
/// from `LensSpecification`. `None` without a usable focal length, or with fewer than four
/// values.
#[must_use]
pub fn lens_from_specification(spec: &[Rational]) -> Option<String> {
    let [min_fl, max_fl, min_fn, max_fn] = *spec else {
        return None;
    };
    let tenths = |r: Rational| r.positive().and_then(Rational::tenths).filter(|&t| t > 0);
    let (lo, hi) = match (tenths(min_fl), tenths(max_fl)) {
        (Some(lo), Some(hi)) if lo <= hi => (lo, hi),
        (Some(lo), Some(hi)) => (hi, lo),
        (Some(t), None) | (None, Some(t)) => (t, t),
        (None, None) => return None,
    };
    let mut name = if lo == hi {
        format!("{}mm", decimal(lo))
    } else {
        format!("{}-{}mm", decimal(lo), decimal(hi))
    };
    match (tenths(min_fn), tenths(max_fn)) {
        (Some(a), Some(b)) if a != b => {
            name.push_str(&format!(" f/{}-{}", decimal(a), decimal(b)));
        }
        (Some(a), _) | (None, Some(a)) => name.push_str(&format!(" f/{}", decimal(a))),
        (None, None) => {}
    }
    Some(name)
}

/// `40` -> `4`, `63` -> `6.3`.
fn decimal(tenths: u64) -> String {
    if tenths.is_multiple_of(10) {
        format!("{}", tenths / 10)
    } else {
        format!("{}.{}", tenths / 10, tenths % 10)
    }
}
