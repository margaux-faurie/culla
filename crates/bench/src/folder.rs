//! What every benchmark does with the folder: resolve it, scan it, read its EXIF, choose each
//! photo's preview source and make one preview, timing each step. Read-only: everything goes
//! through `culla-adapters` (`winfs::resolve_folder`, `scan::scan`, `metadata`) or opens files
//! for reading only.

use std::fs::File;
use std::io::{self, IsTerminal, Read, Write};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use culla_adapters::metadata::exif::read_group_meta;
use culla_adapters::metadata::nef_preview::{self, FileSource, MAX_PREVIEW_BYTES};
use culla_adapters::pipeline::Item;
use culla_adapters::preview::{PreviewOptions, Timings, build_preview};
use culla_adapters::scan::{ScanError, ScanSummary, scan};
use culla_adapters::winfs::{VolumeError, resolve_folder};
use culla_core::exif::{Orientation, PhotoMeta, capture_order};
use culla_core::grouping::{self, Group, Grouping, PhotoKind, ScanPolicy};
use culla_core::ifd::{self, Limits};

use crate::run::Medium;

/// Why a benchmark could not run on the folder.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Failure {
    /// The folder is refused: exit code 2.
    Refused(String),
    /// The folder cannot be read: exit code 3.
    Io(String),
}

/// The folder resolved: the path to scan and its medium.
pub(crate) fn resolve(folder: &Path) -> Result<(PathBuf, Medium), Failure> {
    match resolve_folder(folder) {
        Ok(r) => {
            let medium = Medium::of(&r.volume);
            Ok((r.canonical, medium))
        }
        Err(VolumeError::Unsupported) => Ok((
            folder.to_path_buf(),
            Medium::Unavailable(VolumeError::Unsupported.to_string()),
        )),
        Err(e @ VolumeError::Path(_)) => Err(Failure::Refused(e.to_string())),
        Err(e @ (VolumeError::Io(_) | VolumeError::Win32 { .. })) => {
            Err(Failure::Io(e.to_string()))
        }
    }
}

/// A finished scan with its timings.
pub(crate) struct Scanned {
    pub grouping: Grouping,
    pub summary: ScanSummary,
    /// From the start of the walk to the first group (B-01).
    pub first_group: Option<Duration>,
    /// The walk and grouping, without the time spent in `on_first` (B-02).
    pub listing: Duration,
}

/// Scans `root`. `on_first` runs once, inside the walk, on the first group found (to make its
/// preview as the app would); its time is left out of [`Scanned::listing`].
pub(crate) fn scan_folder(
    root: &Path,
    include_subfolders: bool,
    mut on_first: impl FnMut(&Path, &Group),
) -> Result<Scanned, Failure> {
    let policy = ScanPolicy::new(include_subfolders);
    let mut parts = Vec::new();
    let mut first_group = None;
    let mut hook = Duration::ZERO;
    let started = Instant::now();
    let summary = scan(root, &policy, |batch| {
        let part = grouping::group(batch.entries, &policy);
        if first_group.is_none()
            && let Some(g) = part.groups.first()
        {
            first_group = Some(started.elapsed());
            let t = Instant::now();
            on_first(root, g);
            hook = t.elapsed();
        }
        parts.push(part);
        ControlFlow::Continue(())
    })
    .map_err(|e| match e {
        ScanError::Io(_) => Failure::Io(e.to_string()),
        ScanError::NotADirectory | ScanError::Refused(_) | ScanError::TooLarge { .. } => {
            Failure::Refused(e.to_string())
        }
    })?;
    let grouping = Grouping::merge(parts);
    let listing = started.elapsed().saturating_sub(hook);
    Ok(Scanned {
        grouping,
        summary,
        first_group,
        listing,
    })
}

/// The EXIF pass and the capture-time order, timed apart.
pub(crate) struct Exif {
    pub meta: Vec<Result<PhotoMeta, String>>,
    pub pass: Duration,
    pub order: Vec<usize>,
    pub sort: Duration,
}

/// Reads every group's EXIF (the first ~128 KB of each primary), then orders the groups.
pub(crate) fn exif_pass(root: &Path, groups: &[Group]) -> Exif {
    let t = Instant::now();
    let meta: Vec<Result<PhotoMeta, String>> = groups
        .iter()
        .enumerate()
        .map(|(i, g)| {
            progress("exif", i + 1, groups.len());
            read_group_meta(root, g).map_err(|e| e.to_string())
        })
        .collect();
    progress_done("exif", groups.len());
    let pass = t.elapsed();
    let t = Instant::now();
    let captured: Vec<_> = meta
        .iter()
        .map(|m| m.as_ref().ok().and_then(|m| m.captured))
        .collect();
    let order = capture_order(groups, &captured);
    Exif {
        meta,
        pass,
        order,
        sort: t.elapsed(),
    }
}

/// The orientation of each group, upright when its EXIF could not be read.
pub(crate) fn orientations(meta: &[Result<PhotoMeta, String>]) -> Vec<Orientation> {
    meta.iter()
        .map(|m| m.as_ref().map_or(Orientation::Normal, |m| m.orientation))
        .collect()
}

/// Which JPEG a pair's preview comes from (B-11).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceChoice {
    /// The camera JPEG (the app's choice so far).
    Jpeg,
    /// The NEF's largest embedded JPEG (JpgFromRaw).
    Nef,
}

impl SourceChoice {
    pub(crate) fn parse(s: &str) -> Option<Self> {
        match s {
            "jpeg" => Some(Self::Jpeg),
            "nef" => Some(Self::Nef),
            _ => None,
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Jpeg => "jpeg",
            Self::Nef => "nef",
        }
    }
}

fn path_of(root: &Path, components: &[String]) -> PathBuf {
    let mut p = root.to_path_buf();
    p.extend(components);
    p
}

/// What to read for `group`: with [`SourceChoice::Nef`], a pair's NEF; with
/// [`SourceChoice::Jpeg`], its JPEG. A lone NEF is always its NEF, a JPEG-only group always
/// its JPEG. Also returns which one was taken.
#[must_use]
pub(crate) fn item_for(
    root: &Path,
    group: &Group,
    orientation: Orientation,
    choice: SourceChoice,
) -> (Item, SourceChoice) {
    let nef = match (group.primary.kind, &group.raw) {
        (PhotoKind::Nef, _) => Some(&group.primary),
        (PhotoKind::Jpeg, Some(raw)) if choice == SourceChoice::Nef => Some(raw),
        (PhotoKind::Jpeg, _) => None,
    };
    match nef {
        Some(p) => (
            Item::LoneNef {
                path: path_of(root, &p.entry.components()),
                orientation,
            },
            SourceChoice::Nef,
        ),
        None => (
            Item::Jpeg {
                path: path_of(root, &group.primary.entry.components()),
                orientation,
            },
            SourceChoice::Jpeg,
        ),
    }
}

/// One preview made outside the pipeline, one step after another (B-05, B-08 miss).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct OnePreview {
    /// The IFD walk of a NEF (zero for a JPEG), with the reads it made.
    pub walk: Duration,
    pub walk_reads: u32,
    pub walk_bytes: u64,
    /// Reading the JPEG into memory.
    pub read: Duration,
    pub read_bytes: u64,
    pub timings: Timings,
    /// The encoded preview.
    pub out_bytes: u64,
}

impl OnePreview {
    /// Walk, read and build.
    pub(crate) fn total(&self) -> Duration {
        self.walk + self.read + self.timings.total()
    }
}

/// Reads `item`'s JPEG and builds its preview. Read-only.
pub(crate) fn make_preview(item: &Item, options: &PreviewOptions) -> Result<OnePreview, String> {
    let mut one = OnePreview::default();
    let (jpeg, orientation) = match item {
        Item::Jpeg { path, orientation } => {
            let t = Instant::now();
            let jpeg =
                read_all(path, MAX_PREVIEW_BYTES).map_err(|e| format!("cannot read: {e}"))?;
            one.read = t.elapsed();
            (jpeg, *orientation)
        }
        Item::LoneNef { path, orientation } => {
            let t = Instant::now();
            let mut src = FileSource::open(path).map_err(|e| format!("cannot read: {e}"))?;
            let best = ifd::find_preview(&mut src, &Limits::DEFAULT)
                .map_err(|e| format!("no preview: {e}"))?;
            one.walk = t.elapsed();
            (one.walk_reads, one.walk_bytes) = src.disk_reads();
            drop(src);
            let t = Instant::now();
            let jpeg = nef_preview::read_preview_bytes(path, best.range, MAX_PREVIEW_BYTES)
                .map_err(|e| e.to_string())?;
            one.read = t.elapsed();
            (jpeg, *orientation)
        }
    };
    one.read_bytes = jpeg.len() as u64;
    let p = build_preview(&jpeg, orientation, options).map_err(|e| e.to_string())?;
    one.timings = p.timings;
    one.out_bytes = p.jpeg.len() as u64;
    Ok(one)
}

/// All of `path`, refusing more than `cap` bytes. Read-only.
pub(crate) fn read_all(path: &Path, cap: u64) -> io::Result<Vec<u8>> {
    let file = File::open(path)?;
    let len = file.metadata()?.len();
    if len > cap {
        return Err(io::Error::other(format!(
            "{len} bytes, over the {cap}-byte cap"
        )));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(len).unwrap_or(0));
    file.take(cap).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Every file a group's preview or EXIF may read: the primary and the NEF, in group order.
pub(crate) fn photo_files(root: &Path, groups: &[Group]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for g in groups {
        out.push(path_of(root, &g.primary.entry.components()));
        if let Some(raw) = &g.raw {
            out.push(path_of(root, &raw.entry.components()));
        }
    }
    out
}

/// Progress on stderr: a live line on a terminal, every 100 items otherwise. Best effort.
pub(crate) fn progress(what: &str, done: usize, total: usize) {
    let err = io::stderr();
    let _ = if err.is_terminal() {
        write!(err.lock(), "\r\x1b[2K{what}: {done} of {total}")
    } else if done.is_multiple_of(100) {
        writeln!(err.lock(), "{what}: {done} of {total}")
    } else {
        Ok(())
    };
}

pub(crate) fn progress_done(what: &str, total: usize) {
    let err = io::stderr();
    let _ = if err.is_terminal() {
        writeln!(err.lock(), "\r\x1b[2K{what}: {total} done")
    } else {
        writeln!(err.lock(), "{what}: {total} done")
    };
}

#[cfg(test)]
mod tests {
    use culla_core::grouping::ScanEntry;

    use super::*;

    fn groups(names: &[&str]) -> Vec<Group> {
        let entries: Vec<ScanEntry> = names
            .iter()
            .map(|n| ScanEntry::file(&["DCIM"], n))
            .collect();
        grouping::group(entries, &ScanPolicy::new(true)).groups
    }

    fn path(item: &Item) -> &Path {
        match item {
            Item::Jpeg { path, .. } | Item::LoneNef { path, .. } => path,
        }
    }

    #[test]
    fn pair_reads_the_choice_lone_files_their_own() {
        let root = Path::new("/r");
        let gs = groups(&["A.JPG", "A.NEF", "B.NEF", "C.JPG"]);
        let by = |stem: &str| gs.iter().find(|g| g.stem == stem).unwrap();
        let o = Orientation::Normal;

        let (i, c) = item_for(root, by("A"), o, SourceChoice::Jpeg);
        assert!(matches!(i, Item::Jpeg { .. }));
        assert_eq!(
            (path(&i), c),
            (Path::new("/r/DCIM/A.JPG"), SourceChoice::Jpeg)
        );
        let (i, c) = item_for(root, by("A"), o, SourceChoice::Nef);
        assert!(matches!(i, Item::LoneNef { .. }));
        assert_eq!(
            (path(&i), c),
            (Path::new("/r/DCIM/A.NEF"), SourceChoice::Nef)
        );

        for choice in [SourceChoice::Jpeg, SourceChoice::Nef] {
            let (i, c) = item_for(root, by("B"), o, choice);
            assert_eq!(
                (path(&i), c),
                (Path::new("/r/DCIM/B.NEF"), SourceChoice::Nef)
            );
            let (i, c) = item_for(root, by("C"), o, choice);
            assert_eq!(
                (path(&i), c),
                (Path::new("/r/DCIM/C.JPG"), SourceChoice::Jpeg)
            );
        }
    }

    #[test]
    fn photo_files_are_the_primaries_and_the_nefs() {
        let gs = groups(&["A.JPG", "A.NEF", "B.NEF", "C.JPG"]);
        let mut files = photo_files(Path::new("/r"), &gs);
        files.sort();
        let want: Vec<PathBuf> = ["A.JPG", "A.NEF", "B.NEF", "C.JPG"]
            .iter()
            .map(|n| PathBuf::from("/r/DCIM").join(n))
            .collect();
        assert_eq!(files, want);
    }

    #[test]
    fn choices_parse() {
        assert_eq!(SourceChoice::parse("jpeg"), Some(SourceChoice::Jpeg));
        assert_eq!(SourceChoice::parse("nef"), Some(SourceChoice::Nef));
        assert_eq!(SourceChoice::parse("raw"), None);
        assert_eq!(SourceChoice::Nef.name(), "nef");
    }
}
