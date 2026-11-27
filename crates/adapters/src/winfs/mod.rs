//! Windows file system access, the only module where Win32 `unsafe` is allowed
//! (docs/architecture.md, design/file-engine.md §9.7).
//!
//! This first part holds read-only lookups only; the file operations (`rename_noreplace`,
//! verified copy, `remove_verified`) land in M2:
//!
//! - [`resolve_folder`]: the volume of a folder and the folder's path from the volume root,
//!   which make its identity (ADR 0008, design/file-engine.md §3.1 step 1, story US-09).
//! - [`local_utc_offset_minutes`]: the PC's offset from UTC at an instant, so the file-time
//!   fallback of the capture time is read in the PC's local time zone (ADR 0028).
//!
//! Neither opens, creates or changes a file. On other platforms (Linux CI, the CLI on a dev
//! machine) [`resolve_folder`] returns [`VolumeError::Unsupported`] and the offset is 0 (UTC).
//!
//! # Supported paths
//!
//! The folder is first canonicalized (`std::fs::canonicalize`, which follows links above the
//! folder once and returns the final path), then split by [`culla_core::volume::classify_path`]:
//!
//! - `X:\…` and its verbatim form `\\?\X:\…`: supported. A folder under a volume mounted in a
//!   folder (`C:\Mounts\Card\DCIM`) resolves to that volume, with the mounted folder as root.
//! - A `subst` drive: canonicalization resolves it to the folder it stands for, so its
//!   identity is the underlying volume's, whatever the letter.
//! - Paths longer than `MAX_PATH` are queried in their verbatim form (not covered by tests).
//! - Refused: network paths (`\\server\share`, `\\?\UNC\…`, and mapped network drives, which
//!   canonicalize to UNC), volumes reachable only by GUID path, device paths, and paths that
//!   are not valid Unicode.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use culla_core::names::NameError;
use culla_core::volume::{Source, UnsupportedPath, VolumeInfo, VolumePathError};

#[cfg(windows)]
mod win32;
#[cfg(windows)]
use win32 as sys;

#[cfg(not(windows))]
mod unsupported;
#[cfg(not(windows))]
use unsupported as sys;

/// A folder and the volume it is on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedFolder {
    /// The folder's final path, as `std::fs::canonicalize` returned it.
    pub canonical: PathBuf,
    /// What Windows reports about the folder's volume.
    pub volume: VolumeInfo,
    /// The folder as a source: the volume serial and the path from the volume root. Its
    /// [`Source::key`] finds the session.
    pub source: Source,
}

/// Why a folder's volume could not be resolved.
#[derive(Debug)]
pub enum VolumeError {
    /// Not on Windows: volumes are not looked up on other platforms.
    Unsupported,
    /// The folder could not be canonicalized (missing, no access).
    Io(io::Error),
    /// A Win32 call failed.
    Win32 {
        /// The function that failed.
        call: &'static str,
        /// Its error (`GetLastError`).
        error: io::Error,
    },
    /// The path or the volume root it gave is not supported, or a name in it is not valid.
    Path(VolumePathError),
}

impl fmt::Display for VolumeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => f.write_str("volume lookups are only supported on Windows"),
            Self::Io(e) => write!(f, "cannot resolve the folder: {e}"),
            Self::Win32 { call, error } => write!(f, "{call} failed: {error}"),
            Self::Path(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for VolumeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) | Self::Win32 { error: e, .. } => Some(e),
            Self::Path(e) => Some(e),
            Self::Unsupported => None,
        }
    }
}

impl From<VolumePathError> for VolumeError {
    fn from(e: VolumePathError) -> Self {
        Self::Path(e)
    }
}

impl From<UnsupportedPath> for VolumeError {
    fn from(e: UnsupportedPath) -> Self {
        Self::Path(VolumePathError::Unsupported(e))
    }
}

impl From<NameError> for VolumeError {
    fn from(e: NameError) -> Self {
        Self::Path(VolumePathError::Name(e))
    }
}

/// The volume of the folder at `path` and the folder's path from the volume root.
///
/// Read-only: `std::fs::canonicalize`, then `GetVolumePathNameW` (the volume root),
/// `GetVolumeNameForVolumeMountPointW` (its GUID path), `GetVolumeInformationW` on the GUID
/// path (serial, label, file system, flags) and `GetDriveTypeW` on the root. See the module
/// docs for the supported paths.
///
/// # Errors
///
/// [`VolumeError::Unsupported`] off Windows; otherwise the step that failed.
pub fn resolve_folder(path: &Path) -> Result<ResolvedFolder, VolumeError> {
    sys::resolve_folder(path)
}

/// The PC's offset from UTC at `utc_ns` (nanoseconds since the Unix epoch), in minutes east
/// of UTC, under the PC's current time zone and the daylight saving rule in force on that date
/// (ADR 0028).
///
/// `None` when Windows cannot convert the instant (before 1601, or a time zone lookup that
/// fails); callers then read the time as UTC. Always `Some(0)` off Windows.
#[must_use]
pub fn local_utc_offset_minutes(utc_ns: i64) -> Option<i16> {
    sys::local_utc_offset_minutes(utc_ns)
}
