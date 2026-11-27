//! Checks that a byte range holds a JPEG a preview decoder can take, reading only its marker
//! segments up to the first scan header and its last bytes.

use std::io;

use super::{ByteRange, Limits, ReadAt, Reject};

/// What a valid JPEG declares.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Frame {
    /// Length up to and including EOI (trailing zero padding removed).
    pub length: u64,
    pub width: u32,
    pub height: u32,
    /// The SOFn marker: 0xC0 baseline, 0xC1 extended sequential, 0xC2 progressive.
    pub marker: u8,
    /// Components in the frame (Nf in the SOFn header).
    pub components: u8,
    /// Components in the first scan (Ns in the first SOS header).
    pub scan_components: u8,
}

/// Bytes read from the end to find EOI behind zero padding (so up to ~500 bytes of padding).
const TAIL: u64 = 512;

/// Checks the JPEG at `range`, which the caller has bounds-checked against the source.
/// The outer error is a read failure, the inner one a rejected candidate.
pub(super) fn probe<R: ReadAt + ?Sized>(
    src: &mut R,
    range: ByteRange,
    limits: &Limits,
) -> io::Result<Result<Frame, Reject>> {
    let ByteRange { offset, length } = range;
    if length < 4 {
        return Ok(Err(Reject::TooShort));
    }
    if length > limits.max_preview_bytes {
        return Ok(Err(Reject::TooLarge));
    }
    let mut soi = [0u8; 2];
    src.read_exact_at(offset, &mut soi)?;
    if soi != [0xFF, 0xD8] {
        return Ok(Err(Reject::NoSoi));
    }

    let k = length.min(TAIL);
    let mut tail = [0u8; TAIL as usize];
    let tail = &mut tail[..k as usize];
    src.read_exact_at(offset + length - k, tail)?;
    let kept = tail.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
    if kept < 2 || tail[kept - 2..kept] != [0xFF, 0xD9] {
        return Ok(Err(Reject::NoEoi));
    }
    let length = length - (k - kept as u64);
    if length < 4 {
        return Ok(Err(Reject::TooShort));
    }

    // Marker segments from just after SOI to the frame header, then on to the first scan
    // header. Every step reads inside `offset..offset + length` and moves forward, and the
    // number of steps for both is capped together.
    let end = offset + length;
    let mut pos = offset + 2;
    // The frame header once seen: (width, height, marker, components).
    let mut frame: Option<(u32, u32, u8, u8)> = None;
    for _ in 0..limits.max_jpeg_segments {
        if pos.checked_add(4).is_none_or(|p| p > end) {
            break;
        }
        let mut m = [0u8; 4];
        src.read_exact_at(pos, &mut m)?;
        if m[0] != 0xFF {
            break;
        }
        let marker = m[1];
        // An allowlist of the length-bearing markers that may come before the first scan, so
        // Culla and libjpeg see the same first scan (threat model T5). Anything else is a
        // broken file: SOI, EOI, TEM, RSTn, JPG, DNL, DHP, EXP, JPGn, reserved markers, and
        // `FF 00`, which libjpeg discards before scanning on byte by byte, so a scan header
        // hidden behind it would be skipped here and read there.
        match marker {
            // Fill byte before a marker, skipped as libjpeg does.
            0xFF => {
                pos += 1;
                continue;
            }
            // JPG (C8) is reserved, not a frame header.
            0xC8 => break,
            // APPn, COM, DQT, DHT, DRI, DAC, SOFn, SOS (refused below without a frame).
            0xE0..=0xEF | 0xFE | 0xDB | 0xC4 | 0xDD | 0xCC | 0xC0..=0xCF | 0xDA => {}
            _ => break,
        }
        let seg = u64::from(u16::from_be_bytes([m[2], m[3]]));
        if seg < 2 || pos + 2 + seg > end {
            break;
        }
        if marker == 0xDA {
            let Some((width, height, marker, components)) = frame else {
                break;
            };
            // Ns, then Ns component selectors of 2 bytes and 3 bytes of spectral selection
            // and approximation: the length is 6 + 2 * Ns, with 1 <= Ns <= min(4, Nf), as
            // libjpeg checks.
            if seg < 3 {
                break;
            }
            let mut ns = [0u8; 1];
            src.read_exact_at(pos + 4, &mut ns)?;
            let ns = ns[0];
            if ns == 0 || ns > 4 || ns > components || seg != 6 + 2 * u64::from(ns) {
                break;
            }
            return Ok(Ok(Frame {
                length,
                width,
                height,
                marker,
                components,
                scan_components: ns,
            }));
        }
        // SOFn: C0..CF except DHT (C4), JPG (C8) and DAC (CC).
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            // A second frame header is not a file a preview decoder takes.
            if frame.is_some() || seg < 8 {
                break;
            }
            let mut sof = [0u8; 6];
            src.read_exact_at(pos + 4, &mut sof)?;
            let precision = sof[0];
            let height = u32::from(u16::from_be_bytes([sof[1], sof[2]]));
            let width = u32::from(u16::from_be_bytes([sof[3], sof[4]]));
            let components = sof[5];
            // Baseline, extended sequential and progressive Huffman, 8-bit: what turbojpeg
            // decodes for previews. Lossless (C3) is how raw data is stored as JPEG.
            if !matches!(marker, 0xC0..=0xC2) || precision != 8 {
                return Ok(Err(Reject::UnsupportedFrame { marker, precision }));
            }
            if width == 0 || height == 0 {
                return Ok(Err(Reject::ZeroSize));
            }
            if u64::from(width) * u64::from(height) > limits.max_pixels {
                return Ok(Err(Reject::TooManyPixels));
            }
            // Nf, then 3 bytes per component: libjpeg refuses any other length.
            if components == 0 || seg != 8 + 3 * u64::from(components) {
                break;
            }
            frame = Some((width, height, marker, components));
        }
        pos += 2 + seg;
    }
    Ok(Err(if frame.is_some() {
        Reject::NoScan
    } else {
        Reject::NoFrame
    }))
}
