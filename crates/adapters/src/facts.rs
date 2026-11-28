//! Read-only lookups for a dry-run plan and preflight (`culla plan`, design/file-engine.md §7
//! and §8): what one path is, a snapshot of the target folders, and the text of a decision log
//! the user names.
//!
//! Nothing here creates, writes, renames or changes anything (INV-03, ADR 0020). Paths are
//! described with `std::fs::symlink_metadata`, which never follows a link, and folders are
//! listed with `std::fs::read_dir`, whose entries are described from the listing itself, so a
//! link is reported as a reparse point and never entered. The only file opened is the
//! decision log, for reading, and only when it is a regular file that is not a cloud
//! placeholder (reading a placeholder would download it).
//!
//! These lookups go by path, like the scan, so a swap between a lookup and an apply could make
//! them describe another file (threat model T2). They feed a dry run and the review only; the
//! apply steps check again through handles (`winfs`, M2). The adapter of the apply slice will
//! gather the same facts afresh in the instant before the first operation.

use std::collections::BTreeSet;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use culla_core::grouping::Attributes;
use culla_core::plan::{Existing, Root};
use culla_core::preflight::FolderFact;

use crate::scan::{attributes, unix_ns};

/// What is at a path, seen without following a link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lookup {
    /// Nothing (or a component above it is not a folder).
    Missing,
    /// A regular file.
    File {
        /// Size in bytes.
        size: u64,
        /// Modification time in nanoseconds since the Unix epoch, when known.
        mtime_ns: Option<i64>,
        /// Its attributes.
        attrs: Attributes,
    },
    /// A folder (not a link to one).
    Folder(Attributes),
    /// Anything else: a link, a device, a socket. A link has `reparse_point` set.
    Other(Attributes),
    /// The lookup failed (access denied, an I/O error): the text says why.
    Unreadable(String),
}

/// Describes the entry at `path` without following it. Read-only: one `symlink_metadata`.
#[must_use]
pub fn lookup(path: &Path) -> Lookup {
    match fs::symlink_metadata(path) {
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            Lookup::Missing
        }
        Err(e) => Lookup::Unreadable(e.to_string()),
        Ok(meta) => {
            let mut attrs = attributes(&meta);
            let ft = meta.file_type();
            if ft.is_symlink() {
                attrs.reparse_point = true;
                Lookup::Other(attrs)
            } else if ft.is_dir() {
                Lookup::Folder(attrs)
            } else if ft.is_file() {
                Lookup::File {
                    size: meta.len(),
                    mtime_ns: meta.modified().ok().and_then(unix_ns),
                    attrs,
                }
            } else {
                Lookup::Other(attrs)
            }
        }
    }
}

/// The final path of `path`, as `std::fs::canonicalize` gives it (on Windows it opens a
/// handle with no access rights, like `winfs::resolve_folder`). Read-only.
///
/// # Errors
///
/// When the path cannot be resolved (missing, no access).
pub fn canonical(path: &Path) -> io::Result<PathBuf> {
    fs::canonicalize(path)
}

/// Why a text file was not read.
#[derive(Debug)]
pub enum ReadTextError {
    /// It could not be looked at or read.
    Io(io::Error),
    /// It is not a regular file (a folder, a link, a device).
    NotAFile,
    /// It is a cloud placeholder: reading it would download it.
    Placeholder,
    /// It is larger than the cap, in bytes.
    TooLarge {
        /// The cap.
        max: u64,
    },
    /// It is not valid UTF-8.
    NotUtf8,
}

impl fmt::Display for ReadTextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "cannot read the file: {e}"),
            Self::NotAFile => f.write_str("not a regular file (a folder, a link or a device)"),
            Self::Placeholder => f.write_str("a cloud placeholder: reading it would download it"),
            Self::TooLarge { max } => write!(f, "larger than {max} bytes"),
            Self::NotUtf8 => f.write_str("not valid UTF-8"),
        }
    }
}

impl std::error::Error for ReadTextError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for ReadTextError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// The text of the regular file at `path`, at most `max` bytes. Read-only: it is looked at
/// without following a link, then opened for reading only. A link, a folder or a cloud
/// placeholder is refused before anything is opened.
///
/// # Errors
///
/// [`ReadTextError`] says why.
pub fn read_text(path: &Path, max: u64) -> Result<String, ReadTextError> {
    match lookup(path) {
        Lookup::File { size, attrs, .. } => {
            if attrs.cloud_placeholder {
                return Err(ReadTextError::Placeholder);
            }
            if size > max {
                return Err(ReadTextError::TooLarge { max });
            }
        }
        Lookup::Missing => {
            return Err(ReadTextError::Io(io::Error::from(io::ErrorKind::NotFound)));
        }
        Lookup::Unreadable(why) => return Err(ReadTextError::Io(io::Error::other(why))),
        Lookup::Folder(_) | Lookup::Other(_) => return Err(ReadTextError::NotAFile),
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take(max.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        return Err(ReadTextError::TooLarge { max });
    }
    String::from_utf8(bytes).map_err(|_| ReadTextError::NotUtf8)
}

/// What [`snapshot_targets`] saw of one target root.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TargetFacts {
    /// The facts of the folders it looked at, as components from the target root; the root
    /// itself is the empty path. A folder it found missing has no entry.
    pub folders: Vec<(Vec<String>, FolderFact)>,
    /// The root itself is a file: everything under it is blocked.
    pub root_is_file: bool,
}

/// Records into `existing` what lies under the target root `root` (at `root_dir`) on the way
/// to each of `folders` (components from that root) and inside each of them, and returns the
/// facts of the folders it looked at. Read-only.
///
/// For each folder it walks the components from the root with [`lookup`]: a missing component
/// ends the walk (nothing below it exists), a file is recorded as a file (it blocks the
/// folders below it), a link or a placeholder is recorded and not entered, and a folder that
/// could not be looked at is recorded as unreadable. A folder reached in full is listed
/// completely: every entry (file, folder or other, never followed) is recorded. A listing that
/// fails, or that has more than `max_entries` entries, marks the folder unreadable instead, so
/// the groups going into it are held (fails closed: an incomplete listing could hide a taken
/// name).
pub fn snapshot_targets(
    root_dir: &Path,
    root: Root,
    folders: &BTreeSet<Vec<String>>,
    max_entries: usize,
    existing: &mut Existing,
) -> TargetFacts {
    let mut out = TargetFacts::default();
    match lookup(root_dir) {
        Lookup::Missing => return out,
        Lookup::Folder(a) => {
            out.folders.push((Vec::new(), FolderFact::Exists(a)));
            if a.reparse_point || a.cloud_placeholder {
                return out;
            }
        }
        Lookup::Other(a) => {
            out.folders.push((Vec::new(), FolderFact::Exists(a)));
            return out;
        }
        Lookup::File { .. } => {
            out.root_is_file = true;
            out.folders.push((Vec::new(), FolderFact::Unreadable));
            return out;
        }
        Lookup::Unreadable(_) => {
            out.folders.push((Vec::new(), FolderFact::Unreadable));
            return out;
        }
    }
    let mut seen: BTreeSet<Vec<String>> = BTreeSet::new();
    let mut listed: BTreeSet<Vec<String>> = BTreeSet::new();
    for folder in folders {
        let mut reached = true;
        for n in 1..=folder.len() {
            let part = &folder[..n];
            if !seen.insert(part.to_vec()) {
                // Looked at already: go on only through a plain folder.
                let plain = out.folders.iter().any(|(p, f)| {
                    p == part
                        && matches!(f, FolderFact::Exists(a) if !a.reparse_point && !a.cloud_placeholder)
                });
                if !plain {
                    reached = false;
                    break;
                }
                continue;
            }
            let strs: Vec<&str> = part.iter().map(String::as_str).collect();
            match lookup(&path_of(root_dir, part)) {
                Lookup::Missing => {
                    reached = false;
                    break;
                }
                Lookup::File { .. } => {
                    existing.add_file(root, &strs);
                    reached = false;
                    break;
                }
                Lookup::Folder(a) => {
                    existing.add_dir(root, &strs);
                    out.folders.push((part.to_vec(), FolderFact::Exists(a)));
                    if a.reparse_point || a.cloud_placeholder {
                        reached = false;
                        break;
                    }
                }
                Lookup::Other(a) => {
                    existing.add_dir(root, &strs);
                    out.folders.push((part.to_vec(), FolderFact::Exists(a)));
                    reached = false;
                    break;
                }
                Lookup::Unreadable(_) => {
                    out.folders.push((part.to_vec(), FolderFact::Unreadable));
                    reached = false;
                    break;
                }
            }
        }
        if reached && listed.insert(folder.clone()) {
            list_into(root_dir, root, folder, max_entries, existing, &mut out);
        }
    }
    out
}

/// Lists `folder` (under `root_dir`) into `existing`, or marks it unreadable in `out`.
fn list_into(
    root_dir: &Path,
    root: Root,
    folder: &[String],
    max_entries: usize,
    existing: &mut Existing,
    out: &mut TargetFacts,
) {
    let listing = (|| -> io::Result<Option<Vec<(String, bool)>>> {
        let mut entries = Vec::new();
        for item in fs::read_dir(path_of(root_dir, folder))? {
            if entries.len() >= max_entries {
                return Ok(None);
            }
            let item = item?;
            // The listing's own file type: a link is never followed.
            let is_file = item.file_type()?.is_file();
            entries.push((item.file_name().to_string_lossy().into_owned(), is_file));
        }
        Ok(Some(entries))
    })();
    match listing {
        Ok(Some(entries)) => {
            for (name, is_file) in entries {
                let mut parts: Vec<&str> = folder.iter().map(String::as_str).collect();
                parts.push(&name);
                if is_file {
                    existing.add_file(root, &parts);
                } else {
                    existing.add_dir(root, &parts);
                }
            }
        }
        Ok(None) | Err(_) => {
            for (p, f) in &mut out.folders {
                if p == folder {
                    *f = FolderFact::Unreadable;
                    return;
                }
            }
            out.folders.push((folder.to_vec(), FolderFact::Unreadable));
        }
    }
}

fn path_of(root: &Path, components: &[String]) -> PathBuf {
    let mut p = root.to_path_buf();
    p.extend(components);
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_path_is_missing() {
        let p = std::env::temp_dir().join("culla-facts-no-such-entry-7f3a9c");
        assert_eq!(lookup(&p), Lookup::Missing);
        assert_eq!(lookup(&p.join("below")), Lookup::Missing);
    }

    #[test]
    fn a_missing_root_records_nothing() {
        let p = std::env::temp_dir().join("culla-facts-no-such-root-7f3a9c");
        let mut existing = Existing::new();
        let folders: BTreeSet<Vec<String>> = [vec!["a".to_owned()]].into_iter().collect();
        let got = snapshot_targets(&p, Root::Dest, &folders, 10, &mut existing);
        assert_eq!(got, TargetFacts::default());
        assert!(!existing.has_entries(Root::Dest));
    }
}
