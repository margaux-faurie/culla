//! The streamed scan over real folders: the committed synthetic tree and trees built in a
//! temporary folder (stories US-01 to US-05, design/file-engine.md §3).
//!
//! Temporary trees live under `std::env::temp_dir()` with a unique name and are removed by
//! [`TempTree`] on drop: the only removal in this file, inside its own temp folder (the lint
//! may be switched off in `crates/*/tests/`, scripts/ci/check_lint_allows.py).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use culla_adapters::scan::{ScanBatch, ScanError, scan, scan_and_group};
use culla_core::grouping::{
    Describes, EntryKind, ExclusionReason, GroupStatus, Grouping, PhotoKind, ScanPolicy,
    SidecarKind,
};

fn synthetic() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/synthetic")
}

fn all() -> ScanPolicy {
    ScanPolicy {
        include_subfolders: true,
        destination: None,
    }
}

/// A uniquely named folder under the system temp folder, removed with everything in it on drop.
struct TempTree(PathBuf);

impl TempTree {
    fn new(test: &str) -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!(
            "culla-scan-{test}-{}-{nanos}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    /// Creates a new file (never an existing one) with `bytes`, and its folders.
    fn file(&self, rel: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        f.write_all(bytes).unwrap();
        path
    }

    fn dir(&self, rel: &str) -> PathBuf {
        let path = self.0.join(rel);
        fs::create_dir_all(&path).unwrap();
        path
    }
}

impl Drop for TempTree {
    #[allow(clippy::disallowed_methods, reason = "test temp folder only")]
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Every file and folder under `dir` with its size and mtime, without following links.
fn snapshot(dir: &Path, out: &mut Vec<(PathBuf, u64, Option<SystemTime>, bool)>) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let meta = fs::symlink_metadata(entry.path()).unwrap();
        out.push((
            entry.path(),
            meta.len(),
            meta.modified().ok(),
            meta.permissions().readonly(),
        ));
        if meta.is_dir() {
            snapshot(&entry.path(), out);
        }
    }
    out.sort();
}

fn groups(g: &Grouping) -> Vec<String> {
    g.groups
        .iter()
        .map(|g| format!("{}|{}", g.folder.join("\\"), g.stem))
        .collect()
}

fn unknown(g: &Grouping) -> Vec<(String, &'static str)> {
    g.unknown
        .iter()
        .map(|u| (u.entry.rel_path(), u.kind.issue_kind()))
        .collect()
}

fn excluded(g: &Grouping) -> Vec<(String, ExclusionReason)> {
    g.excluded
        .iter()
        .map(|x| (x.entry.rel_path(), x.reason))
        .collect()
}

fn batches(root: &Path, policy: &ScanPolicy) -> Vec<ScanBatch> {
    let mut out = Vec::new();
    scan(root, policy, |b| {
        out.push(b);
        ControlFlow::Continue(())
    })
    .unwrap();
    out
}

#[test]
fn synthetic_tree_groups_pairs_and_lone_nefs() {
    let (g, summary) = scan_and_group(&synthetic(), &all()).unwrap();
    assert_eq!(
        groups(&g),
        [
            "broken|ifd-loop",
            "broken|truncated",
            "lone|DSC_0002",
            "pair|DSC_0001"
        ]
    );
    assert!(g.unknown.is_empty(), "{:?}", g.unknown);
    assert!(g.excluded.is_empty(), "{:?}", g.excluded);
    let pair = &g.groups[3];
    assert_eq!(pair.primary.kind, PhotoKind::Jpeg);
    assert_eq!(pair.raw.as_ref().unwrap().entry.name, "DSC_0001.NEF");
    assert!(pair.primary.entry.size > 0 && pair.primary.entry.mtime_ns.is_some());
    let lone = &g.groups[2];
    assert!(lone.is_lone_nef() && lone.raw.is_none());
    assert!(g.groups.iter().all(|g| g.status == GroupStatus::Ok));
    assert_eq!(summary.folders, 4);
    assert!(!summary.stopped);

    // Subfolders off: the root has no files, its subfolders are counted only.
    let (g, _) = scan_and_group(&synthetic(), &ScanPolicy::default()).unwrap();
    assert!(g.groups.is_empty());
    assert_eq!(g.excluded.len(), 3);
    assert!(
        g.excluded
            .iter()
            .all(|x| x.reason == ExclusionReason::NotIncluded)
    );
}

/// The tree most tests below use. Returns (tree, source root).
fn sample_tree(test: &str) -> (TempTree, PathBuf) {
    let t = TempTree::new(test);
    for rel in [
        "src/DSC_0001.JPG",
        "src/dsc_0001.nef",
        "src/DSC_0001.xmp",
        "src/DSC_0001.NEF.xmp",
        "src/NKSC_PARAM/DSC_0001.NEF.nksc",
        "src/NKSC_PARAM/notes.txt",
        "src/NKSC_PARAM/inner/I.JPG",
        "src/DSC_0002.NEF",
        "src/CLIP_0001.MOV",
        "src/DSC_0001.WAV",
        "src/.DS_Store",
        "src/DSC_0003.JPG.4-1.culla-partial",
        "src/_rejected/DSC_0009.JPG",
        "src/100NIKON/DSC_0001.JPG",
        "src/101NIKON/DSC_0001.NEF",
        "src/100NIKON/sub/A.JPG",
        "src/Keep/K.JPG",
        "src/Été/café.JPG",
        "src/Été/CAFÉ.NEF",
        "outside/O.JPG",
    ] {
        t.file(rel, rel.as_bytes());
    }
    t.dir("src/empty");
    let src = t.path().join("src");
    (t, src)
}

#[test]
fn us02_to_us05_on_a_real_tree() {
    let (t, src) = sample_tree("tree");
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(t.path().join("outside"), src.join("link_dir")).unwrap();
        symlink(src.join("DSC_0001.JPG"), src.join("link_file.NEF")).unwrap();
    }
    let policy = ScanPolicy {
        include_subfolders: true,
        destination: Some(vec!["keep".into()]),
    };
    let mut before = Vec::new();
    snapshot(t.path(), &mut before);

    let (g, _) = scan_and_group(&src, &policy).unwrap();

    assert_eq!(
        groups(&g),
        [
            "|DSC_0001",
            "|DSC_0002",
            "100NIKON|DSC_0001",
            "100NIKON\\sub|A",
            "101NIKON|DSC_0001",
            "Été|café"
        ]
    );
    let pair = &g.groups[0];
    assert_eq!(pair.raw.as_ref().unwrap().entry.name, "dsc_0001.nef");
    let sidecars: Vec<_> = pair
        .sidecars
        .iter()
        .map(|s| (s.entry.rel_path(), s.kind, pair.follows(s)))
        .collect();
    assert_eq!(
        sidecars,
        [
            (
                "DSC_0001.NEF.xmp".into(),
                SidecarKind::FileXmp,
                Describes::Raw
            ),
            ("DSC_0001.xmp".into(), SidecarKind::Xmp, Describes::Raw),
            (
                "NKSC_PARAM\\DSC_0001.NEF.nksc".into(),
                SidecarKind::Nksc,
                Describes::Raw
            ),
        ]
    );
    assert!(g.groups[1].is_lone_nef());
    // US-02: same name in two folders, two groups; each is alone in its folder.
    assert!(g.groups[2].raw.is_none() && g.groups[4].is_lone_nef());
    // Case-insensitive pairing beyond ASCII, on a case-sensitive file system.
    assert_eq!(g.groups[5].raw.as_ref().unwrap().entry.name, "CAFÉ.NEF");

    assert_eq!(
        unknown(&g),
        [
            ("CLIP_0001.MOV".into(), "unknown_ext"),
            ("DSC_0001.WAV".into(), "same_basename_unknown"),
            ("NKSC_PARAM\\notes.txt".into(), "unknown_ext"),
            ("NKSC_PARAM\\inner\\I.JPG".into(), "unknown_ext"),
        ]
    );
    let mut want = vec![
        (".DS_Store".to_owned(), ExclusionReason::HiddenOrSystem),
        (
            "DSC_0003.JPG.4-1.culla-partial".to_owned(),
            ExclusionReason::CullaPartial,
        ),
        ("Keep".to_owned(), ExclusionReason::Destination),
    ];
    if cfg!(unix) {
        want.push(("link_dir".into(), ExclusionReason::ReparsePoint));
        want.push(("link_file.NEF".into(), ExclusionReason::ReparsePoint));
    }
    want.push(("_rejected".into(), ExclusionReason::RejectedFolder));
    assert_eq!(excluded(&g), want);

    // Nothing behind the link, under _rejected or under the destination was even listed.
    let all_names: Vec<&str> = g
        .groups
        .iter()
        .flat_map(|g| g.files())
        .chain(g.unknown.iter().map(|u| &u.entry))
        .chain(g.excluded.iter().map(|x| &x.entry))
        .map(|e| e.name.as_str())
        .collect();
    for name in ["O.JPG", "DSC_0009.JPG", "K.JPG"] {
        assert!(!all_names.contains(&name), "{name}");
    }

    // Read-only: the tree is exactly as it was.
    let mut after = Vec::new();
    snapshot(t.path(), &mut after);
    assert_eq!(before, after);
}

#[test]
fn us01_batches_come_folder_by_folder_in_walk_order() {
    let (_t, src) = sample_tree("batches");
    let got = batches(&src, &all());
    let folders: Vec<String> = got.iter().map(|b| b.folder.join("\\")).collect();
    assert_eq!(
        folders,
        [
            "",
            "100NIKON",
            "100NIKON\\sub",
            "101NIKON",
            "empty",
            "Keep",
            "NKSC_PARAM\\inner",
            "Été"
        ]
    );
    // NKSC_PARAM's entries travel with the root folder, after the root's own entries.
    let root: Vec<String> = got[0].entries.iter().map(|e| e.rel_path()).collect();
    let n = root.len();
    assert_eq!(
        root[n - 3..],
        [
            "NKSC_PARAM\\DSC_0001.NEF.nksc",
            "NKSC_PARAM\\inner",
            "NKSC_PARAM\\notes.txt"
        ]
    );
    // The root's own entries are in folded-name order.
    assert_eq!(root[..3], [".DS_Store", "100NIKON", "101NIKON"]);
    assert!(got[4].entries.is_empty());
    // Each batch groups to that folder's final groups.
    let parts: Vec<Grouping> = got
        .into_iter()
        .map(|b| culla_core::grouping::group(b.entries, &all()))
        .collect();
    assert_eq!(parts[0].groups.len(), 2);
    let (whole, summary) = scan_and_group(&src, &all()).unwrap();
    assert_eq!(Grouping::merge(parts), whole);
    // Root, NKSC_PARAM and the seven folders above it.
    assert_eq!(summary.folders, 9);
}

#[test]
fn us01_the_callback_can_stop_the_scan() {
    let (_t, src) = sample_tree("stop");
    let mut seen = 0;
    let summary = scan(&src, &all(), |_| {
        seen += 1;
        ControlFlow::Break(())
    })
    .unwrap();
    assert_eq!(seen, 1);
    assert!(summary.stopped);
}

#[test]
fn subfolders_off_reads_the_root_and_its_nksc_param_only() {
    let (_t, src) = sample_tree("flat");
    let got = batches(&src, &ScanPolicy::default());
    assert_eq!(got.len(), 1);
    let g = culla_core::grouping::group(got[0].entries.clone(), &ScanPolicy::default());
    assert_eq!(groups(&g), ["|DSC_0001", "|DSC_0002"]);
    assert_eq!(g.groups[0].sidecars.len(), 3);
    // NKSC_PARAM\inner is not entered either.
    assert!(
        g.excluded
            .iter()
            .any(|x| x.entry.rel_path() == "NKSC_PARAM\\inner"
                && x.reason == ExclusionReason::NotIncluded)
    );
}

#[test]
fn the_root_must_be_a_real_folder() {
    let t = TempTree::new("root");
    let file = t.file("a.JPG", b"x");
    assert!(matches!(
        scan(&file, &all(), |_| ControlFlow::Continue(())),
        Err(ScanError::NotADirectory)
    ));
    let missing = t.path().join("missing");
    assert!(matches!(
        scan(&missing, &all(), |_| ControlFlow::Continue(())),
        Err(ScanError::Io(_))
    ));
    #[cfg(unix)]
    {
        let real = t.dir("real");
        let link = t.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(matches!(
            scan(&link, &all(), |_| ControlFlow::Continue(())),
            Err(ScanError::Refused(ExclusionReason::ReparsePoint))
        ));
    }
}

#[cfg(unix)]
#[test]
fn names_that_are_not_unicode_are_excluded() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let t = TempTree::new("lossy");
    let name = OsStr::from_bytes(b"DSC_\xff.JPG");
    let path = t.path().join(name);
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    let (g, _) = scan_and_group(t.path(), &all()).unwrap();
    assert!(g.groups.is_empty());
    assert_eq!(g.excluded.len(), 1);
    let x = &g.excluded[0];
    assert!(x.entry.name_lossy);
    assert_eq!(x.reason, ExclusionReason::InvalidName);
}

#[test]
fn entries_carry_size_and_kind() {
    let t = TempTree::new("meta");
    t.file("X.JPG", &[0u8; 1234]);
    t.dir("sub");
    let got = batches(t.path(), &ScanPolicy::default());
    let e = &got[0].entries;
    assert_eq!(e.len(), 2);
    assert_eq!(e[0].name, "sub");
    assert_eq!(e[0].kind, EntryKind::Dir);
    assert_eq!(e[1].kind, EntryKind::File);
    assert_eq!(e[1].size, 1234);
    assert!(e[1].mtime_ns.is_some_and(|t| t > 0));
    assert!(!e[1].attrs.reparse_point && !e[1].attrs.cloud_placeholder);
}
