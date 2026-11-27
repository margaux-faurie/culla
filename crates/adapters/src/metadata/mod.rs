//! Metadata from the user's files: EXIF through kamadak-exif (`exif`) and NEF previews through
//! the core IFD walker (`nef_preview`). Read-only: files are opened with `File::open` and read
//! into buffers, never memory-mapped (ADR 0012).

pub mod exif;
pub mod nef_preview;
