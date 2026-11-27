//! Metadata from the user's files: NEF previews now, EXIF (kamadak-exif) later in M1.
//! Read-only: files are opened with `File::open` and read with positioned reads, never
//! memory-mapped (ADR 0012).

pub mod nef_preview;
