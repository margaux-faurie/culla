//! TIFF/IFD walker that finds the JPEG previews embedded in a NEF (ADR 0011, story US-03).
//!
//! The walker reads only TIFF headers, IFDs and the few bytes of each embedded JPEG it needs
//! to check it (SOI, the frame header and the last bytes). It never reads raw image data.
//!
//! # Why a `ReadAt` port and not `&[u8]`
//!
//! A 24 MP NEF is 17–20 MB, and the structures the walker needs sit in its first ~250 KB plus a
//! few bytes at the start and end of each embedded JPEG. Taking `&[u8]` would make the adapter
//! load the whole file (or memory-map it, which is banned on removable media, ADR 0012) for
//! every preview. A small positioned-read trait lets the adapter serve those reads from a file
//! handle with a read-ahead buffer, while tests and the fuzz target pass a byte slice
//! (`impl ReadAt for &[u8]`). The core stays I/O free: it only calls the trait.
//!
//! # What it walks
//!
//! - The main IFD chain (IFD0, IFD1, ...), SubIFDs (tag 0x014A, also nested) and the EXIF IFD
//!   (0x8769), in big- and little-endian files.
//! - The Nikon type 3 MakerNote (0x927C in the EXIF IFD): `"Nikon\0"`, a 4-byte version, then
//!   an embedded TIFF header with its own byte order; its offsets are relative to that header.
//!   Its PreviewIFD (MakerNote tag 0x0011) is walked like an image IFD.
//!
//! # What it collects
//!
//! Every embedded JPEG candidate: `JPEGInterchangeFormat`/`Length` (0x0201/0x0202) in any image
//! IFD, and single-strip `StripOffsets`/`StripByteCounts` (0x0111/0x0117) in an IFD whose
//! Compression is 6 or 7. Each candidate is checked: in bounds, under the size cap, SOI at the
//! start, EOI at the end (trailing zero padding is trimmed), and a baseline, extended or
//! progressive 8-bit frame header, whose size becomes the candidate's width and height.
//! [`Walk::best`] picks the valid candidate with the most pixels, then the most bytes.
//!
//! # Hostile input (threat model T5)
//!
//! Every offset and length is bounds-checked with checked `u64` arithmetic before a read; IFDs
//! are counted, capped in entries and depth, and remembered by absolute offset so loops end;
//! every allocation is bounded by [`Limits`]. Structural problems below the TIFF header do not
//! fail the walk: they are recorded as [`Issue`]s and the walker carries on with what it can
//! still reach, so a broken MakerNote never hides a good SubIFD preview. Only a read error from
//! the source or a missing TIFF header is fatal.

mod jpeg;
#[cfg(test)]
mod tests;

use std::fmt;
use std::io;

/// Positioned reads from a byte source (a file in the adapters, a slice in tests).
///
/// The walker only asks for ranges inside `0..len()`, so an implementation may treat any
/// short read as an error.
pub trait ReadAt {
    /// Length of the source in bytes.
    fn len(&self) -> u64;

    /// True when the source has no bytes.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Fills `buf` with the bytes at `offset..offset + buf.len()`.
    fn read_exact_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<()>;
}

impl ReadAt for &[u8] {
    fn len(&self) -> u64 {
        <[u8]>::len(self) as u64
    }

    fn read_exact_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        let eof = || io::Error::from(io::ErrorKind::UnexpectedEof);
        let start = usize::try_from(offset).map_err(|_| eof())?;
        let end = start.checked_add(buf.len()).ok_or_else(eof)?;
        let src = self.get(start..end).ok_or_else(eof)?;
        buf.copy_from_slice(src);
        Ok(())
    }
}

/// Caps that bound the work and memory of one walk, whatever the input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// IFDs read in total, across all chains and the MakerNote. A Z6III NEF has 11.
    pub max_ifds: usize,
    /// Entries in one IFD; a larger IFD is skipped. The Z6III MakerNote has 81.
    pub max_entries: usize,
    /// Nesting depth. IFD0 is 0; IFD0 > EXIF > MakerNote > PreviewIFD reaches 3.
    pub max_depth: u8,
    /// SubIFD offsets followed per 0x014A tag. The Z6III has 6.
    pub max_sub_ifds: usize,
    /// Candidates kept. The Z6III has 4.
    pub max_candidates: usize,
    /// Issues kept; later ones are counted in [`Walk::issues_dropped`].
    pub max_issues: usize,
    /// Largest JPEG accepted as a preview. The Z6III's full-size preview is 1.2–3.4 MB.
    pub max_preview_bytes: u64,
    /// JPEG marker segments read before the frame header (SOFn) must appear.
    pub max_jpeg_segments: usize,
    /// Largest declared pixel count (width x height from SOFn) accepted, so a small
    /// JPEG claiming a huge frame cannot win and reach the decoder (threat model T5).
    /// The Z6III's full-size preview is 24.4 MP; the largest Nikon Z bodies are ~46 MP.
    pub max_pixels: u64,
}

impl Limits {
    /// The limits used by the app. See the field docs for what real files need.
    pub const DEFAULT: Self = Self {
        max_ifds: 64,
        max_entries: 1024,
        max_depth: 4,
        max_sub_ifds: 32,
        max_candidates: 32,
        max_issues: 32,
        max_preview_bytes: 64 * 1024 * 1024,
        max_jpeg_segments: 64,
        max_pixels: 100_000_000,
    };
}

impl Default for Limits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Byte order of a TIFF structure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ByteOrder {
    /// `II`, Intel.
    Little,
    /// `MM`, Motorola.
    Big,
}

/// A range of bytes in the source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ByteRange {
    pub offset: u64,
    pub length: u64,
}

impl ByteRange {
    /// One past the last byte, or `None` on overflow.
    pub fn end(self) -> Option<u64> {
        self.offset.checked_add(self.length)
    }
}

/// Which IFD a candidate came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateSource {
    /// The main chain: index 0 is IFD0, 1 is IFD1.
    Ifd { index: u8 },
    /// A SubIFD, by its position in the parent's 0x014A array (nested SubIFDs keep their own
    /// position, so the index alone is not unique in exotic files).
    SubIfd { index: u8 },
    /// The Nikon MakerNote PreviewIFD (MakerNote tag 0x0011).
    MakerNotePreview,
}

/// Which tags pointed at the candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JpegPointer {
    /// `JPEGInterchangeFormat` and `JPEGInterchangeFormatLength` (0x0201/0x0202).
    InterchangeFormat,
    /// A single `StripOffsets`/`StripByteCounts` pair (0x0111/0x0117) with Compression 6 or 7.
    Strip,
}

/// Why a candidate is not usable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reject {
    /// The range overflows or ends past the end of the source.
    OutOfBounds,
    /// Shorter than SOI + EOI.
    TooShort,
    /// Longer than [`Limits::max_preview_bytes`].
    TooLarge,
    /// No SOI (FF D8) at the start.
    NoSoi,
    /// No EOI (FF D9) at the end, ignoring trailing zero padding.
    NoEoi,
    /// No frame header before the scan, a bad marker, or more segments than allowed.
    NoFrame,
    /// A frame type or precision a JPEG decoder for previews does not take (lossless,
    /// hierarchical, arithmetic or 12-bit JPEG, as in raw data stored as JPEG).
    UnsupportedFrame { marker: u8, precision: u8 },
    /// The frame header declares zero width or height.
    ZeroSize,
    /// The frame header declares more than [`Limits::max_pixels`] pixels.
    TooManyPixels,
}

/// Result of checking one candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JpegCheck {
    Valid,
    Invalid(Reject),
}

/// An embedded JPEG the walker found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Candidate {
    /// Where the JPEG is. For a valid candidate, trailing zero padding after EOI is trimmed.
    pub range: ByteRange,
    /// From the JPEG frame header when valid, otherwise from the IFD's ImageWidth/Length.
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub source: CandidateSource,
    pub pointer: JpegPointer,
    /// NewSubfileType (0x00FE) of the IFD, if present (1 = reduced-resolution image).
    pub new_subfile_type: Option<u32>,
    /// Compression (0x0103) of the IFD, if present.
    pub compression: Option<u32>,
    pub check: JpegCheck,
}

impl Candidate {
    pub fn is_valid(&self) -> bool {
        self.check == JpegCheck::Valid
    }

    /// Width times height, 0 when unknown.
    pub fn pixels(&self) -> u64 {
        u64::from(self.width.unwrap_or(0)) * u64::from(self.height.unwrap_or(0))
    }
}

/// A structural problem the walker stepped over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Issue {
    /// Absolute offset of the IFD or entry concerned.
    pub offset: u64,
    pub kind: IssueKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IssueKind {
    /// An IFD (or its offset) lies outside the source.
    IfdOutOfBounds,
    /// An IFD offset seen before: a loop between IFDs.
    IfdLoop,
    /// [`Limits::max_ifds`] reached; the IFD was not read.
    TooManyIfds,
    /// [`Limits::max_entries`] exceeded; the IFD was skipped.
    TooManyEntries { count: u16 },
    /// [`Limits::max_depth`] exceeded; the IFD was not read.
    TooDeep,
    /// A tag's value lies outside the source.
    ValueOutOfBounds { tag: u16 },
    /// A tag has a type or count the walker cannot use.
    BadValue { tag: u16 },
    /// More SubIFD offsets than [`Limits::max_sub_ifds`]; the rest were ignored.
    TooManySubIfds { count: u32 },
    /// A JPEG stored in more than one strip; not used as a preview.
    MultiStrip { count: u32 },
    /// A MakerNote that is not Nikon type 3, or whose TIFF header is broken.
    UnsupportedMakerNote,
    /// [`Limits::max_candidates`] reached; the candidate was ignored.
    TooManyCandidates,
}

/// Everything one walk found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Walk {
    /// Byte order of the main TIFF header.
    pub order: ByteOrder,
    /// Candidates in the order found, without duplicates of the same range.
    pub candidates: Vec<Candidate>,
    pub issues: Vec<Issue>,
    /// Issues not kept because of [`Limits::max_issues`].
    pub issues_dropped: usize,
}

impl Walk {
    /// The valid candidate with the most pixels, then the most bytes; the first found wins ties.
    pub fn best(&self) -> Option<&Candidate> {
        let mut best: Option<&Candidate> = None;
        for c in self.candidates.iter().filter(|c| c.is_valid()) {
            let better =
                best.is_none_or(|b| (c.pixels(), c.range.length) > (b.pixels(), b.range.length));
            if better {
                best = Some(c);
            }
        }
        best
    }
}

/// Why a walk or a preview search failed.
///
/// Hand-written for now: the convention is `thiserror` in core, but adding it needs the
/// core dependency allowlist (`scripts/ci/check-core-deps.sh`, a protected path). Switch
/// when core takes its first dependency.
#[derive(Debug)]
pub enum WalkError {
    /// The source does not start with a classic TIFF header (`II*\0` or `MM\0*`).
    NotTiff,
    /// The source failed to read. Not caused by the file's content: the walker only reads
    /// inside the source's length.
    Read(io::Error),
    /// The walk found no valid embedded JPEG.
    NoPreview { candidates: usize },
}

impl fmt::Display for WalkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotTiff => f.write_str("not a TIFF-based file"),
            Self::Read(e) => write!(f, "read failed: {e}"),
            Self::NoPreview { candidates: 0 } => f.write_str("no embedded JPEG found"),
            Self::NoPreview { candidates } => {
                write!(f, "none of the {candidates} embedded JPEG(s) is usable")
            }
        }
    }
}

impl std::error::Error for WalkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for WalkError {
    fn from(e: io::Error) -> Self {
        Self::Read(e)
    }
}

/// Walks the TIFF structure of `src` and collects the embedded JPEG candidates.
pub fn walk<R: ReadAt + ?Sized>(src: &mut R, limits: &Limits) -> Result<Walk, WalkError> {
    let len = src.len();
    if len < 8 {
        return Err(WalkError::NotTiff);
    }
    let mut header = [0u8; 8];
    src.read_exact_at(0, &mut header)?;
    let (order, ifd0) = parse_header(&header).ok_or(WalkError::NotTiff)?;
    let mut walker = Walker {
        src,
        len,
        limits,
        visited: Vec::new(),
        declared: Vec::new(),
        walk: Walk {
            order,
            candidates: Vec::new(),
            issues: Vec::new(),
            issues_dropped: 0,
        },
    };
    let main = Tiff { base: 0, order };
    walker.walk_chain(main, ifd0, 0, Kind::Main(0))?;
    Ok(walker.walk)
}

/// Walks `src` and returns the best candidate ([`Walk::best`]).
pub fn find_preview<R: ReadAt + ?Sized>(
    src: &mut R,
    limits: &Limits,
) -> Result<Candidate, WalkError> {
    let walk = walk(src, limits)?;
    walk.best().copied().ok_or(WalkError::NoPreview {
        candidates: walk.candidates.len(),
    })
}

/// `II*\0` or `MM\0*` and the first IFD offset. BigTIFF (43) is not a NEF and is refused.
fn parse_header(h: &[u8; 8]) -> Option<(ByteOrder, u32)> {
    let order = match (h[0], h[1]) {
        (b'I', b'I') => ByteOrder::Little,
        (b'M', b'M') => ByteOrder::Big,
        _ => return None,
    };
    let t = Tiff { base: 0, order };
    (t.u16(&h[2..4]) == 42).then(|| (order, t.u32(&h[4..8])))
}

/// One TIFF structure: the main file, or the TIFF embedded in the MakerNote.
#[derive(Clone, Copy)]
struct Tiff {
    /// Absolute offset that the structure's offsets are relative to.
    base: u64,
    order: ByteOrder,
}

impl Tiff {
    fn u16(self, b: &[u8]) -> u16 {
        let b = [b[0], b[1]];
        match self.order {
            ByteOrder::Little => u16::from_le_bytes(b),
            ByteOrder::Big => u16::from_be_bytes(b),
        }
    }

    fn u32(self, b: &[u8]) -> u32 {
        let b = [b[0], b[1], b[2], b[3]];
        match self.order {
            ByteOrder::Little => u32::from_le_bytes(b),
            ByteOrder::Big => u32::from_be_bytes(b),
        }
    }

    /// Absolute offset of a relative one (cannot overflow: base and offset are both < 2^33).
    fn abs(self, rel: u32) -> Option<u64> {
        self.base.checked_add(u64::from(rel))
    }
}

/// What an IFD is, which decides the tags that mean something in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Main(u8),
    Sub(u8),
    Exif,
    MakerNote,
    Preview,
}

impl Kind {
    fn is_image(self) -> bool {
        matches!(self, Self::Main(_) | Self::Sub(_) | Self::Preview)
    }

    fn source(self) -> Option<CandidateSource> {
        match self {
            Self::Main(index) => Some(CandidateSource::Ifd { index }),
            Self::Sub(index) => Some(CandidateSource::SubIfd { index }),
            Self::Preview => Some(CandidateSource::MakerNotePreview),
            Self::Exif | Self::MakerNote => None,
        }
    }
}

/// One 12-byte IFD entry.
#[derive(Clone, Copy)]
struct Entry {
    tag: u16,
    kind: u16,
    count: u32,
    value: [u8; 4],
    /// Absolute offset of the entry, for issues.
    at: u64,
}

const TAG_NEW_SUBFILE_TYPE: u16 = 0x00FE;
const TAG_IMAGE_WIDTH: u16 = 0x0100;
const TAG_IMAGE_LENGTH: u16 = 0x0101;
const TAG_COMPRESSION: u16 = 0x0103;
const TAG_STRIP_OFFSETS: u16 = 0x0111;
const TAG_STRIP_BYTE_COUNTS: u16 = 0x0117;
const TAG_SUB_IFDS: u16 = 0x014A;
const TAG_JPEG_OFFSET: u16 = 0x0201;
const TAG_JPEG_LENGTH: u16 = 0x0202;
const TAG_EXIF_IFD: u16 = 0x8769;
const TAG_MAKER_NOTE: u16 = 0x927C;
/// In the Nikon MakerNote only.
const TAG_NIKON_PREVIEW_IFD: u16 = 0x0011;

const TYPE_SHORT: u16 = 3;
const TYPE_LONG: u16 = 4;
const TYPE_IFD: u16 = 13;

/// The tags of one IFD the walker cares about.
#[derive(Default)]
struct Fields {
    new_subfile_type: Option<u32>,
    width: Option<u32>,
    height: Option<u32>,
    compression: Option<u32>,
    strip_offset: Option<Entry>,
    strip_count: Option<Entry>,
    jpeg_offset: Option<u32>,
    jpeg_length: Option<u32>,
    sub_ifds: Option<Entry>,
    exif: Option<u32>,
    maker_note: Option<Entry>,
    preview_ifd: Option<u32>,
}

struct Walker<'a, R: ?Sized> {
    src: &'a mut R,
    len: u64,
    limits: &'a Limits,
    /// Absolute offsets of the IFDs read; at most `max_ifds` long.
    visited: Vec<u64>,
    /// Declared ranges of the candidates probed; at most `max_candidates` long.
    declared: Vec<ByteRange>,
    walk: Walk,
}

impl<R: ReadAt + ?Sized> Walker<'_, R> {
    fn issue(&mut self, offset: u64, kind: IssueKind) {
        if self.walk.issues.len() < self.limits.max_issues {
            self.walk.issues.push(Issue { offset, kind });
        } else {
            self.walk.issues_dropped += 1;
        }
    }

    /// `offset..offset + n` lies inside the source.
    fn fits(&self, offset: u64, n: u64) -> bool {
        offset.checked_add(n).is_some_and(|end| end <= self.len)
    }

    /// Walks the IFD at `rel` and, for chains (main and SubIFDs), the IFDs after it.
    fn walk_chain(&mut self, t: Tiff, rel: u32, depth: u8, kind: Kind) -> Result<(), WalkError> {
        let (mut rel, mut kind) = (rel, kind);
        while let Some((entries, next)) = self.read_ifd(t, rel, depth)? {
            self.visit(t, &entries, depth, kind)?;
            kind = match kind {
                Kind::Main(i) => Kind::Main(i.saturating_add(1)),
                Kind::Sub(i) => Kind::Sub(i),
                // The MakerNote and its PreviewIFD do not chain; Nikon leaves the pointer 0.
                Kind::Exif | Kind::MakerNote | Kind::Preview => return Ok(()),
            };
            rel = next;
        }
        Ok(())
    }

    /// Reads the IFD at `rel`: its entries and the next-IFD offset (0 when absent).
    /// `None` when there is nothing to read or the IFD is refused (with an issue).
    fn read_ifd(
        &mut self,
        t: Tiff,
        rel: u32,
        depth: u8,
    ) -> Result<Option<(Vec<Entry>, u32)>, WalkError> {
        if rel == 0 {
            return Ok(None);
        }
        let Some(at) = t.abs(rel) else {
            return Ok(None);
        };
        if depth > self.limits.max_depth {
            self.issue(at, IssueKind::TooDeep);
            return Ok(None);
        }
        if self.visited.contains(&at) {
            self.issue(at, IssueKind::IfdLoop);
            return Ok(None);
        }
        if self.visited.len() >= self.limits.max_ifds {
            self.issue(at, IssueKind::TooManyIfds);
            return Ok(None);
        }
        if !self.fits(at, 2) {
            self.issue(at, IssueKind::IfdOutOfBounds);
            return Ok(None);
        }
        let mut count = [0u8; 2];
        self.src.read_exact_at(at, &mut count)?;
        let count = t.u16(&count);
        if usize::from(count) > self.limits.max_entries {
            self.issue(at, IssueKind::TooManyEntries { count });
            return Ok(None);
        }
        let body = 12 * u64::from(count); // at most 12 * 65535
        let first = at + 2;
        if !self.fits(first, body) {
            self.issue(at, IssueKind::IfdOutOfBounds);
            return Ok(None);
        }
        // The next-IFD offset is read with the entries when it fits; a file cut right after
        // the entries still yields them.
        let with_next = self.fits(first, body + 4);
        let mut buf = vec![0u8; usize::from(count) * 12 + if with_next { 4 } else { 0 }];
        self.src.read_exact_at(first, &mut buf)?;
        self.visited.push(at);
        let entries = buf
            .chunks_exact(12)
            .zip(0u64..)
            .map(|(e, i)| Entry {
                tag: t.u16(&e[0..2]),
                kind: t.u16(&e[2..4]),
                count: t.u32(&e[4..8]),
                value: [e[8], e[9], e[10], e[11]],
                at: first + 12 * i,
            })
            .collect();
        let next = if with_next {
            t.u32(&buf[buf.len() - 4..])
        } else {
            0
        };
        Ok(Some((entries, next)))
    }

    fn visit(
        &mut self,
        t: Tiff,
        entries: &[Entry],
        depth: u8,
        kind: Kind,
    ) -> Result<(), WalkError> {
        let mut f = Fields::default();
        for &e in entries {
            match (kind, e.tag) {
                (k, TAG_NEW_SUBFILE_TYPE) if k.is_image() => {
                    f.new_subfile_type = self.first(t, e)?
                }
                (k, TAG_IMAGE_WIDTH) if k.is_image() => f.width = self.first(t, e)?,
                (k, TAG_IMAGE_LENGTH) if k.is_image() => f.height = self.first(t, e)?,
                (k, TAG_COMPRESSION) if k.is_image() => f.compression = self.first(t, e)?,
                (k, TAG_STRIP_OFFSETS) if k.is_image() => f.strip_offset = Some(e),
                (k, TAG_STRIP_BYTE_COUNTS) if k.is_image() => f.strip_count = Some(e),
                (k, TAG_JPEG_OFFSET) if k.is_image() => f.jpeg_offset = self.first(t, e)?,
                (k, TAG_JPEG_LENGTH) if k.is_image() => f.jpeg_length = self.first(t, e)?,
                (Kind::Main(_) | Kind::Sub(_), TAG_SUB_IFDS) => f.sub_ifds = Some(e),
                (Kind::Main(_), TAG_EXIF_IFD) => f.exif = self.first(t, e)?,
                (Kind::Exif, TAG_MAKER_NOTE) => f.maker_note = Some(e),
                (Kind::MakerNote, TAG_NIKON_PREVIEW_IFD) => f.preview_ifd = self.first(t, e)?,
                _ => {}
            }
        }

        if let Some(source) = kind.source() {
            self.collect(t, &f, source)?;
        }
        if let Some(e) = f.sub_ifds {
            let offsets = self.uints(t, e, self.limits.max_sub_ifds)?;
            if e.count as usize > self.limits.max_sub_ifds && !offsets.is_empty() {
                self.issue(e.at, IssueKind::TooManySubIfds { count: e.count });
            }
            for (i, rel) in offsets.into_iter().enumerate() {
                let index = u8::try_from(i).unwrap_or(u8::MAX);
                self.walk_chain(t, rel, depth.saturating_add(1), Kind::Sub(index))?;
            }
        }
        if let Some(rel) = f.exif {
            self.walk_chain(t, rel, depth.saturating_add(1), Kind::Exif)?;
        }
        if let Some(e) = f.maker_note {
            self.maker_note(t, e, depth.saturating_add(1))?;
        }
        if let Some(rel) = f.preview_ifd {
            self.walk_chain(t, rel, depth.saturating_add(1), Kind::Preview)?;
        }
        Ok(())
    }

    /// Adds the candidates an image IFD points at.
    fn collect(&mut self, t: Tiff, f: &Fields, source: CandidateSource) -> Result<(), WalkError> {
        if let (Some(offset), Some(length)) = (f.jpeg_offset, f.jpeg_length) {
            self.candidate(t, offset, length, f, source, JpegPointer::InterchangeFormat)?;
        }
        if !matches!(f.compression, Some(6 | 7)) {
            return Ok(());
        }
        if let (Some(so), Some(sc)) = (f.strip_offset, f.strip_count) {
            if so.count != 1 || sc.count != 1 {
                self.issue(
                    so.at,
                    IssueKind::MultiStrip {
                        count: so.count.max(sc.count),
                    },
                );
                return Ok(());
            }
            if let (Some(offset), Some(length)) = (self.first(t, so)?, self.first(t, sc)?) {
                self.candidate(t, offset, length, f, source, JpegPointer::Strip)?;
            }
        }
        Ok(())
    }

    fn candidate(
        &mut self,
        t: Tiff,
        rel: u32,
        length: u32,
        f: &Fields,
        source: CandidateSource,
        pointer: JpegPointer,
    ) -> Result<(), WalkError> {
        let Some(offset) = t.abs(rel) else {
            return Ok(());
        };
        let length = u64::from(length);
        // The same JPEG is often pointed at twice (0x0201 and a strip); probe it once.
        let declared = ByteRange { offset, length };
        if self.declared.contains(&declared) {
            return Ok(());
        }
        if self.declared.len() >= self.limits.max_candidates {
            self.issue(offset, IssueKind::TooManyCandidates);
            return Ok(());
        }
        self.declared.push(declared);
        let mut c = Candidate {
            range: declared,
            width: f.width,
            height: f.height,
            source,
            pointer,
            new_subfile_type: f.new_subfile_type,
            compression: f.compression,
            check: JpegCheck::Valid,
        };
        if !self.fits(offset, length) {
            c.check = JpegCheck::Invalid(Reject::OutOfBounds);
        } else {
            match jpeg::probe(&mut *self.src, c.range, self.limits)? {
                Ok(frame) => {
                    c.range.length = frame.length;
                    c.width = Some(frame.width);
                    c.height = Some(frame.height);
                }
                Err(reject) => c.check = JpegCheck::Invalid(reject),
            }
        }
        // A trimmed range can make two pointers to one JPEG equal; keep the first.
        if !self.walk.candidates.iter().any(|o| o.range == c.range) {
            self.walk.candidates.push(c);
        }
        Ok(())
    }

    /// Parses a Nikon type 3 MakerNote and walks its IFD.
    fn maker_note(&mut self, t: Tiff, e: Entry, depth: u8) -> Result<(), WalkError> {
        const HEADER: u64 = 18; // "Nikon\0", 4-byte version, 8-byte TIFF header
        if u64::from(e.count) < HEADER {
            self.issue(e.at, IssueKind::UnsupportedMakerNote);
            return Ok(());
        }
        let Some(at) = t.abs(t.u32(&e.value)) else {
            return Ok(());
        };
        if !self.fits(at, HEADER) {
            self.issue(e.at, IssueKind::ValueOutOfBounds { tag: e.tag });
            return Ok(());
        }
        let mut h = [0u8; HEADER as usize];
        self.src.read_exact_at(at, &mut h)?;
        // Type 3 (Nikon D-series and Z): version 02 xx. Types 1 and 2 hold no PreviewIFD.
        let tiff = (&h[..6] == b"Nikon\0" && h[6] == 0x02)
            .then(|| h[10..18].try_into().ok().and_then(parse_header))
            .flatten();
        let Some((order, ifd)) = tiff else {
            self.issue(at, IssueKind::UnsupportedMakerNote);
            return Ok(());
        };
        let mn = Tiff {
            base: at + 10,
            order,
        };
        self.walk_chain(mn, ifd, depth, Kind::MakerNote)
    }

    /// The first value of a SHORT, LONG or IFD entry (`None`, with an issue, otherwise).
    fn first(&mut self, t: Tiff, e: Entry) -> Result<Option<u32>, WalkError> {
        Ok(self.uints(t, e, 1)?.first().copied())
    }

    /// Up to `max` values of a SHORT, LONG or IFD entry; empty, with an issue, when the type
    /// is wrong, the count is 0 or the values lie outside the source.
    fn uints(&mut self, t: Tiff, e: Entry, max: usize) -> Result<Vec<u32>, WalkError> {
        let size: u64 = match e.kind {
            TYPE_SHORT => 2,
            TYPE_LONG | TYPE_IFD => 4,
            _ => 0,
        };
        if size == 0 || e.count == 0 || max == 0 {
            self.issue(e.at, IssueKind::BadValue { tag: e.tag });
            return Ok(Vec::new());
        }
        let n = u64::from(e.count).min(max as u64); // max is small (a Limits field)
        let total = size * u64::from(e.count); // < 2^35, no overflow
        let mut bytes = vec![0u8; (size * n) as usize];
        if total <= 4 {
            let n = bytes.len();
            bytes.copy_from_slice(&e.value[..n]);
        } else {
            let Some(at) = t.abs(t.u32(&e.value)) else {
                return Ok(Vec::new());
            };
            if !self.fits(at, size * n) {
                self.issue(e.at, IssueKind::ValueOutOfBounds { tag: e.tag });
                return Ok(Vec::new());
            }
            self.src.read_exact_at(at, &mut bytes)?;
        }
        Ok(if size == 2 {
            bytes.chunks_exact(2).map(|b| u32::from(t.u16(b))).collect()
        } else {
            bytes.chunks_exact(4).map(|b| t.u32(b)).collect()
        })
    }
}
