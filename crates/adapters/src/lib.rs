//! Culla adapters: the only crate that touches the user's files.
//!
//! The audited removal module `winfs::remove_verified` lands in M2
//! (`docs/design/file-engine.md` §9.7). Until then this crate holds no file operations; it only
//! lists folders (`scan`), reads files (`metadata`: EXIF and NEF previews) and makes read-only
//! Win32 lookups (`winfs`: a folder's volume identity, the PC's UTC offset).

pub mod metadata;
pub mod scan;
pub mod winfs;

pub use culla_core::VERSION;
