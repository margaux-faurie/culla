//! Volume lookups off Windows: there are none. Culla ships for Windows only (SPEC §2); this
//! keeps the crate building and testable on Linux.

use std::path::Path;

use super::{ResolvedFolder, VolumeError};

pub(super) fn resolve_folder(_path: &Path) -> Result<ResolvedFolder, VolumeError> {
    Err(VolumeError::Unsupported)
}

/// UTC: off Windows the file time is read as UTC.
pub(super) fn local_utc_offset_minutes(_utc_ns: i64) -> Option<i16> {
    Some(0)
}
