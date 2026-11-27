//! Checks that a byte range holds a JPEG a preview decoder can take, reading only its first
//! segments and its last bytes.

use std::io;

use super::{ByteRange, Limits, ReadAt, Reject};

/// What a valid JPEG declares.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Frame {
    /// Length up to and including EOI (trailing zero padding removed).
    pub length: u64,
    pub width: u32,
    pub height: u32,
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

    // Marker segments from just after SOI to the frame header. Every step reads inside
    // `offset..offset + length` and moves forward, and the number of steps is capped.
    let end = offset + length;
    let mut pos = offset + 2;
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
        match marker {
            // Fill byte before a marker.
            0xFF => {
                pos += 1;
                continue;
            }
            // SOI, EOI, SOS, TEM, RSTn: no frame header before them is a broken file.
            0xD8 | 0xD9 | 0xDA | 0x01 | 0xD0..=0xD7 => break,
            _ => {}
        }
        let seg = u64::from(u16::from_be_bytes([m[2], m[3]]));
        if seg < 2 || pos + 2 + seg > end {
            break;
        }
        // SOFn: C0..CF except DHT (C4), JPG (C8) and DAC (CC).
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            if seg < 8 {
                break;
            }
            let mut sof = [0u8; 5];
            src.read_exact_at(pos + 4, &mut sof)?;
            let precision = sof[0];
            let height = u32::from(u16::from_be_bytes([sof[1], sof[2]]));
            let width = u32::from(u16::from_be_bytes([sof[3], sof[4]]));
            // Baseline, extended sequential and progressive Huffman, 8-bit: what turbojpeg
            // decodes for previews. Lossless (C3) is how raw data is stored as JPEG.
            if !matches!(marker, 0xC0..=0xC2) || precision != 8 {
                return Ok(Err(Reject::UnsupportedFrame { marker, precision }));
            }
            if width == 0 || height == 0 {
                return Ok(Err(Reject::ZeroSize));
            }
            return Ok(Ok(Frame {
                length,
                width,
                height,
            }));
        }
        pos += 2 + seg;
    }
    Ok(Err(Reject::NoFrame))
}
