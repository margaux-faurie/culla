//! Walker tests on TIFFs built here by a small writer (never by the walker).

use super::*;

// ---------------------------------------------------------------------------------------------
// A minimal TIFF writer
// ---------------------------------------------------------------------------------------------

const SHORT: u16 = 3;
const LONG: u16 = 4;
const UNDEFINED: u16 = 7;

#[derive(Clone, Copy)]
struct E {
    tag: u16,
    kind: u16,
    count: u32,
    value: u32,
}

/// TIFF bytes with offsets relative to the start of `buf`.
struct Tiff {
    le: bool,
    buf: Vec<u8>,
}

impl Tiff {
    /// A header with IFD0 offset 0, patched by `ifd0`.
    fn new(le: bool) -> Self {
        let mut t = Self {
            le,
            buf: if le {
                b"II*\0".to_vec()
            } else {
                b"MM\0*".to_vec()
            },
        };
        t.buf.extend_from_slice(&[0; 4]);
        t
    }

    fn b16(&self, v: u16) -> [u8; 2] {
        if self.le {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        }
    }

    fn b32(&self, v: u32) -> [u8; 4] {
        if self.le {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        }
    }

    fn pos(&self) -> u32 {
        u32::try_from(self.buf.len()).unwrap()
    }

    /// Appends `data` at a word-aligned offset and returns that offset.
    fn push(&mut self, data: &[u8]) -> u32 {
        if self.buf.len() % 2 == 1 {
            self.buf.push(0);
        }
        let at = self.pos();
        self.buf.extend_from_slice(data);
        at
    }

    fn short(&self, tag: u16, v: u16) -> E {
        let b = self.b16(v);
        let value = u32::from_ne_bytes([b[0], b[1], 0, 0]);
        E {
            tag,
            kind: SHORT,
            count: 1,
            value,
        }
    }

    fn long(&self, tag: u16, v: u32) -> E {
        E {
            tag,
            kind: LONG,
            count: 1,
            value: u32::from_ne_bytes(self.b32(v)),
        }
    }

    /// An entry with `count` values stored at `offset`.
    fn at(&self, tag: u16, kind: u16, count: u32, offset: u32) -> E {
        E {
            tag,
            kind,
            count,
            value: u32::from_ne_bytes(self.b32(offset)),
        }
    }

    fn longs(&mut self, tag: u16, vs: &[u32]) -> E {
        if vs.len() == 1 {
            return self.long(tag, vs[0]);
        }
        let data: Vec<u8> = vs.iter().flat_map(|v| self.b32(*v)).collect();
        let at = self.push(&data);
        self.at(tag, LONG, u32::try_from(vs.len()).unwrap(), at)
    }

    /// Appends an IFD (entries sorted by tag) and returns its offset.
    fn ifd(&mut self, entries: &[E], next: u32) -> u32 {
        let mut entries = entries.to_vec();
        entries.sort_by_key(|e| e.tag);
        let mut data = self.b16(u16::try_from(entries.len()).unwrap()).to_vec();
        for e in &entries {
            data.extend_from_slice(&self.b16(e.tag));
            data.extend_from_slice(&self.b16(e.kind));
            data.extend_from_slice(&self.b32(e.count));
            data.extend_from_slice(&e.value.to_ne_bytes());
        }
        data.extend_from_slice(&self.b32(next));
        self.push(&data)
    }

    fn set_ifd0(&mut self, at: u32) {
        let b = self.b32(at);
        self.buf[4..8].copy_from_slice(&b);
    }

    /// An image IFD pointing at `jpeg` through 0x0201/0x0202.
    fn jpeg_ifd(&mut self, jpeg: &[u8], extra: &[E]) -> u32 {
        let at = self.push(jpeg);
        let mut e = vec![
            self.long(0x00FE, 1),
            self.short(0x0103, 6),
            self.long(0x0201, at),
            self.long(0x0202, len(jpeg)),
        ];
        e.extend_from_slice(extra);
        self.ifd(&e, 0)
    }
}

fn len(b: &[u8]) -> u32 {
    u32::try_from(b.len()).unwrap()
}

/// SOI, a frame header with `marker`, `precision`, width and height, a scan header, then EOI.
fn jpeg_with(marker: u8, precision: u8, w: u16, h: u16) -> Vec<u8> {
    let mut j = vec![0xFF, 0xD8];
    // An APP1 segment first, as Nikon's previews have.
    j.extend_from_slice(&[0xFF, 0xE1, 0x00, 0x06, b'E', b'x', b'i', b'f']);
    j.extend_from_slice(&[0xFF, marker, 0x00, 0x0B, precision]);
    j.extend_from_slice(&h.to_be_bytes());
    j.extend_from_slice(&w.to_be_bytes());
    j.extend_from_slice(&[0x01, 0x01, 0x11, 0x00]);
    // SOS: one component, tables 0/0, spectral 0..63; then stand-in entropy data and EOI.
    j.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00]);
    j.extend_from_slice(&[0x12, 0x34, 0xFF, 0xD9]);
    j
}

fn jpeg(w: u16, h: u16) -> Vec<u8> {
    jpeg_with(0xC0, 8, w, h)
}

/// A Nikon type 3 MakerNote: header, then a TIFF in its own byte order whose IFD has a
/// PreviewIFD (0x0011) pointing at `preview`.
fn maker_note(le: bool, preview: &[u8]) -> Vec<u8> {
    let mut mn = Tiff::new(le);
    let version = mn.short(0x0001, 0x0211);
    let pre = mn.jpeg_ifd(preview, &[]);
    let p = mn.long(0x0011, pre);
    let ifd = mn.ifd(&[version, p], 0);
    mn.set_ifd0(ifd);
    let mut out = b"Nikon\0\x02\x11\0\0".to_vec();
    out.extend_from_slice(&mn.buf);
    out
}

/// A NEF-shaped file: IFD0 thumbnail, SubIFD0 JPEG (0x0201), SubIFD1 strip JPEG, SubIFD2 raw
/// data, EXIF IFD with a Nikon MakerNote PreviewIFD in the other byte order.
struct Nef {
    bytes: Vec<u8>,
    thumb: ByteRange,
    full: ByteRange,
    strip: ByteRange,
    preview: ByteRange,
}

fn nef(le: bool) -> Nef {
    let mut t = Tiff::new(le);
    let full = jpeg(64, 48);
    let sub0 = t.jpeg_ifd(&full, &[]);
    let full_at = sub0_jpeg_offset(&t, sub0);
    let strip = jpeg(32, 32);
    let strip_at = t.push(&strip);
    let e1 = [
        t.long(0x00FE, 1),
        t.long(0x0100, 32),
        t.long(0x0101, 32),
        t.short(0x0103, 6),
        t.long(0x0111, strip_at),
        t.long(0x0117, len(&strip)),
    ];
    let sub1 = t.ifd(&e1, 0);
    let raw = vec![0xFFu8; 4096];
    let raw_at = t.push(&raw);
    let e2 = [
        t.long(0x00FE, 0),
        t.short(0x0102, 14),
        t.short(0x0103, 34713),
        t.long(0x0111, raw_at),
        t.long(0x0117, len(&raw)),
    ];
    let sub2 = t.ifd(&e2, 0);
    let preview = jpeg(40, 30);
    let mn = maker_note(!le, &preview);
    let mn_at = t.push(&mn);
    let mnote = t.at(0x927C, UNDEFINED, len(&mn), mn_at);
    let exif = t.ifd(&[mnote], 0);
    let subs = t.longs(0x014A, &[sub0, sub1, sub2]);
    let thumb = jpeg(16, 8);
    let ex = t.long(0x8769, exif);
    let ifd0 = t.jpeg_ifd(&thumb, &[subs, ex]);
    let thumb_at = sub0_jpeg_offset(&t, ifd0);
    t.set_ifd0(ifd0);
    // Preview offset: MakerNote TIFF starts 10 bytes into the MakerNote.
    let preview_at = find(&t.buf, &preview, u64::from(mn_at));
    Nef {
        thumb: ByteRange {
            offset: thumb_at.into(),
            length: thumb.len() as u64,
        },
        full: ByteRange {
            offset: full_at.into(),
            length: full.len() as u64,
        },
        strip: ByteRange {
            offset: strip_at.into(),
            length: strip.len() as u64,
        },
        preview: ByteRange {
            offset: preview_at,
            length: preview.len() as u64,
        },
        bytes: t.buf,
    }
}

/// The 0x0201 value of the IFD at `ifd` (written by `jpeg_ifd`, so entries are known).
fn sub0_jpeg_offset(t: &Tiff, ifd: u32) -> u32 {
    let ifd = ifd as usize;
    let rd16 = |at: usize| {
        let b = [t.buf[at], t.buf[at + 1]];
        if t.le {
            u16::from_le_bytes(b)
        } else {
            u16::from_be_bytes(b)
        }
    };
    let n = usize::from(rd16(ifd));
    (0..n)
        .map(|i| ifd + 2 + 12 * i)
        .find(|&e| rd16(e) == 0x0201)
        .map(|e| {
            let b = [t.buf[e + 8], t.buf[e + 9], t.buf[e + 10], t.buf[e + 11]];
            if t.le {
                u32::from_le_bytes(b)
            } else {
                u32::from_be_bytes(b)
            }
        })
        .unwrap()
}

fn find(hay: &[u8], needle: &[u8], from: u64) -> u64 {
    let from = from as usize;
    (from
        + hay[from..]
            .windows(needle.len())
            .position(|w| w == needle)
            .unwrap()) as u64
}

fn run(bytes: &[u8]) -> Result<Walk, WalkError> {
    walk(&mut &bytes[..], &Limits::DEFAULT)
}

fn valid(w: &Walk) -> Vec<(CandidateSource, ByteRange, u32, u32)> {
    w.candidates
        .iter()
        .filter(|c| c.is_valid())
        .map(|c| (c.source, c.range, c.width.unwrap(), c.height.unwrap()))
        .collect()
}

fn has_issue(w: &Walk, f: impl Fn(IssueKind) -> bool) -> bool {
    w.issues.iter().any(|i| f(i.kind))
}

// ---------------------------------------------------------------------------------------------
// Structure
// ---------------------------------------------------------------------------------------------

#[test]
fn finds_ifd0_jpeg_big_and_little_endian() {
    for le in [false, true] {
        let mut t = Tiff::new(le);
        let j = jpeg(16, 8);
        let ifd = t.jpeg_ifd(&j, &[]);
        t.set_ifd0(ifd);
        let w = run(&t.buf).unwrap();
        assert_eq!(
            w.order,
            if le {
                ByteOrder::Little
            } else {
                ByteOrder::Big
            }
        );
        assert_eq!(w.candidates.len(), 1);
        let c = w.best().unwrap();
        assert_eq!(c.source, CandidateSource::Ifd { index: 0 });
        assert_eq!(c.pointer, JpegPointer::InterchangeFormat);
        assert_eq!((c.width, c.height), (Some(16), Some(8)));
        assert_eq!(c.range.length, j.len() as u64);
        assert!(w.issues.is_empty(), "{:?}", w.issues);
    }
}

#[test]
fn nef_shape_yields_every_candidate_and_picks_the_largest() {
    for le in [false, true] {
        let n = nef(le);
        let w = run(&n.bytes).unwrap();
        assert!(w.issues.is_empty(), "{:?}", w.issues);
        assert_eq!(
            valid(&w),
            vec![
                (CandidateSource::Ifd { index: 0 }, n.thumb, 16, 8),
                (CandidateSource::SubIfd { index: 0 }, n.full, 64, 48),
                (CandidateSource::SubIfd { index: 1 }, n.strip, 32, 32),
                (CandidateSource::MakerNotePreview, n.preview, 40, 30),
            ]
        );
        // The raw strip (Compression 34713) is not a candidate.
        assert_eq!(w.candidates.len(), 4);
        let best = find_preview(&mut &n.bytes[..], &Limits::DEFAULT).unwrap();
        assert_eq!(best.range, n.full);
        assert_eq!(best.source, CandidateSource::SubIfd { index: 0 });
        assert_eq!(best.new_subfile_type, Some(1));
        assert_eq!(best.compression, Some(6));
        let strip = w
            .candidates
            .iter()
            .find(|c| c.pointer == JpegPointer::Strip)
            .unwrap();
        assert_eq!(strip.source, CandidateSource::SubIfd { index: 1 });
    }
}

#[test]
fn maker_note_offsets_are_relative_to_its_own_tiff_header() {
    // Both byte-order combinations, and the preview resolved to its absolute offset.
    for le in [false, true] {
        let n = nef(le);
        let w = run(&n.bytes).unwrap();
        let p = w
            .candidates
            .iter()
            .find(|c| c.source == CandidateSource::MakerNotePreview)
            .unwrap();
        assert_eq!(p.range, n.preview);
        let at = p.range.offset as usize;
        assert_eq!(&n.bytes[at..at + 2], &[0xFF, 0xD8]);
    }
}

#[test]
fn the_most_pixels_win_over_the_most_bytes() {
    let mut t = Tiff::new(true);
    let mut small = jpeg(100, 100);
    // Pad the small one with a big APP segment so it has more bytes.
    small.splice(
        2..2,
        [0xFF, 0xE2, 0x10, 0x02]
            .into_iter()
            .chain(vec![0u8; 0x1000]),
    );
    let big = jpeg(200, 100);
    let s0 = t.jpeg_ifd(&small, &[]);
    let s1 = t.jpeg_ifd(&big, &[]);
    let subs = t.longs(0x014A, &[s0, s1]);
    let ifd0 = t.ifd(&[subs], 0);
    t.set_ifd0(ifd0);
    let w = run(&t.buf).unwrap();
    assert!(w.candidates[0].range.length > w.candidates[1].range.length);
    assert_eq!(
        w.best().unwrap().source,
        CandidateSource::SubIfd { index: 1 }
    );
}

#[test]
fn ifd1_in_the_main_chain_is_walked() {
    let mut t = Tiff::new(false);
    let j = jpeg(8, 8);
    let ifd1 = t.jpeg_ifd(&j, &[]);
    let ifd0 = t.ifd(&[t.short(0x0103, 1)], ifd1);
    t.set_ifd0(ifd0);
    let w = run(&t.buf).unwrap();
    assert_eq!(w.best().unwrap().source, CandidateSource::Ifd { index: 1 });
}

#[test]
fn the_same_jpeg_pointed_at_twice_is_one_candidate() {
    let mut t = Tiff::new(true);
    let j = jpeg(8, 8);
    let at = t.push(&j);
    let e = [
        t.short(0x0103, 6),
        t.long(0x0111, at),
        t.long(0x0117, len(&j)),
        t.long(0x0201, at),
        t.long(0x0202, len(&j)),
    ];
    let sub = t.ifd(&e, 0);
    let e0 = [
        t.long(0x014A, sub),
        t.long(0x0201, at),
        t.long(0x0202, len(&j)),
    ];
    let ifd0 = t.ifd(&e0, 0);
    t.set_ifd0(ifd0);
    let w = run(&t.buf).unwrap();
    assert_eq!(w.candidates.len(), 1);
    assert_eq!(w.candidates[0].source, CandidateSource::Ifd { index: 0 });
}

#[test]
fn multi_strip_jpeg_is_reported_not_used() {
    let mut t = Tiff::new(true);
    let j = jpeg(8, 8);
    let at = t.push(&j);
    let so = t.longs(0x0111, &[at, at + 4]);
    let sc = t.longs(0x0117, &[4, len(&j) - 4]);
    let ifd0 = t.ifd(&[t.short(0x0103, 7), so, sc], 0);
    t.set_ifd0(ifd0);
    let w = run(&t.buf).unwrap();
    assert!(w.candidates.is_empty());
    assert!(has_issue(&w, |k| k == IssueKind::MultiStrip { count: 2 }));
}

#[test]
fn not_tiff_is_an_error() {
    let inputs: [&[u8]; 5] = [
        b"",
        b"II*\0",
        b"hello world",
        b"II+\0\x08\0\0\0",
        b"MM\0+\0\0\0\x08",
    ];
    for bytes in inputs {
        assert!(matches!(run(bytes), Err(WalkError::NotTiff)), "{bytes:?}");
    }
    let e = find_preview(&mut &b"II*\0\0\0\0\0"[..], &Limits::DEFAULT).unwrap_err();
    assert!(matches!(e, WalkError::NoPreview { candidates: 0 }));
    assert_eq!(e.to_string(), "no embedded JPEG found");
}

#[test]
fn unsupported_maker_note_is_skipped() {
    let headers: [&[u8]; 3] = [
        b"Nikon\0\x01\0",
        b"Canon",
        b"Nikon\0\x02\x11\0\0XX*\0\0\0\0\x08",
    ];
    for header in headers {
        let mut t = Tiff::new(true);
        let mut mn = header.to_vec();
        mn.resize(32, 0);
        let mn_at = t.push(&mn);
        let exif = t.ifd(&[t.at(0x927C, UNDEFINED, len(&mn), mn_at)], 0);
        let j = jpeg(8, 8);
        let ifd0 = t.jpeg_ifd(&j, &[t.long(0x8769, exif)]);
        t.set_ifd0(ifd0);
        let w = run(&t.buf).unwrap();
        assert!(
            has_issue(&w, |k| k == IssueKind::UnsupportedMakerNote),
            "{header:?}"
        );
        assert_eq!(w.best().unwrap().source, CandidateSource::Ifd { index: 0 });
    }
}

// ---------------------------------------------------------------------------------------------
// Hostile input
// ---------------------------------------------------------------------------------------------

#[test]
fn ifd_loops_end() {
    // IFD0 -> next IFD0; IFD0 -> SubIFD -> next IFD0; SubIFD -> SubIFD itself.
    let mut t = Tiff::new(false);
    let j = jpeg(8, 8);
    let jat = t.push(&j);
    let sub_at = t.push(&[]); // aligned, so the IFD lands here
    let ifd0_at = sub_at + 200; // where IFD0 will land after the SubIFD
    let sub = t.ifd(&[t.long(0x014A, sub_at), t.short(0x0103, 1)], ifd0_at);
    assert_eq!(sub, sub_at);
    t.buf.resize(ifd0_at as usize, 0);
    let e0 = [
        t.long(0x014A, sub),
        t.long(0x0201, jat),
        t.long(0x0202, len(&j)),
    ];
    let ifd0 = t.ifd(&e0, ifd0_at);
    assert_eq!(ifd0, ifd0_at);
    t.set_ifd0(ifd0);
    let w = run(&t.buf).unwrap();
    assert!(has_issue(&w, |k| k == IssueKind::IfdLoop));
    assert_eq!(w.candidates.len(), 1);
}

#[test]
fn depth_is_capped() {
    // A chain of SubIFDs nested 20 deep, the JPEG at the bottom.
    let mut t = Tiff::new(true);
    let j = jpeg(8, 8);
    let mut child = t.jpeg_ifd(&j, &[]);
    for _ in 0..20 {
        child = t.ifd(&[t.long(0x014A, child)], 0);
    }
    t.set_ifd0(child);
    let w = run(&t.buf).unwrap();
    assert!(has_issue(&w, |k| k == IssueKind::TooDeep));
    assert!(w.candidates.is_empty());
    assert!(w.issues.len() <= Limits::DEFAULT.max_issues);
}

#[test]
fn ifd_count_is_capped() {
    let mut t = Tiff::new(true);
    let mut next = 0;
    for _ in 0..500 {
        next = t.ifd(&[t.short(0x0103, 1)], next);
    }
    t.set_ifd0(next);
    let w = run(&t.buf).unwrap();
    assert!(has_issue(&w, |k| k == IssueKind::TooManyIfds));
}

#[test]
fn many_sub_ifds_are_capped_and_reported() {
    let mut t = Tiff::new(true);
    let j = jpeg(8, 8);
    let sub = t.jpeg_ifd(&j, &[]);
    let subs = t.longs(0x014A, &[sub; 1000]);
    let ifd0 = t.ifd(&[subs], 0);
    t.set_ifd0(ifd0);
    let w = run(&t.buf).unwrap();
    assert!(has_issue(&w, |k| k == IssueKind::TooManySubIfds { count: 1000 }));
    assert!(has_issue(&w, |k| k == IssueKind::IfdLoop));
    assert_eq!(w.candidates.len(), 1);
}

#[test]
fn huge_entry_counts_are_refused() {
    let mut t = Tiff::new(false);
    let at = t.push(&[0xFF, 0xFF]); // IFD claiming 65535 entries, none present
    t.set_ifd0(at);
    let w = run(&t.buf).unwrap();
    assert!(has_issue(&w, |k| k == IssueKind::TooManyEntries { count: 0xFFFF }));

    // 1000 entries declared but the file ends: out of bounds.
    let mut t = Tiff::new(false);
    let at = t.push(&1000u16.to_be_bytes());
    t.set_ifd0(at);
    let w = run(&t.buf).unwrap();
    assert!(has_issue(&w, |k| k == IssueKind::IfdOutOfBounds));
}

#[test]
fn huge_value_counts_and_offsets_past_the_end() {
    let mut t = Tiff::new(true);
    let e = [
        t.at(0x014A, LONG, u32::MAX, 16),       // SubIFDs: 4 * 2^32 bytes
        t.at(0x0111, LONG, u32::MAX, u32::MAX), // strips everywhere
        t.at(0x0117, LONG, u32::MAX, u32::MAX),
        t.short(0x0103, 6),
        t.long(0x0201, u32::MAX), // JPEG offset past the end
        t.long(0x0202, u32::MAX),
        t.long(0x8769, u32::MAX), // EXIF IFD past the end
    ];
    let ifd0 = t.ifd(&e, u32::MAX);
    t.set_ifd0(ifd0);
    let w = run(&t.buf).unwrap();
    assert_eq!(w.candidates.len(), 1);
    assert_eq!(
        w.candidates[0].check,
        JpegCheck::Invalid(Reject::OutOfBounds)
    );
    assert!(w.best().is_none());
    assert!(has_issue(&w, |k| matches!(k, IssueKind::IfdOutOfBounds)));
    assert!(has_issue(&w, |k| matches!(k, IssueKind::MultiStrip { .. })));
}

#[test]
fn ifd0_offset_past_the_end() {
    let mut t = Tiff::new(true);
    t.set_ifd0(u32::MAX);
    let w = run(&t.buf).unwrap();
    assert!(w.candidates.is_empty());
    assert!(has_issue(&w, |k| k == IssueKind::IfdOutOfBounds));
}

#[test]
fn bad_value_types_are_reported() {
    let mut t = Tiff::new(true);
    let e = [
        E {
            tag: 0x0201,
            kind: 2,
            count: 4,
            value: 0,
        }, // ASCII offset
        E {
            tag: 0x0202,
            kind: LONG,
            count: 0,
            value: 0,
        },
    ];
    let ifd0 = t.ifd(&e, 0);
    t.set_ifd0(ifd0);
    let w = run(&t.buf).unwrap();
    assert!(has_issue(&w, |k| k == IssueKind::BadValue { tag: 0x0201 }));
    assert!(has_issue(&w, |k| k == IssueKind::BadValue { tag: 0x0202 }));
}

#[test]
fn every_truncation_of_a_nef_is_handled() {
    for le in [false, true] {
        let n = nef(le);
        let mut lost_full = false;
        for cut in 0..n.bytes.len() {
            let bytes = &n.bytes[..cut];
            match run(bytes) {
                Err(WalkError::NotTiff) => assert!(cut < 8),
                Err(e) => panic!("cut {cut}: {e}"),
                Ok(w) => {
                    for c in w.candidates.iter().filter(|c| c.is_valid()) {
                        assert!(c.range.end().unwrap() <= cut as u64);
                    }
                    lost_full |= w.best().map(|b| b.range) != Some(n.full);
                }
            }
        }
        assert!(lost_full);
    }
}

#[test]
fn jpeg_checks() {
    let probe = |j: &[u8], limits: &Limits| {
        let mut t = Tiff::new(true);
        let ifd = t.jpeg_ifd(j, &[]);
        t.set_ifd0(ifd);
        let w = walk(&mut &t.buf[..], limits).unwrap();
        (w.candidates[0].check, w.candidates[0].range.length)
    };
    let l = Limits::DEFAULT;
    let ok = jpeg(8, 8);
    assert_eq!(probe(&ok, &l), (JpegCheck::Valid, ok.len() as u64));

    let inv = |r| JpegCheck::Invalid(r);
    assert_eq!(probe(&[0xFF, 0xD8, 0xFF], &l).0, inv(Reject::TooShort));
    assert_eq!(probe(&ok[1..], &l).0, inv(Reject::NoSoi));
    assert_eq!(probe(&ok[..ok.len() - 1], &l).0, inv(Reject::NoEoi));

    // Zero padding after EOI is trimmed, up to the tail window.
    let mut padded = ok.clone();
    padded.extend_from_slice(&[0; 100]);
    assert_eq!(probe(&padded, &l), (JpegCheck::Valid, ok.len() as u64));
    padded.extend_from_slice(&[0; 600]);
    assert_eq!(probe(&padded, &l).0, inv(Reject::NoEoi));
    // Non-zero bytes after EOI are not padding.
    let mut junk = ok.clone();
    junk.push(0x42);
    assert_eq!(probe(&junk, &l).0, inv(Reject::NoEoi));

    assert_eq!(probe(&jpeg_with(0xC2, 8, 8, 8), &l).0, JpegCheck::Valid);
    assert_eq!(probe(&jpeg_with(0xC1, 8, 8, 8), &l).0, JpegCheck::Valid);
    assert_eq!(
        probe(&jpeg_with(0xC3, 14, 8, 8), &l).0,
        inv(Reject::UnsupportedFrame {
            marker: 0xC3,
            precision: 14
        })
    );
    assert_eq!(
        probe(&jpeg_with(0xC1, 12, 8, 8), &l).0,
        inv(Reject::UnsupportedFrame {
            marker: 0xC1,
            precision: 12
        })
    );
    assert_eq!(
        probe(&jpeg_with(0xC0, 8, 0, 8), &l).0,
        inv(Reject::ZeroSize)
    );
    // A tiny JPEG claiming a 4.3 GP frame never becomes a candidate (T5).
    assert_eq!(
        probe(&jpeg_with(0xC0, 8, u16::MAX, u16::MAX), &l).0,
        inv(Reject::TooManyPixels)
    );
    let few = Limits {
        max_pixels: 63,
        ..l
    };
    assert_eq!(probe(&jpeg(8, 8), &few).0, inv(Reject::TooManyPixels));
    let exact = Limits {
        max_pixels: 64,
        ..l
    };
    assert_eq!(probe(&jpeg(8, 8), &exact).0, JpegCheck::Valid);

    // SOS before any frame header.
    assert_eq!(
        probe(&[0xFF, 0xD8, 0xFF, 0xDA, 0, 2, 0xFF, 0xD9], &l).0,
        inv(Reject::NoFrame)
    );
    // A segment length running past the end.
    assert_eq!(
        probe(&[0xFF, 0xD8, 0xFF, 0xE1, 0xFF, 0xFF, 0xFF, 0xD9], &l).0,
        inv(Reject::NoFrame)
    );
    // A frame header but no scan header before EOI.
    let mut no_sos = jpeg(8, 8);
    let sos = no_sos.windows(2).position(|w| w == [0xFF, 0xDA]).unwrap();
    no_sos.drain(sos..no_sos.len() - 2);
    assert_eq!(probe(&no_sos, &l).0, inv(Reject::NoScan));
    // A scan header with more components than the frame, or a length that does not match Ns.
    let mut bad_ns = jpeg(8, 8);
    bad_ns[sos + 4] = 2;
    assert_eq!(probe(&bad_ns, &l).0, inv(Reject::NoScan));
    let mut bad_len = jpeg(8, 8);
    bad_len[sos + 3] = 0x0A;
    assert_eq!(probe(&bad_len, &l).0, inv(Reject::NoScan));
    // Ns > Nf alone: two components in the scan, its length matching (6 + 2 * 2), Nf = 1.
    let mut ns_over_nf = jpeg(8, 8);
    ns_over_nf.splice(
        sos..sos + 10,
        [
            0xFF, 0xDA, 0x00, 0x0A, 0x02, 0x01, 0x00, 0x01, 0x00, 0x00, 0x3F, 0x00,
        ],
    );
    assert_eq!(probe(&ns_over_nf, &l).0, inv(Reject::NoScan));
    // `FF 00 <len>` is not a segment: libjpeg discards `FF 00` and rescans byte by byte, so
    // whatever it hides would be read there and skipped here. Refused before and after SOF.
    let hidden = [0xFF, 0x00, 0x00, 0x04, 0x00, 0x00];
    let mut before_sof = jpeg(8, 8);
    let sof = before_sof
        .windows(2)
        .position(|w| w == [0xFF, 0xC0])
        .unwrap();
    before_sof.splice(sof..sof, hidden);
    assert_eq!(probe(&before_sof, &l).0, inv(Reject::NoFrame));
    let mut after_sof = jpeg(8, 8);
    after_sof.splice(sos..sos, hidden);
    assert_eq!(probe(&after_sof, &l).0, inv(Reject::NoScan));
    // Other markers outside the allowlist: TEM, RST0, JPG, DNL, DHP, EXP, JPG0, reserved.
    for m in [0x01, 0xD0, 0xC8, 0xDC, 0xDE, 0xDF, 0xF0, 0x02, 0xBF] {
        let mut j = jpeg(8, 8);
        j.splice(sos..sos, [0xFF, m, 0x00, 0x04, 0x00, 0x00]);
        assert_eq!(probe(&j, &l).0, inv(Reject::NoScan), "marker {m:#04x}");
    }
    // Allowed markers between SOF and SOS: DQT, DHT, DRI, DAC, COM, APP15, and fill bytes.
    for seg in [
        &[0xFF, 0xDB, 0x00, 0x02][..],
        &[0xFF, 0xC4, 0x00, 0x02],
        &[0xFF, 0xDD, 0x00, 0x04, 0x00, 0x00],
        &[0xFF, 0xCC, 0x00, 0x02],
        &[0xFF, 0xFE, 0x00, 0x03, b'x'],
        &[0xFF, 0xFF, 0xFF, 0xEF, 0x00, 0x02],
    ] {
        let mut j = jpeg(8, 8);
        j.splice(sos..sos, seg.iter().copied());
        assert_eq!(probe(&j, &l).0, JpegCheck::Valid, "{seg:02x?}");
    }
    // A frame header whose length does not match Nf.
    let mut bad_nf = jpeg(8, 8);
    let sof = bad_nf.windows(2).position(|w| w == [0xFF, 0xC0]).unwrap();
    bad_nf[sof + 9] = 3;
    assert_eq!(probe(&bad_nf, &l).0, inv(Reject::NoFrame));
    // A flood of fill bytes hits the segment cap.
    let mut flood = vec![0xFF, 0xD8];
    flood.extend_from_slice(&[0xFF; 1000]);
    flood.extend_from_slice(&jpeg(8, 8)[2..]);
    assert_eq!(probe(&flood, &l).0, inv(Reject::NoFrame));

    let tight = Limits {
        max_preview_bytes: 10,
        ..l
    };
    assert_eq!(probe(&ok, &tight).0, inv(Reject::TooLarge));
}

#[test]
fn read_errors_are_fatal() {
    struct Failing(u64);
    impl ReadAt for Failing {
        fn len(&self) -> u64 {
            self.0
        }
        fn read_exact_at(&mut self, _: u64, _: &mut [u8]) -> io::Result<()> {
            Err(io::Error::other("device gone"))
        }
    }
    let e = walk(&mut Failing(1 << 20), &Limits::DEFAULT).unwrap_err();
    assert!(matches!(e, WalkError::Read(_)));
    assert!(std::error::Error::source(&e).is_some());
}

/// Records every read, to check the walker stays inside the source and skips raw data.
struct Counting<'a> {
    bytes: &'a [u8],
    reads: usize,
    read_bytes: u64,
}

impl ReadAt for Counting<'_> {
    fn len(&self) -> u64 {
        self.bytes.len() as u64
    }
    fn read_exact_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        assert!(offset + buf.len() as u64 <= self.len(), "read past the end");
        self.reads += 1;
        self.read_bytes += buf.len() as u64;
        let mut s = self.bytes;
        s.read_exact_at(offset, buf)
    }
}

#[test]
fn reads_only_headers_not_image_data() {
    let mut t = Tiff::new(true);
    // A 1 MiB JPEG: a 64 KiB APP segment repeated 16 times, then the frame.
    let mut j = vec![0xFF, 0xD8];
    for _ in 0..16 {
        j.extend_from_slice(&[0xFF, 0xE2, 0xFF, 0xFF]);
        j.extend_from_slice(&vec![0u8; 0xFFFD]);
    }
    j.extend_from_slice(&jpeg(4000, 3000)[2..]);
    let sub = t.jpeg_ifd(&j, &[]);
    let raw = vec![0u8; 8 << 20];
    let raw_at = t.push(&raw);
    let raw_ifd = t.ifd(
        &[
            t.short(0x0103, 34713),
            t.long(0x0111, raw_at),
            t.long(0x0117, len(&raw)),
        ],
        0,
    );
    let subs = t.longs(0x014A, &[sub, raw_ifd]);
    let ifd0 = t.ifd(&[subs], 0);
    t.set_ifd0(ifd0);
    let mut src = Counting {
        bytes: &t.buf,
        reads: 0,
        read_bytes: 0,
    };
    let best = find_preview(&mut src, &Limits::DEFAULT).unwrap();
    assert_eq!((best.width, best.height), (Some(4000), Some(3000)));
    assert!(src.read_bytes < 1024, "read {} bytes", src.read_bytes);
    assert!(src.reads < 40, "{} reads", src.reads);
}

/// Deterministic pseudo-random mutations of the NEF shape: no panic, every valid candidate
/// in bounds. A stand-in for property tests without a dependency; the fuzz target goes further.
#[test]
fn mutated_nefs_never_panic() {
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let seeds = [nef(false).bytes, nef(true).bytes];
    for i in 0..20_000 {
        let mut b = seeds[i % 2].clone();
        let n = b.len() as u64;
        match next() % 4 {
            0 => {
                for _ in 0..1 + next() % 8 {
                    let at = (next() % n) as usize;
                    b[at] = next() as u8;
                }
            }
            1 => {
                // A random 32-bit value at a random offset: offsets, counts and types.
                let at = (next() % (n - 4)) as usize;
                b[at..at + 4].copy_from_slice(&(next() as u32).to_ne_bytes());
            }
            2 => {
                // A small value, likely an in-file offset or a loop.
                let at = (next() % (n - 4)) as usize;
                let v = (next() % n) as u32;
                b[at..at + 4].copy_from_slice(&v.to_be_bytes());
            }
            _ => b.truncate((next() % n) as usize),
        }
        if let Ok(w) = run(&b) {
            for c in w.candidates.iter().filter(|c| c.is_valid()) {
                assert!(c.range.end().unwrap() <= b.len() as u64);
                assert!(c.pixels() > 0);
            }
            assert!(w.candidates.len() <= Limits::DEFAULT.max_candidates);
            assert!(w.issues.len() <= Limits::DEFAULT.max_issues);
        }
    }
}
