//! Preview sizing and orientation (ADR 0012, US-07): pure arithmetic and pixel moves.
//!
//! The adapters' `preview` module decodes a JPEG with turbojpeg's DCT-domain scaling, resizes
//! it with `fast_image_resize` and encodes it again. This module decides the numbers it uses,
//! before any decoder sees the bytes:
//!
//! 1. [`check_frame`]: the JPEG's own frame header, read by Culla's bounds-checked parser
//!    (`ifd::check_jpeg`), must declare at most [`Limits::max_pixels`] (threat model T5).
//! 2. [`plan`]: the final size fits a [`PreviewBox`] (the G14's 2880x1800 by default) in the
//!    photo's own orientation, never upscaled; the decode scale is the smallest `M/8` whose
//!    output still covers that size, so the resize only ever shrinks, a little.
//! 3. [`orient`]: turns the resized pixels upright for EXIF orientations 1 to 8.
//!
//! On a 3:2 photo the preview is 2700x1800 (1800x2700 for a portrait): it fills the G14's
//! screen height exactly. A 24 MP (6048x4032) JPEG decodes at 1/2 (3024x2016) and a 45 MP
//! (8256x5504) one at 3/8 (3096x2064), as ADR 0012 expects.

#[cfg(test)]
mod tests;

use std::fmt;

use crate::exif::Orientation;
use crate::ifd::{self, JpegFrame, Reject};

/// JPEG quality of the encoded preview (ADR 0012: q85–90).
pub const QUALITY: u8 = 88;

/// Bytes per pixel of the decode and resize buffers (RGB, 8-bit).
pub const CHANNELS: usize = 3;

/// A width and height in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Width times height.
    #[must_use]
    pub fn pixels(self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    /// Width and height swapped.
    #[must_use]
    pub fn transposed(self) -> Self {
        Self::new(self.height, self.width)
    }

    /// The longer side.
    #[must_use]
    pub fn long_edge(self) -> u32 {
        self.width.max(self.height)
    }

    /// The shorter side.
    #[must_use]
    pub fn short_edge(self) -> u32 {
        self.width.min(self.height)
    }

    /// The size once shown upright under `orientation`.
    #[must_use]
    pub fn oriented(self, orientation: Orientation) -> Self {
        if orientation.swaps_dimensions() {
            self.transposed()
        } else {
            self
        }
    }

    /// Bytes of an RGB buffer of this size, `None` on overflow.
    #[must_use]
    pub fn rgb_bytes(self) -> Option<usize> {
        usize::try_from(self.pixels()).ok()?.checked_mul(CHANNELS)
    }
}

/// The box a preview must fit in, given as a long and a short edge so it applies to landscape
/// and portrait photos alike.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PreviewBox {
    long: u32,
    short: u32,
}

impl PreviewBox {
    /// The reference screen: ROG Zephyrus G14 2024, 2880x1800 (SPEC §2, nfr.md).
    pub const G14: Self = Self {
        long: 2880,
        short: 1800,
    };

    /// A box of `a` by `b` pixels, in either order. `None` if either is zero.
    #[must_use]
    pub fn new(a: u32, b: u32) -> Option<Self> {
        (a > 0 && b > 0).then(|| Self {
            long: a.max(b),
            short: a.min(b),
        })
    }

    #[must_use]
    pub fn long_edge(self) -> u32 {
        self.long
    }

    #[must_use]
    pub fn short_edge(self) -> u32 {
        self.short
    }
}

impl Default for PreviewBox {
    fn default() -> Self {
        Self::G14
    }
}

/// A DCT scaling factor `num/denom`, kept reduced (libjpeg-turbo matches factors exactly, so
/// 4/8 must be passed as 1/2). Only the downscaling factors `M/8`, `M` in 1..=8, are made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Scale {
    num: u8,
    denom: u8,
}

impl Scale {
    /// No scaling.
    pub const ONE: Self = Self { num: 1, denom: 1 };

    /// `m/8` reduced, for `m` in 1..=8.
    #[must_use]
    pub fn eighths(m: u8) -> Option<Self> {
        if !(1..=8).contains(&m) {
            return None;
        }
        let g = gcd(m, 8);
        Some(Self {
            num: m / g,
            denom: 8 / g,
        })
    }

    #[must_use]
    pub fn num(self) -> u8 {
        self.num
    }

    #[must_use]
    pub fn denom(self) -> u8 {
        self.denom
    }

    /// A side after scaling, rounded up as libjpeg-turbo does (`TJSCALED`).
    #[must_use]
    pub fn apply(self, side: u32) -> u32 {
        let (n, d) = (u64::from(self.num), u64::from(self.denom));
        // At most u32::MAX * 1 / 1, since num <= denom.
        u32::try_from((u64::from(side) * n).div_ceil(d)).unwrap_or(u32::MAX)
    }

    /// A size after scaling.
    #[must_use]
    pub fn apply_size(self, size: Size) -> Size {
        Size::new(self.apply(size.width), self.apply(size.height))
    }
}

fn gcd(mut a: u8, mut b: u8) -> u8 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// Caps checked before a decode (threat model T5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Largest JPEG accepted, in bytes. A 45 MP camera JPEG is 15–25 MB.
    pub max_bytes: u64,
    /// Largest declared pixel count (frame header). The largest Nikon Z bodies are ~46 MP.
    pub max_pixels: u64,
    /// Largest declared pixel count for a multi-scan frame ([`JpegFrame::multi_scan`]:
    /// progressive, or non-interleaved). libjpeg-turbo keeps such a JPEG's whole coefficient
    /// buffer in memory (2 bytes per sample: about 3 bytes per pixel at 4:2:0 and 6 at 4:4:4,
    /// whatever the DCT scale), so a multi-scan file costs far more than a single-scan one of
    /// the same size. Nikon bodies write interleaved baseline JPEGs.
    pub max_multi_scan_pixels: u64,
}

/// [`Limits::max_multi_scan_pixels`] in the app: 25 MP, a provisional safe default (about
/// 150 MB of coefficients at worst), open for Margaux in SPEC §10.
pub const MAX_MULTI_SCAN_PIXELS: u64 = 25_000_000;

impl Limits {
    /// The app's limits: the same caps as the NEF walker's ([`ifd::Limits::DEFAULT`]).
    pub const DEFAULT: Self = Self {
        max_bytes: ifd::Limits::DEFAULT.max_preview_bytes,
        max_pixels: ifd::Limits::DEFAULT.max_pixels,
        max_multi_scan_pixels: MAX_MULTI_SCAN_PIXELS,
    };
}

impl Default for Limits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Why a preview cannot be planned.
///
/// Hand-written, like `ifd::WalkError`: core takes no dependency yet (no `thiserror`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanError {
    /// The bytes are not a JPEG the decoder may take ([`ifd::check_jpeg`]).
    Rejected(Reject),
    /// The declared size has a zero side.
    ZeroSize,
    /// The declared size is over [`Limits::max_pixels`].
    TooManyPixels { pixels: u64, cap: u64 },
    /// A multi-scan frame (progressive or non-interleaved) declares more than
    /// [`Limits::max_multi_scan_pixels`].
    MultiScanTooManyPixels { pixels: u64, cap: u64 },
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected(r) => write!(f, "not a usable JPEG: {r:?}"),
            Self::ZeroSize => f.write_str("the JPEG declares a zero width or height"),
            Self::TooManyPixels { pixels, cap } => {
                write!(f, "the JPEG declares {pixels} pixels, over the {cap} cap")
            }
            Self::MultiScanTooManyPixels { pixels, cap } => write!(
                f,
                "the multi-scan (progressive or non-interleaved) JPEG declares {pixels} pixels, over the {cap} cap for multi-scan files"
            ),
        }
    }
}

impl std::error::Error for PlanError {}

/// The numbers one preview is made with. Sizes before [`Plan::output`] are in the stored
/// orientation (as decoded); `output` is upright.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Plan {
    /// As declared by the JPEG's frame header.
    pub source: Size,
    pub orientation: Orientation,
    /// The DCT scale passed to the decoder.
    pub scale: Scale,
    /// What the decoder produces: `scale` applied to `source`.
    pub decoded: Size,
    /// After the resize, still in the stored orientation.
    pub resized: Size,
    /// The preview as shown: `resized` turned upright.
    pub output: Size,
}

impl Plan {
    /// True when the decoder's output must be resized.
    #[must_use]
    pub fn needs_resize(&self) -> bool {
        self.decoded != self.resized
    }
}

/// The largest size with the aspect ratio of `upright` that fits `target`, never larger than
/// `upright` itself (no upscaling). Each side is at least 1.
#[must_use]
pub fn fit(upright: Size, target: PreviewBox) -> Size {
    let (bl, bs) = (u64::from(target.long), u64::from(target.short));
    let (sl, ss) = (
        u64::from(upright.long_edge()),
        u64::from(upright.short_edge()),
    );
    if sl == 0 || ss == 0 || (sl <= bl && ss <= bs) {
        return upright;
    }
    // Limited by the long edge when bl / sl <= bs / ss.
    let (long, short) = if bl * ss <= bs * sl {
        (bl, (ss * bl + sl / 2) / sl)
    } else {
        ((sl * bs + ss / 2) / ss, bs)
    };
    // Both are at most the box's sides, so they fit in u32.
    let long = u32::try_from(long.max(1)).unwrap_or(u32::MAX);
    let short = u32::try_from(short.max(1)).unwrap_or(u32::MAX);
    if upright.width >= upright.height {
        Size::new(long, short)
    } else {
        Size::new(short, long)
    }
}

/// The smallest `M/8` whose output covers `want` on both sides. `want` must be at most
/// `source` on each side; [`Scale::ONE`] otherwise.
#[must_use]
pub fn scale_for(source: Size, want: Size) -> Scale {
    (1..=8)
        .filter_map(Scale::eighths)
        .find(|s| {
            let d = s.apply_size(source);
            d.width >= want.width && d.height >= want.height
        })
        .unwrap_or(Scale::ONE)
}

/// Checks a declared size against `limits`.
///
/// # Errors
///
/// [`PlanError::ZeroSize`] or [`PlanError::TooManyPixels`].
pub fn check_size(source: Size, limits: &Limits) -> Result<(), PlanError> {
    if source.width == 0 || source.height == 0 {
        return Err(PlanError::ZeroSize);
    }
    if source.pixels() > limits.max_pixels {
        return Err(PlanError::TooManyPixels {
            pixels: source.pixels(),
            cap: limits.max_pixels,
        });
    }
    Ok(())
}

/// The plan for a JPEG of `source` pixels (stored orientation) shown under `orientation`
/// in `target`.
///
/// # Errors
///
/// As [`check_size`].
pub fn plan(
    source: Size,
    orientation: Orientation,
    target: PreviewBox,
    limits: &Limits,
) -> Result<Plan, PlanError> {
    check_size(source, limits)?;
    let output = fit(source.oriented(orientation), target);
    let resized = output.oriented(orientation);
    let scale = scale_for(source, resized);
    Ok(Plan {
        source,
        orientation,
        scale,
        decoded: scale.apply_size(source),
        resized,
        output,
    })
}

/// Checks the JPEG bytes with Culla's own parser ([`ifd::check_jpeg`]): size cap, SOI, EOI,
/// an 8-bit baseline, extended or progressive frame, a scan header, the pixel cap, and the
/// lower cap for a multi-scan frame. Run before any decoder.
///
/// # Errors
///
/// [`PlanError::Rejected`], [`PlanError::MultiScanTooManyPixels`], or as [`check_size`].
pub fn check_frame(jpeg: &[u8], limits: &Limits) -> Result<JpegFrame, PlanError> {
    let walker_limits = ifd::Limits {
        max_preview_bytes: limits.max_bytes,
        max_pixels: limits.max_pixels,
        ..ifd::Limits::DEFAULT
    };
    let frame = match ifd::check_jpeg(jpeg, &walker_limits) {
        Ok(frame) => frame,
        Err(Reject::ZeroSize) => return Err(PlanError::ZeroSize),
        Err(Reject::TooManyPixels) => {
            return Err(PlanError::TooManyPixels {
                pixels: declared_pixels(jpeg).unwrap_or(0),
                cap: limits.max_pixels,
            });
        }
        Err(r) => return Err(PlanError::Rejected(r)),
    };
    let size = Size::new(frame.width, frame.height);
    check_size(size, limits)?;
    if frame.multi_scan && size.pixels() > limits.max_multi_scan_pixels {
        return Err(PlanError::MultiScanTooManyPixels {
            pixels: size.pixels(),
            cap: limits.max_multi_scan_pixels,
        });
    }
    Ok(frame)
}

/// The pixel count of a JPEG the walker refused for its size, for the error message only.
fn declared_pixels(jpeg: &[u8]) -> Option<u64> {
    let unlimited = ifd::Limits {
        max_preview_bytes: u64::MAX,
        max_pixels: u64::MAX,
        ..ifd::Limits::DEFAULT
    };
    let f = ifd::check_jpeg(jpeg, &unlimited).ok()?;
    Some(u64::from(f.width) * u64::from(f.height))
}

/// [`check_frame`] then [`plan`]: the plan for these JPEG bytes.
///
/// # Errors
///
/// As [`check_frame`].
pub fn plan_for_jpeg(
    jpeg: &[u8],
    orientation: Orientation,
    target: PreviewBox,
    limits: &Limits,
) -> Result<Plan, PlanError> {
    let frame = check_frame(jpeg, limits)?;
    plan(
        Size::new(frame.width, frame.height),
        orientation,
        target,
        limits,
    )
}

/// Why [`orient`] could not turn a buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrientError {
    /// The buffer length is not `width * height * channels`, or `channels` is 0.
    WrongLength,
    /// The output buffer could not be reserved.
    OutOfMemory { bytes: usize },
}

impl fmt::Display for OrientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongLength => f.write_str("the pixel buffer has the wrong length"),
            Self::OutOfMemory { bytes } => write!(f, "cannot reserve {bytes} bytes"),
        }
    }
}

impl std::error::Error for OrientError {}

/// Turns `pixels` (rows of `size.width` pixels of `channels` bytes, stored orientation)
/// upright for `orientation`, and returns them with their upright size. Orientation 1 returns
/// the buffer unchanged; the others write one new buffer of the same length, reserved with
/// `try_reserve_exact`.
///
/// # Errors
///
/// [`OrientError::WrongLength`] when the buffer length is not `width * height * channels` or
/// `channels` is 0; [`OrientError::OutOfMemory`] when the new buffer cannot be reserved.
pub fn orient(
    pixels: Vec<u8>,
    size: Size,
    channels: usize,
    orientation: Orientation,
) -> Result<(Vec<u8>, Size), OrientError> {
    let wrong = |_| OrientError::WrongLength;
    let (w, h) = (
        usize::try_from(size.width).map_err(wrong)?,
        usize::try_from(size.height).map_err(wrong)?,
    );
    let len = w
        .checked_mul(h)
        .and_then(|n| n.checked_mul(channels))
        .ok_or(OrientError::WrongLength)?;
    if channels == 0 || pixels.len() != len {
        return Err(OrientError::WrongLength);
    }
    let out_size = size.oriented(orientation);
    if len == 0 {
        return Ok((pixels, out_size));
    }
    let row = w * channels;
    if orientation == Orientation::Normal {
        return Ok((pixels, out_size));
    }
    let mut out = Vec::new();
    out.try_reserve_exact(len)
        .map_err(|_| OrientError::OutOfMemory { bytes: len })?;
    out.resize(len, 0);
    match orientation {
        Orientation::Normal => {}
        // Row moves only: mirror within rows, or reverse the row order.
        Orientation::MirrorHorizontal | Orientation::Rotate180 => {
            let flip_rows = orientation == Orientation::Rotate180;
            for y in 0..h {
                let src = &pixels[y * row..(y + 1) * row];
                let oy = if flip_rows { h - 1 - y } else { y };
                let dst = &mut out[oy * row..(oy + 1) * row];
                for (d, s) in dst
                    .chunks_exact_mut(channels)
                    .zip(src.chunks_exact(channels).rev())
                {
                    d.copy_from_slice(s);
                }
            }
        }
        Orientation::MirrorVertical => {
            for y in 0..h {
                let oy = h - 1 - y;
                out[oy * row..(oy + 1) * row].copy_from_slice(&pixels[y * row..(y + 1) * row]);
            }
        }
        // The output is h wide and w tall. Output pixel (x, y) is input pixel number
        // base + x * dx + y * dy (number = row * w + column).
        Orientation::Transpose
        | Orientation::Rotate90
        | Orientation::Transverse
        | Orientation::Rotate270 => {
            let (wi, hi) = (w as isize, h as isize);
            let (base, dx, dy) = match orientation {
                Orientation::Transpose => (0, wi, 1),
                Orientation::Rotate90 => ((hi - 1) * wi, -wi, 1),
                Orientation::Transverse => ((hi - 1) * wi + wi - 1, -wi, -1),
                _ => (wi - 1, wi, -1), // Rotate270
            };
            if channels == CHANNELS {
                transpose_tiles::<CHANNELS>(&pixels, &mut out, w, h, (base, dx, dy));
            } else {
                transpose_tiles_n(&pixels, &mut out, w, h, (base, dx, dy), channels);
            }
        }
    }
    Ok((out, out_size))
}

/// Tile size for the transposing orientations: the strided side stays in cache.
const TILE: usize = 64;

/// Writes `out` (h wide, w tall, `N` bytes per pixel): output pixel (x, y) is input pixel
/// number `base + x * dx + y * dy`, which [`orient`] keeps inside `0..w * h`.
fn transpose_tiles<const N: usize>(
    pixels: &[u8],
    out: &mut [u8],
    w: usize,
    h: usize,
    (base, dx, dy): (isize, isize, isize),
) {
    let out_w = h;
    for ty in (0..w).step_by(TILE) {
        for tx in (0..out_w).step_by(TILE) {
            for y in ty..(ty + TILE).min(w) {
                let row_base = base + y as isize * dy;
                let d_row = y * out_w;
                for x in tx..(tx + TILE).min(out_w) {
                    let s = (row_base + x as isize * dx) as usize * N;
                    let d = (d_row + x) * N;
                    let px: [u8; N] = pixels[s..s + N].try_into().unwrap_or([0; N]);
                    out[d..d + N].copy_from_slice(&px);
                }
            }
        }
    }
}

/// As [`transpose_tiles`], for any number of bytes per pixel.
fn transpose_tiles_n(
    pixels: &[u8],
    out: &mut [u8],
    w: usize,
    h: usize,
    (base, dx, dy): (isize, isize, isize),
    n: usize,
) {
    let out_w = h;
    for y in 0..w {
        let row_base = base + y as isize * dy;
        for x in 0..out_w {
            let s = (row_base + x as isize * dx) as usize * n;
            let d = (y * out_w + x) * n;
            out[d..d + n].copy_from_slice(&pixels[s..s + n]);
        }
    }
}
