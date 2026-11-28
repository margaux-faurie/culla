//! Streamed, read-only scan of a source folder (design/file-engine.md §3.1, stories US-01 and
//! US-05).
//!
//! The walk lists folders with `std::fs::read_dir` and nothing else: it never opens, reads,
//! creates or changes a file, and it does not enter a link it sees. It re-checks each folder
//! just before listing it, but lists by path, so a swap in between can still redirect a
//! listing (see "Swaps" below). Each entry is described from the listing itself (`DirEntry::file_type` and `DirEntry::metadata`, which do not traverse links:
//! `lstat` on Unix, the `FindNextFileW` data on Windows), so a symlink, junction or mount point
//! is seen as a reparse point and is never entered. Which folders are entered, and how files
//! group, is decided by `culla_core::grouping`.
//!
//! The walk lives here over `std::fs` for now. In M2 it moves behind the core `FileSystem`
//! port (implemented in `winfs`), so the recording fake can prove it read-only like the rest
//! of a session (testing.md §4; ADR 0027).
//!
//! # Never a partial folder
//!
//! A folder is listed completely or not at all. If any entry cannot be described, if the
//! listing fails part-way, or if the folder has more than
//! [`ScanPolicy::max_entries_per_folder`] entries (listing stops at the first entry over the
//! cap), none of its entries is emitted: the folder is reported once, as an
//! [`EntryKind::Unlisted`] entry against its parent. The same holds for a folder's
//! `NKSC_PARAM`, and the core then holds back the parent's files too.
//!
//! # Swaps (threat model T2)
//!
//! Each folder is checked with `symlink_metadata` just before it is listed, and refused if it
//! has become a link, a placeholder or anything but a folder since its parent was listed. That
//! narrows the window but does not close it: the listing still goes by path. Scan output is
//! therefore not trusted against a swap; preflight and every apply step re-check each path
//! component through handles, and handle-relative listing with `FILE_FLAG_OPEN_REPARSE_POINT`
//! comes with `winfs` in M2.
//!
//! # Batches
//!
//! Folders are walked depth-first with each folder's children in folded-name order, and each
//! folder is listed completely before it is emitted as one [`ScanBatch`], so the first groups
//! can be shown before the rest of the tree is listed (US-01). A folder's `NKSC_PARAM` is listed
//! with it and its entries go into the same batch, since its `*.nksc` files belong to the
//! parent's photos. Grouping a batch with [`culla_core::grouping::group`] therefore gives that
//! folder's groups; [`Grouping::merge`] combines them. Every entry of a folder shares one
//! `Arc` of its components.
//!
//! # Windows attributes
//!
//! On Windows the read-only, hidden, system, reparse-point and cloud attributes
//! (`RECALL_ON_DATA_ACCESS`, `RECALL_ON_OPEN`, `OFFLINE`) come from
//! `MetadataExt::file_attributes`. The reparse tag itself is not read yet (`winfs`, M2), so a
//! hydrated cloud file without those attributes is reported as a reparse point rather than a
//! placeholder; it is excluded either way. On other platforms only symlinks are reparse points
//! and the attributes are false (dot names still count as hidden, in the core). Junctions and
//! the Windows attributes are covered by the `windows-it` suite in M2.
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
use std::sync::Arc;
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
    /// parent and whose only entry names the subfolder as [`EntryKind::Unlisted`].
    pub entries: Vec<ScanEntry>,
}

/// Counts for a finished or stopped scan.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScanSummary {
    /// Folders visited (listed or found unlistable), `NKSC_PARAM` folders included.
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
    /// The root has more entries than the cap.
    TooLarge {
        /// [`ScanPolicy::max_entries_per_folder`].
        cap: usize,
    },
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "cannot read the folder: {e}"),
            Self::NotADirectory => f.write_str("not a folder"),
            Self::Refused(r) => write!(f, "folder refused: {r}"),
            Self::TooLarge { cap } => write!(f, "the folder has more than {cap} entries"),
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

/// Why a folder was not listed.
#[derive(Debug)]
enum ListFailure {
    Io(io::Error),
    NotADirectory,
    Refused(ExclusionReason),
    TooLarge(usize),
}

impl ListFailure {
    fn into_root_error(self) -> ScanError {
        match self {
            Self::Io(e) => ScanError::Io(e),
            Self::NotADirectory => ScanError::NotADirectory,
            Self::Refused(r) => ScanError::Refused(r),
            Self::TooLarge(cap) => ScanError::TooLarge { cap },
        }
    }

    fn into_kind(self) -> EntryKind {
        let (reason, detail) = match self {
            Self::Io(e) => (ExclusionReason::Unreadable, e.to_string()),
            Self::NotADirectory => (
                ExclusionReason::Unreadable,
                "no longer a folder when it was listed".to_owned(),
            ),
            Self::Refused(r) => (
                r,
                "became a link or a placeholder during the scan".to_owned(),
            ),
            Self::TooLarge(cap) => (
                ExclusionReason::TooLarge,
                format!("more than {cap} entries"),
            ),
        };
        EntryKind::Unlisted { reason, detail }
    }
}

impl From<io::Error> for ListFailure {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Walks `root` and hands each folder's entries to `on_batch`, in walk order. Return
/// [`ControlFlow::Break`] from the callback to stop. Read-only.
///
/// # Errors
///
/// When the root cannot be read or listed completely, is not a folder, is a link or a cloud
/// placeholder, or has more entries than the cap.
pub fn scan(
    root: &Path,
    policy: &ScanPolicy,
    mut on_batch: impl FnMut(ScanBatch) -> ControlFlow<()>,
) -> Result<ScanSummary, ScanError> {
    check_folder(root).map_err(ListFailure::into_root_error)?;

    let mut summary = ScanSummary::default();
    // Folders still to list, the next one last; `None` is the root. Each queued folder holds
    // only its parent's shared components and its own name (threat model T19).
    let mut stack: Vec<Option<Queued>> = vec![None];
    while let Some(queued) = stack.pop() {
        summary.folders += 1;
        let folder = match &queued {
            None => Vec::new(),
            Some(q) => {
                let mut path = q.parent.to_vec();
                path.push(q.name.clone());
                path
            }
        };
        let (batch_folder, mut entries) = match (list(root, &folder, policy), queued) {
            (Ok(entries), _) => (folder, entries),
            (Err(fail), None) => return Err(fail.into_root_error()),
            (Err(fail), Some(q)) => {
                // Reported against its parent; nothing below it is emitted.
                let parent = q.parent.to_vec();
                let entry = ScanEntry::in_folder(q.parent, &q.name, fail.into_kind());
                (parent, vec![entry])
            }
        };

        // Children to enter, in walk order. NKSC_PARAM is listed now, into this batch.
        let mut next: Vec<Option<Queued>> = Vec::new();
        let mut sidecar_entries = Vec::new();
        for e in &mut entries {
            if e.kind != EntryKind::Dir {
                continue;
            }
            match policy.dir_action(e) {
                DirAction::Enter => next.push(Some(Queued::child_of(e))),
                DirAction::Sidecars => {
                    summary.folders += 1;
                    match list(root, &e.components(), policy) {
                        Ok(inner) => {
                            for d in inner.iter().filter(|d| d.kind == EntryKind::Dir) {
                                if policy.dir_action(d) == DirAction::Enter {
                                    next.push(Some(Queued::child_of(d)));
                                }
                            }
                            sidecar_entries.extend(inner);
                        }
                        // The parent's sidecars are incomplete: the core holds it back.
                        Err(fail) => e.kind = fail.into_kind(),
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

/// A folder waiting to be listed: its parent's shared components and its name.
struct Queued {
    parent: Arc<[String]>,
    name: String,
}

impl Queued {
    fn child_of(dir: &ScanEntry) -> Self {
        Self {
            parent: Arc::clone(&dir.folder),
            name: dir.name.clone(),
        }
    }
}

fn path_of(root: &Path, components: &[String]) -> PathBuf {
    let mut p = root.to_path_buf();
    p.extend(components);
    p
}

/// Refuses a folder that is now a link, a placeholder or not a folder (checked without
/// following it).
fn check_folder(path: &Path) -> Result<(), ListFailure> {
    let meta = fs::symlink_metadata(path)?;
    let attrs = attributes(&meta);
    if attrs.cloud_placeholder {
        return Err(ListFailure::Refused(ExclusionReason::CloudPlaceholder));
    }
    if attrs.reparse_point {
        return Err(ListFailure::Refused(ExclusionReason::ReparsePoint));
    }
    if !meta.is_dir() {
        return Err(ListFailure::NotADirectory);
    }
    Ok(())
}

/// Lists one folder completely, sorted by folded name. Any error, or one entry over the cap,
/// fails the whole folder.
fn list(
    root: &Path,
    components: &[String],
    policy: &ScanPolicy,
) -> Result<Vec<ScanEntry>, ListFailure> {
    let dir = path_of(root, components);
    check_folder(&dir)?;
    let folder: Arc<[String]> = components.into();
    let cap = policy.max_entries_per_folder;
    let mut out = Vec::new();
    for item in fs::read_dir(&dir)? {
        if out.len() >= cap {
            return Err(ListFailure::TooLarge(cap));
        }
        out.push(describe(&item?, &folder)?);
    }
    out.sort_by_cached_key(|e| (fold(&e.name), e.name.clone()));
    Ok(out)
}

/// Describes one listed entry without following it.
fn describe(entry: &DirEntry, folder: &Arc<[String]>) -> io::Result<ScanEntry> {
    let (name, name_lossy) = match entry.file_name().into_string() {
        Ok(s) => (s, false),
        Err(os) => (os.to_string_lossy().into_owned(), true),
    };
    // Both calls describe the entry itself, never a link's target.
    let file_type = entry.file_type()?;
    let meta = entry.metadata()?;
    let mut e = ScanEntry::in_folder(Arc::clone(folder), &name, EntryKind::Other);
    e.name_lossy = name_lossy;
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
    Ok(e)
}

pub(crate) fn unix_ns(t: SystemTime) -> Option<i64> {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_nanos()).ok(),
        Err(before) => i64::try_from(before.duration().as_nanos()).ok().map(|n| -n),
    }
}

#[cfg(windows)]
pub(crate) fn attributes(meta: &Metadata) -> Attributes {
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
pub(crate) fn attributes(meta: &Metadata) -> Attributes {
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

    #[test]
    fn list_failures_map_to_reasons() {
        let kind = ListFailure::TooLarge(5).into_kind();
        assert!(matches!(
            kind,
            EntryKind::Unlisted {
                reason: ExclusionReason::TooLarge,
                ..
            }
        ));
        let kind = ListFailure::Refused(ExclusionReason::ReparsePoint).into_kind();
        assert!(matches!(
            kind,
            EntryKind::Unlisted {
                reason: ExclusionReason::ReparsePoint,
                ..
            }
        ));
        assert!(matches!(
            ListFailure::TooLarge(5).into_root_error(),
            ScanError::TooLarge { cap: 5 }
        ));
    }
}
