//! The read-only lookups behind `culla plan` (`culla_adapters::facts`) over trees built in a
//! temporary folder: the decision log read, and the target snapshot's fail-closed cases.
//!
//! Temporary trees live under `std::env::temp_dir()` with a unique name and are removed by
//! [`TempTree`] on drop: the only removal in this file, inside its own temp folder (the lint
//! may be switched off in `crates/*/tests/`, scripts/ci/check_lint_allows.py).

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use culla_adapters::facts::{ReadTextError, read_text, snapshot_targets};
use culla_core::plan::{Existing, Root};
use culla_core::preflight::FolderFact;

/// A uniquely named folder under the system temp folder, removed with everything in it on drop.
struct TempTree(PathBuf);

impl TempTree {
    fn new(test: &str) -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!(
            "culla-facts-{test}-{}-{nanos}-{}",
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

fn folders(list: &[&[&str]]) -> BTreeSet<Vec<String>> {
    list.iter()
        .map(|f| f.iter().map(|s| (*s).to_owned()).collect())
        .collect()
}

fn parts(p: &[&str]) -> Vec<String> {
    p.iter().map(|s| (*s).to_owned()).collect()
}

fn fact_of<'a>(facts: &'a [(Vec<String>, FolderFact)], p: &[&str]) -> Option<&'a FolderFact> {
    facts.iter().find(|(f, _)| *f == parts(p)).map(|(_, f)| f)
}

// ---------------------------------------------------------------------------------------------
// read_text
// ---------------------------------------------------------------------------------------------

#[test]
fn read_text_reads_a_file_up_to_the_cap() {
    let t = TempTree::new("text");
    let p = t.file("log.jsonl", b"0123456789");
    assert_eq!(read_text(&p, 10).unwrap(), "0123456789");
    assert!(matches!(
        read_text(&p, 9),
        Err(ReadTextError::TooLarge { max: 9 })
    ));
    assert!(matches!(
        read_text(&p, 0),
        Err(ReadTextError::TooLarge { max: 0 })
    ));
}

#[test]
fn read_text_refuses_what_is_not_a_utf8_regular_file() {
    let t = TempTree::new("text-bad");
    let bad = t.file("bad.jsonl", &[0xff, 0xfe, b'\n']);
    assert!(matches!(read_text(&bad, 64), Err(ReadTextError::NotUtf8)));
    let dir = t.dir("folder");
    assert!(matches!(read_text(&dir, 64), Err(ReadTextError::NotAFile)));
    assert!(matches!(
        read_text(&t.path().join("none"), 64),
        Err(ReadTextError::Io(_))
    ));
    #[cfg(unix)]
    {
        let target = t.file("real.jsonl", b"{}\n");
        let link = t.path().join("link.jsonl");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(matches!(read_text(&link, 64), Err(ReadTextError::NotAFile)));
    }
}

// ---------------------------------------------------------------------------------------------
// snapshot_targets
// ---------------------------------------------------------------------------------------------

#[test]
fn a_listing_over_max_entries_is_unreadable() {
    let t = TempTree::new("cap");
    t.file("full/a.JPG", b"a");
    t.file("full/b.JPG", b"b");
    t.file("full/c.JPG", b"c");
    t.file("ok/a.JPG", b"a");
    t.file("ok/b.JPG", b"b");
    let mut existing = Existing::new();
    let got = snapshot_targets(
        t.path(),
        Root::Dest,
        &folders(&[&["full"], &["ok"]]),
        2,
        &mut existing,
    );
    assert_eq!(
        fact_of(&got.folders, &["full"]),
        Some(&FolderFact::Unreadable)
    );
    assert!(matches!(
        fact_of(&got.folders, &["ok"]),
        Some(FolderFact::Exists(_))
    ));
    // Exactly at the cap is listed in full.
    assert!(existing.is_taken(Root::Dest, &parts(&["ok", "a.JPG"])));
    assert!(existing.is_taken(Root::Dest, &parts(&["ok", "b.JPG"])));
    // Over it, none of the entries is recorded.
    assert!(!existing.is_taken(Root::Dest, &parts(&["full", "a.JPG"])));
}

#[test]
fn a_file_where_a_target_folder_should_be_blocks_it() {
    let t = TempTree::new("file-folder");
    t.file("100", b"not a folder");
    let mut existing = Existing::new();
    let got = snapshot_targets(
        t.path(),
        Root::Dest,
        &folders(&[&["100", "sub"]]),
        10,
        &mut existing,
    );
    assert!(existing.is_blocked(Root::Dest, &parts(&["100", "sub"])));
    assert!(existing.is_blocked(Root::Dest, &parts(&["100"])));
    assert_eq!(fact_of(&got.folders, &["100"]), None);
    assert_eq!(fact_of(&got.folders, &["100", "sub"]), None);
}

#[test]
fn a_file_where_rejected_should_be_is_unreadable() {
    let t = TempTree::new("file-rejected");
    let rejected = t.file("_rejected", b"not a folder");
    let mut existing = Existing::new();
    let got = snapshot_targets(
        &rejected,
        Root::Rejected,
        &folders(&[&["100"]]),
        10,
        &mut existing,
    );
    assert_eq!(got.folders, vec![(Vec::new(), FolderFact::Unreadable)]);
    assert!(!existing.has_entries(Root::Rejected));
}

#[test]
fn a_second_walk_stops_at_a_file_it_has_seen() {
    let t = TempTree::new("rewalk-file");
    t.file("f", b"a file");
    let mut existing = Existing::new();
    let got = snapshot_targets(
        t.path(),
        Root::Dest,
        &folders(&[&["f", "x"], &["f", "y"]]),
        10,
        &mut existing,
    );
    // Only the root is a folder fact; neither walk goes below the file.
    assert_eq!(got.folders.len(), 1, "{:?}", got.folders);
    assert!(existing.is_blocked(Root::Dest, &parts(&["f", "y"])));
}

#[cfg(unix)]
#[test]
fn a_second_walk_never_enters_a_link_it_has_seen() {
    let t = TempTree::new("rewalk-link");
    let outside = t.dir("outside");
    t.file("outside/x/DSC_0001.JPG", b"x");
    t.file("outside/y/DSC_0002.JPG", b"y");
    let dest = t.dir("dest");
    std::os::unix::fs::symlink(&outside, dest.join("l")).unwrap();
    let mut existing = Existing::new();
    let got = snapshot_targets(
        &dest,
        Root::Dest,
        &folders(&[&["l", "x"], &["l", "y"]]),
        10,
        &mut existing,
    );
    match fact_of(&got.folders, &["l"]) {
        Some(FolderFact::Exists(a)) => assert!(a.reparse_point),
        other => panic!("{other:?}"),
    }
    assert_eq!(got.folders.len(), 2, "{:?}", got.folders);
    for p in [
        &["l", "x"][..],
        &["l", "y"],
        &["l", "x", "DSC_0001.JPG"],
        &["l", "y", "DSC_0002.JPG"],
    ] {
        assert!(!existing.is_taken(Root::Dest, &parts(p)), "{p:?}");
    }
}
