//! `culla preview` run as a process (ADR 0020): previews of a built tree, made, timed and
//! dropped, and the tree (and the folder around it) left exactly as it was: the command
//! writes nothing but stdout and stderr (file-engine INV-03).
//!
//! Temporary trees live under `std::env::temp_dir()` with a unique name, are built with
//! `create_new` only, and are removed by [`TempTree`] on drop: the only removal in this file,
//! inside its own temp folder (the lint may be switched off in `crates/*/tests/`).
//!
//! The author's samples are read only by the ignored test at the end, when
//! `CULLA_REAL_SAMPLES` names their folder:
//! `CULLA_REAL_SAMPLES=/path/to/samples cargo test -p culla-cli --test preview -- --ignored`

use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

fn synthetic() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/synthetic")
}

fn culla(args: &[&OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_culla"))
        .args(args)
        .output()
        .expect("run culla")
}

fn preview(folder: &Path) -> Output {
    culla(&["preview".as_ref(), folder.as_os_str()])
}

fn stdout(o: &Output) -> String {
    String::from_utf8(o.stdout.clone()).expect("stdout is UTF-8")
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn code(o: &Output) -> i32 {
    o.status.code().expect("exited normally")
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
            "culla-cli-preview-{test}-{}-{nanos}-{}",
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
}

impl Drop for TempTree {
    #[allow(clippy::disallowed_methods, reason = "test temp folder only")]
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

type Snap = (
    PathBuf,
    bool,
    u64,
    Option<SystemTime>,
    bool,
    Option<Vec<u8>>,
);

/// Every file and folder under `dir`, with the bytes of every file, without following links.
fn snapshot(dir: &Path) -> Vec<Snap> {
    fn walk(dir: &Path, out: &mut Vec<Snap>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let meta = fs::symlink_metadata(&path).unwrap();
            let bytes = meta.is_file().then(|| fs::read(&path).unwrap());
            out.push((
                path.clone(),
                meta.is_dir(),
                meta.len(),
                meta.modified().ok(),
                meta.permissions().readonly(),
                bytes,
            ));
            if meta.is_dir() {
                walk(&path, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, &mut out);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn fixture(rel: &str) -> Vec<u8> {
    fs::read(synthetic().join(rel)).unwrap()
}

/// A card: a pair and a lone NEF in a subfolder.
fn card(t: &TempTree) -> PathBuf {
    t.file("card/DCIM/DSC_0001.JPG", &fixture("pair/DSC_0001.JPG"));
    t.file("card/DCIM/DSC_0001.NEF", &fixture("pair/DSC_0001.NEF"));
    t.file("card/DCIM/sub/DSC_0002.NEF", &fixture("lone/DSC_0002.NEF"));
    t.path().join("card")
}

#[test]
fn previews_a_card_and_writes_nothing() {
    let t = TempTree::new("default");
    let card = card(&t);
    let before = snapshot(t.path());
    let o = preview(&card);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = stdout(&o);
    assert!(
        out.contains("previews: 2 made, 0 failed, of 2 groups"),
        "{out}"
    );
    assert!(out.contains("written:  nothing (read-only)"), "{out}");
    assert!(
        out.contains("DCIM/DSC_0001.JPG") || out.contains("DCIM\\DSC_0001.JPG"),
        "{out}"
    );
    assert!(
        out.contains("  jpeg  ") && out.contains("  nef   "),
        "{out}"
    );
    assert!(out.contains("prefetch 5 ahead and 2 behind"), "{out}");
    assert!(out.contains("wait:     p50 "), "{out}");
    assert!(stderr(&o).contains("previewed: 2"), "{}", stderr(&o));
    assert_eq!(snapshot(t.path()), before, "nothing may be written");
}

#[test]
fn a_broken_photo_is_listed_and_the_others_go_on() {
    let t = TempTree::new("broken");
    let card = card(&t);
    t.file("card/DCIM/DSC_0003.JPG", b"not a JPEG at all");
    let before = snapshot(t.path());
    let o = preview(&card);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = stdout(&o);
    assert!(
        out.contains("previews: 2 made, 1 failed, of 3 groups"),
        "{out}"
    );
    assert!(out.contains("failed: preview refused"), "{out}");
    assert_eq!(snapshot(t.path()), before, "nothing may be written");
}

#[test]
fn a_missing_folder_exits_3_and_a_file_exits_2() {
    let t = TempTree::new("missing");
    let o = preview(&t.path().join("nope"));
    assert_eq!(code(&o), 3, "{}", stderr(&o));
    let f = t.file("f.JPG", b"x");
    let o = preview(&f);
    assert_eq!(code(&o), 2, "{}", stderr(&o));
}

#[test]
fn there_is_no_out_option() {
    let t = TempTree::new("no-out");
    let card = card(&t);
    let before = snapshot(t.path());
    let out = t.path().join("previews");
    let o = culla(&[
        "preview".as_ref(),
        card.as_os_str(),
        "--out".as_ref(),
        out.as_os_str(),
    ]);
    assert_eq!(code(&o), 1, "{}", stderr(&o));
    assert!(
        stderr(&o).contains("unknown option '--out'"),
        "{}",
        stderr(&o)
    );
    assert!(o.stdout.is_empty());
    assert_eq!(snapshot(t.path()), before, "nothing may be written");
}

#[test]
fn help_describes_preview() {
    for args in [&["help", "preview"][..], &["preview", "--help"]] {
        let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
        let o = culla(&args);
        assert_eq!(code(&o), 0);
        let out = stdout(&o);
        assert!(out.starts_with("Usage: culla preview <folder>"), "{out}");
        assert!(out.contains("nothing is written"), "{out}");
        assert!(!out.contains("--out"), "{out}");
    }
    let o = culla(&[OsStr::new("help")]);
    let out = stdout(&o);
    assert!(out.contains("culla preview <folder> [--no-subfolders]\n"));
    assert!(!out.contains("--out") && !out.contains("  4  "), "{out}");
}

#[test]
#[ignore = "needs the author's samples: set CULLA_REAL_SAMPLES"]
fn real_samples_preview_at_screen_size() {
    let Ok(root) = std::env::var("CULLA_REAL_SAMPLES") else {
        println!("skipped: CULLA_REAL_SAMPLES is not set");
        return;
    };
    let root = PathBuf::from(root);
    let before = snapshot(&root);
    let o = preview(&root);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let text = stdout(&o);
    println!("{text}");
    assert!(
        text.contains("previews: 9 made, 0 failed, of 9 groups"),
        "{text}"
    );
    for line in text
        .lines()
        .filter(|l| l.contains(".JPG") || l.contains(".NEF"))
    {
        assert!(
            line.contains("2700x1800") || line.contains("1800x2700"),
            "not screen sized: {line}"
        );
    }
    assert!(!text.to_ascii_lowercase().contains("gps"));
    assert_eq!(snapshot(&root), before, "the preview changed the samples");
}
