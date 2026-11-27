//! Screen previews from JPEG bytes (ADR 0012, US-07): the camera JPEG of a pair, or a lone
//! NEF's embedded JPEG from [`read_preview_bytes`](crate::metadata::nef_preview::read_preview_bytes).
//!
//! One call, one thread, no shared state:
//!
//! 1. Culla's own parser checks the bytes and the declared pixel count
//!    ([`culla_core::preview::check_frame`], threat model T5) and plans the sizes
//!    ([`culla_core::preview::plan`]); nothing is allocated for pixels before that.
//! 2. turbojpeg reads the header again, which must declare the same size, then decodes with
//!    DCT scaling (`set_scaling_factor`) into an RGB buffer reserved with `try_reserve_exact`.
//! 3. `fast_image_resize` (Lanczos3) shrinks it to the planned size, if needed.
//! 4. [`culla_core::preview::orient`] turns it upright for the EXIF orientation, into a buffer
//!    it also reserves with `try_reserve_exact`.
//! 5. turbojpeg encodes it at [`QUALITY`] (4:2:0).
//!
//! A panic anywhere in the pipeline fails that preview only ([`PreviewError::Panicked`]).
//! The input is a buffer: files are read into memory by the caller, never memory-mapped.
//! Memory per call is bounded by the plan: at most the input, one decode buffer (about 6 MP
//! for a 24 or 45 MP photo on the G14, 18 MB), one resized and one oriented buffer (5 MP each,
//! 15 MB) and the encoded output (about 1 MB), plus libjpeg-turbo's working memory, which for a
//! progressive JPEG holds the whole image's coefficients (2 bytes per sample, whatever the
//! scale), so progressive frames get a lower pixel cap
//! ([`Limits::max_progressive_pixels`], 25 MP; Nikon cameras write baseline JPEGs). The decode
//! and resize buffers and the oriented one are reserved fallibly; the input, the encoded output
//! and libjpeg-turbo's own memory are not.
//!
//! The reader threads, decode pool, prefetch, LRU and disk cache come in the next slice.

use std::fmt;
use std::panic::{self, AssertUnwindSafe};
use std::time::{Duration, Instant};

use culla_core::exif::Orientation;
use culla_core::preview::{self, CHANNELS, Limits, OrientError, Plan, PlanError, PreviewBox, Size};

use fast_image_resize as fir;

pub use culla_core::preview::QUALITY;

/// What to make.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreviewOptions {
    /// The box the upright preview fits in.
    pub target: PreviewBox,
    /// Caps checked before the decode.
    pub limits: Limits,
    /// JPEG quality of the output, 1 to 100 (clamped).
    pub quality: u8,
}

impl Default for PreviewOptions {
    fn default() -> Self {
        Self {
            target: PreviewBox::G14,
            limits: Limits::DEFAULT,
            quality: QUALITY,
        }
    }
}

/// Time spent in each step of one preview.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Timings {
    /// Culla's frame check and the plan.
    pub check: Duration,
    /// turbojpeg header read and scaled decode.
    pub decode: Duration,
    pub resize: Duration,
    pub orient: Duration,
    pub encode: Duration,
}

impl Timings {
    #[must_use]
    pub fn total(&self) -> Duration {
        self.check + self.decode + self.resize + self.orient + self.encode
    }
}

/// An encoded screen preview.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    /// The JPEG, upright, without EXIF.
    pub jpeg: Vec<u8>,
    /// Its size (the plan's `output`).
    pub size: Size,
    pub plan: Plan,
    pub timings: Timings,
}

/// Why one preview could not be made. The other previews are not affected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewError {
    /// Refused before decoding: not a usable JPEG, or over a cap.
    Plan(PlanError),
    /// turbojpeg's header disagrees with Culla's frame check.
    HeaderMismatch { checked: Size, header: Size },
    /// turbojpeg failed to read or decode the JPEG.
    Decode(String),
    /// The resize failed.
    Resize(String),
    /// turbojpeg failed to encode the preview.
    Encode(String),
    /// A buffer could not be reserved.
    OutOfMemory { bytes: usize },
    /// Something in the pipeline panicked; caught so only this preview fails.
    Panicked,
}

impl fmt::Display for PreviewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Plan(e) => write!(f, "preview refused: {e}"),
            Self::HeaderMismatch { checked, header } => write!(
                f,
                "the JPEG header reads {}x{} to the decoder but {}x{} to Culla's check",
                header.width, header.height, checked.width, checked.height
            ),
            Self::Decode(e) => write!(f, "cannot decode the JPEG: {e}"),
            Self::Resize(e) => write!(f, "cannot resize the preview: {e}"),
            Self::Encode(e) => write!(f, "cannot encode the preview: {e}"),
            Self::OutOfMemory { bytes } => write!(f, "cannot reserve {bytes} bytes"),
            Self::Panicked => f.write_str("the preview pipeline failed unexpectedly"),
        }
    }
}

impl std::error::Error for PreviewError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Plan(e) => Some(e),
            _ => None,
        }
    }
}

impl From<PlanError> for PreviewError {
    fn from(e: PlanError) -> Self {
        Self::Plan(e)
    }
}

/// Makes the screen preview of `jpeg`, shown under `orientation` (from the photo's EXIF,
/// `metadata::exif`; the JPEG's own EXIF is not read here).
///
/// # Errors
///
/// See [`PreviewError`]. Never panics: a panic inside is returned as
/// [`PreviewError::Panicked`].
pub fn build_preview(
    jpeg: &[u8],
    orientation: Orientation,
    options: &PreviewOptions,
) -> Result<Preview, PreviewError> {
    isolate(|| build(jpeg, orientation, options))
}

/// Runs `f`, turning a panic into [`PreviewError::Panicked`] (threat model T5: per-decode
/// isolation). libjpeg-turbo reports its own errors as return codes, so they arrive as
/// [`PreviewError::Decode`] or [`PreviewError::Encode`], not as panics.
fn isolate<T>(f: impl FnOnce() -> Result<T, PreviewError>) -> Result<T, PreviewError> {
    panic::catch_unwind(AssertUnwindSafe(f)).unwrap_or(Err(PreviewError::Panicked))
}

fn build(
    jpeg: &[u8],
    orientation: Orientation,
    options: &PreviewOptions,
) -> Result<Preview, PreviewError> {
    let mut timings = Timings::default();

    let t = Instant::now();
    let plan = preview::plan_for_jpeg(jpeg, orientation, options.target, &options.limits)?;
    timings.check = t.elapsed();

    let t = Instant::now();
    let decoded = decode(jpeg, &plan)?;
    timings.decode = t.elapsed();

    let t = Instant::now();
    let resized = if plan.needs_resize() {
        resize(decoded, plan.decoded, plan.resized)?
    } else {
        decoded
    };
    timings.resize = t.elapsed();

    let t = Instant::now();
    let (upright, size) =
        preview::orient(resized, plan.resized, CHANNELS, orientation).map_err(|e| match e {
            OrientError::OutOfMemory { bytes } => PreviewError::OutOfMemory { bytes },
            OrientError::WrongLength => {
                PreviewError::Resize("resized buffer has the wrong length".into())
            }
        })?;
    timings.orient = t.elapsed();

    let t = Instant::now();
    let out = encode(&upright, size, options.quality)?;
    timings.encode = t.elapsed();

    Ok(Preview {
        jpeg: out,
        size,
        plan,
        timings,
    })
}

/// A zeroed buffer of `bytes`, or [`PreviewError::OutOfMemory`] instead of an abort.
fn zeroed(bytes: usize) -> Result<Vec<u8>, PreviewError> {
    let mut v = Vec::new();
    v.try_reserve_exact(bytes)
        .map_err(|_| PreviewError::OutOfMemory { bytes })?;
    v.resize(bytes, 0);
    Ok(v)
}

fn usize_of(n: u32) -> usize {
    // u32 always fits in usize on the 64-bit targets Culla builds for.
    usize::try_from(n).unwrap_or(usize::MAX)
}

/// Scaled decode to RGB at `plan.decoded`.
fn decode(jpeg: &[u8], plan: &Plan) -> Result<Vec<u8>, PreviewError> {
    let err = |e: turbojpeg::Error| PreviewError::Decode(e.to_string());
    let mut d = turbojpeg::Decompressor::new().map_err(err)?;
    let header = d.read_header(jpeg).map_err(err)?;
    let header_size = Size::new(
        u32::try_from(header.width).unwrap_or(u32::MAX),
        u32::try_from(header.height).unwrap_or(u32::MAX),
    );
    if header_size != plan.source {
        return Err(PreviewError::HeaderMismatch {
            checked: plan.source,
            header: header_size,
        });
    }
    let scale = turbojpeg::ScalingFactor::new(
        usize::from(plan.scale.num()),
        usize::from(plan.scale.denom()),
    );
    d.set_scaling_factor(scale).map_err(err)?;

    let bytes = plan
        .decoded
        .rgb_bytes()
        .ok_or(PreviewError::OutOfMemory { bytes: usize::MAX })?;
    let mut pixels = zeroed(bytes)?;
    let width = usize_of(plan.decoded.width);
    let image = turbojpeg::Image {
        pixels: pixels.as_mut_slice(),
        width,
        pitch: width * CHANNELS,
        height: usize_of(plan.decoded.height),
        format: turbojpeg::PixelFormat::RGB,
    };
    d.decompress(jpeg, image).map_err(err)?;
    Ok(pixels)
}

/// Lanczos3 resize of an RGB buffer from `from` to `to`.
fn resize(pixels: Vec<u8>, from: Size, to: Size) -> Result<Vec<u8>, PreviewError> {
    let err = |e: &dyn fmt::Display| PreviewError::Resize(e.to_string());
    let src =
        fir::images::Image::from_vec_u8(from.width, from.height, pixels, fir::PixelType::U8x3)
            .map_err(|e| err(&e))?;
    let bytes = to
        .rgb_bytes()
        .ok_or(PreviewError::OutOfMemory { bytes: usize::MAX })?;
    let dst_buf = zeroed(bytes)?;
    let mut dst =
        fir::images::Image::from_vec_u8(to.width, to.height, dst_buf, fir::PixelType::U8x3)
            .map_err(|e| err(&e))?;
    let options = fir::ResizeOptions::new()
        .resize_alg(fir::ResizeAlg::Convolution(fir::FilterType::Lanczos3));
    fir::Resizer::new()
        .resize(&src, &mut dst, &options)
        .map_err(|e| err(&e))?;
    Ok(dst.into_vec())
}

/// RGB to JPEG at `quality` (clamped to 1..=100), 4:2:0.
fn encode(pixels: &[u8], size: Size, quality: u8) -> Result<Vec<u8>, PreviewError> {
    let err = |e: turbojpeg::Error| PreviewError::Encode(e.to_string());
    let mut c = turbojpeg::Compressor::new().map_err(err)?;
    c.set_quality(i32::from(quality.clamp(1, 100)))
        .map_err(err)?;
    c.set_subsamp(turbojpeg::Subsamp::Sub2x2).map_err(err)?;
    let width = usize_of(size.width);
    let image = turbojpeg::Image {
        pixels,
        width,
        pitch: width * CHANNELS,
        height: usize_of(size.height),
        format: turbojpeg::PixelFormat::RGB,
    };
    c.compress_to_vec(image).map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_fails_one_preview() {
        let r: Result<(), _> = isolate(|| panic!("boom"));
        assert_eq!(r, Err(PreviewError::Panicked));
        assert_eq!(isolate(|| Ok(7)), Ok(7));
        assert_eq!(
            isolate::<()>(|| Err(PreviewError::Decode("x".into()))),
            Err(PreviewError::Decode("x".into()))
        );
    }

    #[test]
    fn nothing_is_decoded_past_the_pixel_cap() {
        // A header claiming 20000x20000: refused by Culla's check, before turbojpeg runs.
        let mut j = vec![0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x0B, 8];
        j.extend_from_slice(&20_000u16.to_be_bytes());
        j.extend_from_slice(&20_000u16.to_be_bytes());
        j.extend_from_slice(&[1, 1, 0x11, 0, 0xFF, 0xDA, 0, 2, 0xFF, 0xD9]);
        let e = build_preview(&j, Orientation::Normal, &PreviewOptions::default()).unwrap_err();
        assert_eq!(
            e,
            PreviewError::Plan(PlanError::TooManyPixels {
                pixels: 400_000_000,
                cap: 100_000_000
            })
        );
    }

    #[test]
    fn zeroed_reports_out_of_memory() {
        assert_eq!(zeroed(16).unwrap().len(), 16);
        assert!(matches!(
            zeroed(usize::MAX),
            Err(PreviewError::OutOfMemory { .. })
        ));
    }

    #[test]
    fn timings_add_up() {
        let t = Timings {
            check: Duration::from_millis(1),
            decode: Duration::from_millis(2),
            resize: Duration::from_millis(3),
            orient: Duration::from_millis(4),
            encode: Duration::from_millis(5),
        };
        assert_eq!(t.total(), Duration::from_millis(15));
    }

    #[test]
    fn errors_display() {
        let e = PreviewError::HeaderMismatch {
            checked: Size::new(1, 2),
            header: Size::new(3, 4),
        };
        assert_eq!(
            e.to_string(),
            "the JPEG header reads 3x4 to the decoder but 1x2 to Culla's check"
        );
        assert!(PreviewError::Panicked.to_string().contains("unexpectedly"));
    }
}
