//! Streamed, read-only scan of a source folder (design/file-engine.md §3.1, stories US-01 and
//! US-05).
//!
//! The walk lists folders with `std::fs::read_dir` and nothing else: it never opens, reads,
//! creates or changes a file, and it never follows a link. Each entry is described from the
//! listing itself (`DirEntry::file_type` and `DirEntry::metadata`, which do not traverse links:
//! `lstat` on Unix, the `FindNextFileW` data on Windows), so a symlink, junction or mount point
//! is seen as a reparse point and is never entered. Which folders are entered, and how files
//! group, is decided by `culla_core::grouping`.
//!
//! # Batches
//!
//! Folders are walked depth-first with each folder's children in folded-name order, and each
//! folder is listed completely before it is emitted as one [`ScanBatch`], so the first groups
//! can be shown before the rest of the tree is listed (US-01). A folder's `NKSC_PARAM` is listed
//! with it and its entries go into the same batch, since its `*.nksc` files belong to the
//! parent's photos. Grouping a batch with [`culla_core::grouping::group`] therefore gives that
//! folder's groups; [`Grouping::merge`] combines them.
//!
//! # Windows attributes
//!
//! On Windows the read-only, hidden, system, reparse-point and cloud attributes
//! (`RECALL_ON_DATA_ACCESS`, `RECALL_ON_OPEN`, `OFFLINE`) come from
//! `MetadataExt::file_attributes`. The reparse tag itself is not read yet (`winfs`, M2), so a
//! hydrated cloud file without those attributes is reported as a reparse point rather than a
//! placeholder; it is excluded either way. On other platforms only symlinks are reparse points
//! and the attributes are false (dot names still count as hidden, in the core).
//!
//! # Not in this slice
//!
//! Resolving the chosen folder (final path, volume GUID and serial, file-engine §3.1 step 1) and
//! the destination against it come with volume identity. Until then the root must not itself be
//! a link or a placeholder, and the caller passes an in-source destination as components.

use std::fmt;
use std::fs::{self, DirEntry, Metadata};
use std::io;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use culla_core::grouping::{
    self, Attributes, DirAction, EntryKind, ExclusionReason, Grouping, ScanEntry, ScanPolicy,
};
use culla_core::names::fold;

/// The entries of one folder (and of its `NKSC_PARAM`), complete.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanBatch {
    /// The folder, as components from the source root.
    pub folder: Vec<String>,
    /// Its entries in folded-name order, then those of its `NKSC_PARAM`.
    ///
    /// A subfolder that could not be listed gets a batch of its own, whose `folder` is the
    /// parent and whose only entry names the subfolder as [`EntryKind::Unreadable`].
    pub entries: Vec<ScanEntry>,
}

/// Counts for a finished or stopped scan.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScanSummary {
    /// Folders visited (listed or found unreadable), `NKSC_PARAM` folders included.
    pub folders: u64,
    /// Entries emitted.
    pub entries: u64,
    /// The callback asked to stop before the walk ended.
    pub stopped: bool,
}

/// Why a scan could not start.
#[derive(Debug)]
pub enum ScanError {
    /// The root could not be read.
    Io(io::Error),
    /// The root is not a folder.
    NotADirectory,
    /// The root is a link or a cloud placeholder, so it is refused (file-engine §3.2).
    Refused(ExclusionReason),
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "cannot read the folder: {e}"),
            Self::NotADirectory => f.write_str("not a folder"),
            Self::Refused(r) => write!(f, "folder refused: {r}"),
        }
    }
}

impl std::error::Error for ScanError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for ScanError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Walks `root` and hands each folder's entries to `on_batch`, in walk order. Return
/// [`ControlFlow::Break`] from the callback to stop. Read-only.
///
/// A subfolder that cannot be listed is reported as an [`EntryKind::Unreadable`] entry, and none
/// of its entries are emitted (a partial listing could split a group).
///
/// # Errors
///
/// When the root cannot be read, is not a folder, or is a link or a cloud placeholder.
pub fn scan(
    root: &Path,
    policy: &ScanPolicy,
    mut on_batch: impl FnMut(ScanBatch) -> ControlFlow<()>,
) -> Result<ScanSummary, ScanError> {
    // Not following a link at the root: a link there is refused, not resolved.
    let meta = fs::symlink_metadata(root)?;
    let attrs = attributes(&meta);
    if attrs.cloud_placeholder {
        return Err(ScanError::Refused(ExclusionReason::CloudPlaceholder));
    }
    if attrs.reparse_point {
        return Err(ScanError::Refused(ExclusionReason::ReparsePoint));
    }
    if !meta.is_dir() {
        return Err(ScanError::NotADirectory);
    }

    let mut summary = ScanSummary::default();
    // Folders still to list, the next one last.
    let mut stack: Vec<Vec<String>> = vec![Vec::new()];
    while let Some(folder) = stack.pop() {
        let mut batch_folder = folder.clone();
        let mut entries = match list(&path_of(root, &folder), &folder) {
            Ok(entries) => entries,
            Err(e) if folder.is_empty() => return Err(ScanError::Io(e)),
            Err(e) => {
                // Reported against its parent; nothing below it is emitted.
                let name = batch_folder.pop().unwrap_or_default();
                vec![unreadable(&batch_folder, name, &e)]
            }
        };
        summary.folders += 1;

        // Children to enter, in walk order. NKSC_PARAM is listed now, into this batch.
        let mut next: Vec<Vec<String>> = Vec::new();
        let mut sidecar_entries = Vec::new();
        for e in entries.iter().filter(|e| e.kind == EntryKind::Dir) {
            match policy.dir_action(e) {
                DirAction::Enter => next.push(e.components()),
                DirAction::Sidecars => {
                    let path = e.components();
                    summary.folders += 1;
                    match list(&path_of(root, &path), &path) {
                        Ok(inner) => {
                            for d in inner.iter().filter(|d| d.kind == EntryKind::Dir) {
                                if policy.dir_action(d) == DirAction::Enter {
                                    next.push(d.components());
                                }
                            }
                            sidecar_entries.extend(inner);
                        }
                        Err(err) => {
                            sidecar_entries.push(unreadable(&e.folder, e.name.clone(), &err));
                        }
                    }
                }
                DirAction::Skip(_) => {}
            }
        }
        entries.extend(sidecar_entries);
        stack.extend(next.into_iter().rev());

        summary.entries += entries.len() as u64;
        let batch = ScanBatch {
            folder: batch_folder,
            entries,
        };
        if on_batch(batch).is_break() {
            summary.stopped = true;
            break;
        }
    }
    Ok(summary)
}

/// Scans `root` to the end and groups everything (the headless path: CLI, tests).
///
/// # Errors
///
/// As [`scan`].
pub fn scan_and_group(
    root: &Path,
    policy: &ScanPolicy,
) -> Result<(Grouping, ScanSummary), ScanError> {
    let mut parts = Vec::new();
    let summary = scan(root, policy, |batch| {
        parts.push(grouping::group(batch.entries, policy));
        ControlFlow::Continue(())
    })?;
    Ok((Grouping::merge(parts), summary))
}

fn path_of(root: &Path, components: &[String]) -> PathBuf {
    let mut p = root.to_path_buf();
    p.extend(components);
    p
}

fn unreadable(folder: &[String], name: String, err: &io::Error) -> ScanEntry {
    ScanEntry {
        folder: folder.to_vec(),
        name,
        name_lossy: false,
        kind: EntryKind::Unreadable(err.to_string()),
        size: 0,
        mtime_ns: None,
        attrs: Attributes::default(),
    }
}

/// Lists one folder completely, sorted by folded name. Any error while listing fails the whole
/// folder.
fn list(dir: &Path, folder: &[String]) -> io::Result<Vec<ScanEntry>> {
    let mut out = Vec::new();
    for item in fs::read_dir(dir)? {
        out.push(describe(&item?, folder));
    }
    out.sort_by_cached_key(|e| (fold(&e.name), e.name.clone()));
    Ok(out)
}

/// Describes one listed entry without following it.
fn describe(entry: &DirEntry, folder: &[String]) -> ScanEntry {
    let (name, name_lossy) = match entry.file_name().into_string() {
        Ok(s) => (s, false),
        Err(os) => (os.to_string_lossy().into_owned(), true),
    };
    let mut e = ScanEntry {
        folder: folder.to_vec(),
        name,
        name_lossy,
        kind: EntryKind::Other,
        size: 0,
        mtime_ns: None,
        attrs: Attributes::default(),
    };
    // Both calls describe the entry itself, never a link's target.
    let file_type = match entry.file_type() {
        Ok(t) => t,
        Err(err) => {
            e.kind = EntryKind::Unreadable(err.to_string());
            return e;
        }
    };
    let meta = match entry.metadata() {
        Ok(m) => m,
        Err(err) => {
            e.kind = EntryKind::Unreadable(err.to_string());
            return e;
        }
    };
    e.attrs = attributes(&meta);
    if file_type.is_symlink() {
        e.attrs.reparse_point = true;
    } else if file_type.is_dir() {
        e.kind = EntryKind::Dir;
    } else if file_type.is_file() {
        e.kind = EntryKind::File;
        e.size = meta.len();
    }
    e.mtime_ns = meta.modified().ok().and_then(unix_ns);
    e
}

fn unix_ns(t: SystemTime) -> Option<i64> {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_nanos()).ok(),
        Err(before) => i64::try_from(before.duration().as_nanos()).ok().map(|n| -n),
    }
}

#[cfg(windows)]
fn attributes(meta: &Metadata) -> Attributes {
    use std::os::windows::fs::MetadataExt;
    const READONLY: u32 = 0x0000_0001;
    const HIDDEN: u32 = 0x0000_0002;
    const SYSTEM: u32 = 0x0000_0004;
    const REPARSE_POINT: u32 = 0x0000_0400;
    const OFFLINE: u32 = 0x0000_1000;
    const RECALL_ON_OPEN: u32 = 0x0004_0000;
    const RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
    let a = meta.file_attributes();
    Attributes {
        read_only: a & READONLY != 0,
        hidden: a & HIDDEN != 0,
        system: a & SYSTEM != 0,
        reparse_point: a & REPARSE_POINT != 0 || meta.file_type().is_symlink(),
        cloud_placeholder: a & (OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS) != 0,
    }
}

#[cfg(not(windows))]
fn attributes(meta: &Metadata) -> Attributes {
    Attributes {
        reparse_point: meta.file_type().is_symlink(),
        ..Attributes::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_ns_handles_both_sides_of_the_epoch() {
        let after = UNIX_EPOCH + std::time::Duration::from_nanos(1_500);
        assert_eq!(unix_ns(after), Some(1_500));
        let before = UNIX_EPOCH - std::time::Duration::from_secs(1);
        assert_eq!(unix_ns(before), Some(-1_000_000_000));
    }

    #[test]
    fn path_of_joins_components() {
        let p = path_of(Path::new("root"), &["a".to_owned(), "b".to_owned()]);
        assert_eq!(p, Path::new("root").join("a").join("b"));
    }
}
