//! `culla preview` run as a process (ADR 0020): previews of a built tree, the `--out` rules
//! (new or empty folder, outside the scanned one, files created and never replaced), and that
//! the scanned tree is left exactly as it was.
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

fn preview(folder: &Path, out: Option<&Path>) -> Output {
    let mut args: Vec<&OsStr> = vec!["preview".as_ref(), folder.as_os_str()];
    if let Some(out) = out {
        args.push("--out".as_ref());
        args.push(out.as_os_str());
    }
    culla(&args)
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

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[test]
fn previews_a_card_and_writes_nothing_by_default() {
    let t = TempTree::new("default");
    let card = card(&t);
    let before = snapshot(t.path());
    let o = preview(&card, None);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = stdout(&o);
    assert!(
        out.contains("previews: 2 made, 0 failed, of 2 groups"),
        "{out}"
    );
    assert!(out.contains("written:  nothing (no --out)"), "{out}");
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
fn writes_previews_into_a_new_folder() {
    let t = TempTree::new("new-out");
    let card = card(&t);
    let src_before = snapshot(&card);
    let out_dir = t.path().join("previews");
    let o = preview(&card, Some(&out_dir));
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert!(
        stdout(&o).contains("written:  2 files to"),
        "{}",
        stdout(&o)
    );
    let files = names(&out_dir);
    assert_eq!(files.len(), 2, "{files:?}");
    assert!(
        files.iter().any(|f| f.ends_with("-DSC_0001.jpg")),
        "{files:?}"
    );
    assert!(
        files.iter().any(|f| f.ends_with("-DSC_0002.jpg")),
        "{files:?}"
    );
    for f in &files {
        let bytes = fs::read(out_dir.join(f)).unwrap();
        assert_eq!(&bytes[..2], &[0xFF, 0xD8]);
        assert_eq!(&bytes[bytes.len() - 2..], &[0xFF, 0xD9]);
    }
    assert_eq!(snapshot(&card), src_before, "the source is only read");
}

#[test]
fn writes_into_an_empty_existing_folder() {
    let t = TempTree::new("empty-out");
    let card = card(&t);
    let out_dir = t.dir("out");
    let o = preview(&card, Some(&out_dir));
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert_eq!(names(&out_dir).len(), 2);
}

#[test]
fn refuses_an_output_folder_that_is_not_empty_or_not_a_folder() {
    let t = TempTree::new("bad-out");
    let card = card(&t);
    let busy = t.dir("busy");
    t.file("busy/0001-DSC_0001.jpg", b"keep me");
    let file = t.file("a-file", b"x");
    let before = snapshot(t.path());
    for (out, why) in [
        (busy.clone(), "the output folder is not empty"),
        (file.clone(), "the output folder is not a folder"),
        (
            t.path().join("missing/new"),
            "cannot create the output folder",
        ),
    ] {
        let o = preview(&card, Some(&out));
        assert_eq!(code(&o), 4, "{out:?}: {}", stderr(&o));
        assert!(stderr(&o).contains(why), "{out:?}: {}", stderr(&o));
        assert!(o.stdout.is_empty());
    }
    assert_eq!(
        snapshot(t.path()),
        before,
        "nothing may be written or replaced"
    );
}

#[test]
fn refuses_an_output_folder_inside_the_scanned_folder() {
    let t = TempTree::new("inside-out");
    let card = card(&t);
    let before = snapshot(t.path());
    for out in [
        card.join("previews"),
        card.join("DCIM"),
        card.clone(),
        card.join("x/../previews"),
    ] {
        let o = preview(&card, Some(&out));
        assert_eq!(code(&o), 4, "{out:?}: {}", stderr(&o));
        assert!(
            stderr(&o).contains("inside the scanned folder"),
            "{out:?}: {}",
            stderr(&o)
        );
    }
    assert_eq!(
        snapshot(t.path()),
        before,
        "nothing may be created in the source"
    );
}

#[cfg(unix)]
#[test]
fn refuses_an_output_link() {
    let t = TempTree::new("link-out");
    let card = card(&t);
    let target = t.dir("target");
    let link = t.path().join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let o = preview(&card, Some(&link));
    assert_eq!(code(&o), 4, "{}", stderr(&o));
    assert!(stderr(&o).contains("is a link"), "{}", stderr(&o));
    // A link to a folder inside the source is refused for where it points.
    let inside = t.path().join("inside");
    std::os::unix::fs::symlink(card.join("DCIM"), &inside).unwrap();
    let o = preview(&card, Some(&inside.join("new")));
    assert_eq!(code(&o), 4, "{}", stderr(&o));
    assert!(stderr(&o).contains("inside the scanned folder"));
    assert!(names(&target).is_empty());
}

#[test]
fn a_broken_photo_is_listed_and_the_others_go_on() {
    let t = TempTree::new("broken");
    let card = card(&t);
    t.file("card/DCIM/DSC_0003.JPG", b"not a JPEG at all");
    let out_dir = t.path().join("out");
    let o = preview(&card, Some(&out_dir));
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = stdout(&o);
    assert!(
        out.contains("previews: 2 made, 1 failed, of 3 groups"),
        "{out}"
    );
    assert!(out.contains("failed: preview refused"), "{out}");
    assert_eq!(names(&out_dir).len(), 2);
}

#[test]
fn a_missing_folder_exits_3_and_a_file_exits_2() {
    let t = TempTree::new("missing");
    let o = preview(&t.path().join("nope"), None);
    assert_eq!(code(&o), 3, "{}", stderr(&o));
    let f = t.file("f.JPG", b"x");
    let o = preview(&f, None);
    assert_eq!(code(&o), 2, "{}", stderr(&o));
    // A bad --out next to a missing folder: the output folder is checked first, and nothing
    // is created before the scan succeeds.
    let out = t.path().join("out");
    let o = preview(&t.path().join("nope"), Some(&out));
    assert_eq!(code(&o), 3, "{}", stderr(&o));
    assert!(!out.exists());
}

#[test]
fn help_describes_preview() {
    for args in [&["help", "preview"][..], &["preview", "--help"]] {
        let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
        let o = culla(&args);
        assert_eq!(code(&o), 0);
        let out = stdout(&o);
        assert!(out.starts_with("Usage: culla preview <folder>"), "{out}");
        assert!(out.contains("never replaced"), "{out}");
    }
    let o = culla(&[OsStr::new("help")]);
    let out = stdout(&o);
    assert!(out.contains("culla preview <folder> [--no-subfolders] [--out <dir>]"));
    assert!(out.contains("4  the output folder is refused"));
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
    let t = TempTree::new("real");
    let out_dir = t.path().join("previews");
    let o = preview(&root, Some(&out_dir));
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
    assert_eq!(names(&out_dir).len(), 9);
    assert!(!text.to_ascii_lowercase().contains("gps"));
    assert_eq!(snapshot(&root), before, "the preview changed the samples");
}
