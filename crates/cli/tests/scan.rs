//! `culla scan`, `culla help` and `culla --version` run as a process (ADR 0020): exit codes,
//! text and JSON output, and that a scan leaves the tree exactly as it was.
//!
//! Temporary trees live under `std::env::temp_dir()` with a unique name, are built with
//! `create_new` only, and are removed by [`TempTree`] on drop: the only removal in this file,
//! inside its own temp folder (the lint may be switched off in `crates/*/tests/`,
//! scripts/ci/check_lint_allows.py).
//!
//! The author's samples are read only by the ignored test at the end, when
//! `CULLA_REAL_SAMPLES` names their folder:
//! `CULLA_REAL_SAMPLES=/path/to/samples cargo test -p culla-cli --test scan -- --ignored`

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

fn synthetic() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/synthetic")
}

fn culla(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_culla"))
        .args(args)
        .output()
        .expect("run culla")
}

fn run(args: &[&str]) -> Output {
    let args: Vec<&std::ffi::OsStr> = args.iter().map(std::ffi::OsStr::new).collect();
    culla(&args)
}

fn scan(folder: &Path, extra: &[&str]) -> Output {
    let mut args: Vec<&std::ffi::OsStr> = vec!["scan".as_ref()];
    args.extend(extra.iter().map(std::ffi::OsStr::new));
    args.push(folder.as_os_str());
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
            "culla-cli-{test}-{}-{nanos}-{}",
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

/// One entry of a tree snapshot: path, whether it is a folder, size, mtime, read-only flag
/// and, for a file, its bytes.
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

/// The bytes of a committed synthetic fixture (a JPEG with full EXIF, a NEF).
fn fixture(rel: &str) -> Vec<u8> {
    fs::read(synthetic().join(rel)).unwrap()
}

// A tiny JSON reader, enough to check `--json` output without a dependency.

#[derive(Clone, Debug, PartialEq)]
enum J {
    Null,
    Bool(bool),
    Num(i128),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

impl J {
    fn get(&self, key: &str) -> &J {
        match self {
            J::Obj(fields) => {
                &fields
                    .iter()
                    .find(|(k, _)| k == key)
                    .unwrap_or_else(|| panic!("no key {key}"))
                    .1
            }
            _ => panic!("not an object: {self:?}"),
        }
    }

    fn at(&self, i: usize) -> &J {
        match self {
            J::Arr(items) => &items[i],
            _ => panic!("not an array: {self:?}"),
        }
    }

    fn arr(&self) -> &[J] {
        match self {
            J::Arr(items) => items,
            _ => panic!("not an array: {self:?}"),
        }
    }

    fn str(&self) -> &str {
        match self {
            J::Str(s) => s,
            _ => panic!("not a string: {self:?}"),
        }
    }

    fn num(&self) -> i128 {
        match self {
            J::Num(n) => *n,
            _ => panic!("not a number: {self:?}"),
        }
    }
}

fn parse_json(text: &str) -> J {
    let chars: Vec<char> = text.chars().collect();
    let mut p = 0;
    let v = value(&chars, &mut p);
    ws(&chars, &mut p);
    assert_eq!(p, chars.len(), "trailing data after JSON");
    v
}

fn ws(c: &[char], p: &mut usize) {
    while *p < c.len() && c[*p].is_ascii_whitespace() {
        *p += 1;
    }
}

fn expect(c: &[char], p: &mut usize, s: &str) {
    for ch in s.chars() {
        assert_eq!(c.get(*p), Some(&ch), "bad JSON at {}", *p);
        *p += 1;
    }
}

fn value(c: &[char], p: &mut usize) -> J {
    ws(c, p);
    match c[*p] {
        'n' => {
            expect(c, p, "null");
            J::Null
        }
        't' => {
            expect(c, p, "true");
            J::Bool(true)
        }
        'f' => {
            expect(c, p, "false");
            J::Bool(false)
        }
        '"' => J::Str(string(c, p)),
        '[' => {
            *p += 1;
            let mut items = Vec::new();
            ws(c, p);
            if c[*p] == ']' {
                *p += 1;
                return J::Arr(items);
            }
            loop {
                items.push(value(c, p));
                ws(c, p);
                *p += 1;
                match c[*p - 1] {
                    ',' => {}
                    ']' => return J::Arr(items),
                    other => panic!("bad JSON array: {other}"),
                }
            }
        }
        '{' => {
            *p += 1;
            let mut fields = Vec::new();
            ws(c, p);
            if c[*p] == '}' {
                *p += 1;
                return J::Obj(fields);
            }
            loop {
                ws(c, p);
                let k = string(c, p);
                ws(c, p);
                expect(c, p, ":");
                fields.push((k, value(c, p)));
                ws(c, p);
                *p += 1;
                match c[*p - 1] {
                    ',' => {}
                    '}' => return J::Obj(fields),
                    other => panic!("bad JSON object: {other}"),
                }
            }
        }
        _ => {
            let start = *p;
            while *p < c.len() && (c[*p] == '-' || c[*p].is_ascii_digit()) {
                *p += 1;
            }
            let s: String = c[start..*p].iter().collect();
            J::Num(s.parse().unwrap_or_else(|_| panic!("bad JSON number {s}")))
        }
    }
}

fn string(c: &[char], p: &mut usize) -> String {
    expect(c, p, "\"");
    let mut s = String::new();
    loop {
        let ch = c[*p];
        *p += 1;
        match ch {
            '"' => return s,
            '\\' => {
                let e = c[*p];
                *p += 1;
                match e {
                    '"' => s.push('"'),
                    '\\' => s.push('\\'),
                    '/' => s.push('/'),
                    'n' => s.push('\n'),
                    'r' => s.push('\r'),
                    't' => s.push('\t'),
                    'b' => s.push('\u{8}'),
                    'f' => s.push('\u{c}'),
                    'u' => {
                        let hex: String = c[*p..*p + 4].iter().collect();
                        *p += 4;
                        let n = u32::from_str_radix(&hex, 16).unwrap();
                        s.push(char::from_u32(n).expect("no surrogates in culla output"));
                    }
                    other => panic!("bad JSON escape {other}"),
                }
            }
            ch => {
                assert!(
                    u32::from(ch) >= 0x20,
                    "raw control character in a JSON string"
                );
                s.push(ch);
            }
        }
    }
}

fn primaries(j: &J) -> Vec<&str> {
    j.get("groups")
        .arr()
        .iter()
        .map(|g| g.get("primary").str())
        .collect()
}

// ---------------------------------------------------------------------------------------------
// help, version, usage
// ---------------------------------------------------------------------------------------------

#[test]
fn version_prints_the_crate_version() {
    for flag in ["--version", "-V"] {
        let o = run(&[flag]);
        assert_eq!(code(&o), 0);
        assert_eq!(stdout(&o), format!("culla {}\n", env!("CARGO_PKG_VERSION")));
    }
}

#[test]
fn help_lists_commands_and_exit_codes() {
    for args in [&["help"][..], &["--help"], &["-h"]] {
        let o = run(args);
        assert_eq!(code(&o), 0);
        let out = stdout(&o);
        assert!(
            out.contains("culla scan <folder> [--no-subfolders] [--json]"),
            "{out}"
        );
        assert!(out.contains("Exit codes:"), "{out}");
        assert!(o.stderr.is_empty());
    }
    for args in [&["help", "scan"][..], &["scan", "--help"]] {
        let o = run(args);
        assert_eq!(code(&o), 0);
        assert!(stdout(&o).starts_with("Usage: culla scan <folder>"));
    }
}

#[test]
fn usage_errors_exit_1_with_usage_on_stderr() {
    for (args, msg) in [
        (&[][..], "culla: no command given"),
        (&["apply"], "culla: unknown command 'apply'"),
        (&["scan"], "culla: no folder given"),
        (&["scan", "a", "b"], "culla: unexpected argument 'b'"),
        (
            &["scan", "--recursive", "a"],
            "culla: unknown option '--recursive'",
        ),
        (
            &["scan", "--json", "--json", "a"],
            "culla: '--json' given twice",
        ),
    ] {
        let o = run(args);
        assert_eq!(code(&o), 1, "{args:?}");
        assert!(o.stdout.is_empty(), "{args:?}");
        let err = stderr(&o);
        assert!(err.starts_with(msg), "{args:?}: {err}");
        assert!(err.contains("Usage:"), "{args:?}: {err}");
    }
}

// ---------------------------------------------------------------------------------------------
// The synthetic tree
// ---------------------------------------------------------------------------------------------

#[test]
fn synthetic_tree_as_text() {
    let before = snapshot(&synthetic());
    let o = scan(&synthetic(), &[]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = stdout(&o);
    for line in [
        "subfolders:  included",
        "scanned:     5 folders, 14 entries",
        "  groups          9",
        "  pairs           1",
        "  lone NEFs       4",
        "  JPEG only       4",
        "  ambiguous       0",
        "  held back       0",
        "  unknown files   0",
        "  excluded        0",
        "photos in capture-time order",
        // EXIF times first (2025), in time order; file times (the checkout) after.
        "1. exif\\DSC_0101.JPG",
        "   captured     2025-02-15T03:59:50.550+11:00 (DateTimeOriginal)",
        "   orientation  6 (rotate 90° clockwise)",
        "   lens         NIKKOR Z 24-200mm f/4-6.3 VR",
        "   files        exif\\DSC_0101.JPG",
        "2. exif\\DSC_0105.NEF",
        "3. exif\\DSC_0102.JPG",
        "   captured     2025-05-11T01:12:27.370-03:30 (CreateDate)",
        "   lens         24-200mm f/4-6.3",
        "   issues       no_exif",
        "   files        pair\\DSC_0001.JPG, pair\\DSC_0001.NEF",
    ] {
        assert!(out.lines().any(|l| l == line), "missing {line:?} in\n{out}");
    }
    assert_eq!(out.matches(" (file time)").count(), 6, "{out}");
    if cfg!(windows) {
        assert!(out.contains("\nvolume:      serial "), "{out}");
        assert!(out.contains("\nsource key:  "), "{out}");
    } else {
        assert!(out.contains(
            "volume:      unavailable (volume lookups are only supported on Windows)\n\
             source key:  unavailable\n"
        ));
    }
    let err = stderr(&o);
    assert!(err.contains("scanned: 5 folders, 14 entries"), "{err}");
    assert!(err.contains("read metadata: 9 groups"), "{err}");
    assert_eq!(snapshot(&synthetic()), before, "the scan changed the tree");
}

#[test]
fn synthetic_tree_as_json() {
    let before = snapshot(&synthetic());
    let o = scan(&synthetic(), &["--json"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let j = parse_json(&stdout(&o));
    assert_eq!(j.get("culla_version").str(), env!("CARGO_PKG_VERSION"));
    assert_eq!(j.get("include_subfolders"), &J::Bool(true));
    if cfg!(windows) {
        let v = j.get("volume");
        assert_ne!(v.get("serial_number").num(), 0);
        assert_eq!(v.get("serial").str().len(), 9);
        assert_eq!(j.get("identity_unavailable"), &J::Null);
        assert_eq!(
            j.get("source").get("key").get("serial").num(),
            v.get("serial_number").num()
        );
    } else {
        assert_eq!(j.get("volume"), &J::Null);
        assert_eq!(j.get("source"), &J::Null);
        assert_eq!(
            j.get("identity_unavailable").str(),
            "volume lookups are only supported on Windows"
        );
    }
    assert_eq!(j.get("scan").get("folders").num(), 5);
    assert_eq!(j.get("scan").get("entries").num(), 14);
    let t = j.get("totals");
    for (k, n) in [
        ("groups", 9),
        ("pairs", 1),
        ("lone_nefs", 4),
        ("jpeg_only", 4),
        ("ambiguous", 0),
        ("held_back", 0),
        ("unknown_files", 0),
        ("excluded", 0),
        ("metadata_errors", 0),
    ] {
        assert_eq!(t.get(k).num(), n, "{k}");
    }
    let p = primaries(&j);
    assert_eq!(p.len(), 9);
    assert_eq!(
        p[..3],
        [
            "exif\\DSC_0101.JPG",
            "exif\\DSC_0105.NEF",
            "exif\\DSC_0102.JPG"
        ]
    );
    let first = j.get("groups").at(0);
    assert_eq!(first.get("position").num(), 1);
    assert_eq!(first.get("kind").str(), "jpeg_only");
    assert_eq!(first.get("status").str(), "ok");
    assert_eq!(first.get("orientation").num(), 6);
    assert_eq!(first.get("lens").str(), "NIKKOR Z 24-200mm f/4-6.3 VR");
    let c = first.get("captured");
    assert_eq!(c.get("local").str(), "2025-02-15T03:59:50.550+11:00");
    assert_eq!(c.get("local_ms").num(), 1_739_591_990_550);
    assert_eq!(c.get("offset_minutes").num(), 660);
    assert_eq!(c.get("source").str(), "date_time_original");
    let pair = j
        .get("groups")
        .arr()
        .iter()
        .find(|g| g.get("primary").str() == "pair\\DSC_0001.JPG")
        .unwrap();
    assert_eq!(pair.get("kind").str(), "pair");
    assert_eq!(pair.get("captured").get("source").str(), "file_modified");
    let files = pair.get("files");
    assert_eq!(files.at(1).get("path").str(), "pair\\DSC_0001.NEF");
    assert_eq!(files.at(1).get("role").str(), "raw");
    assert_eq!(snapshot(&synthetic()), before, "the scan changed the tree");
}

// ---------------------------------------------------------------------------------------------
// Built trees
// ---------------------------------------------------------------------------------------------

/// A card-like tree: a pair with sidecars and `NKSC_PARAM`, a lone NEF in a subfolder, an
/// unknown file, a `_rejected` folder, a hidden file and a partial copy.
fn card(t: &TempTree) {
    let jpg = fixture("exif/DSC_0101.JPG");
    let nef = fixture("pair/DSC_0001.NEF");
    t.file("DSC_0001.JPG", &jpg);
    t.file("DSC_0001.NEF", &nef);
    t.file("DSC_0001.xmp", b"<x/>");
    t.file("DSC_0001.NEF.xmp", b"<x/>");
    t.file("NKSC_PARAM/DSC_0001.NEF.nksc", b"nksc");
    t.file("DSC_0001.MOV", b"mov");
    t.file("notes.txt", b"notes");
    t.file(".hidden.JPG", &jpg);
    t.file("DSC_0009.JPG.culla-partial", b"part");
    t.file("_rejected/DSC_0000.JPG", &jpg);
    t.file("sub/DSC_0002.NEF", &fixture("lone/DSC_0002.NEF"));
}

#[test]
fn built_tree_with_subfolders() {
    let t = TempTree::new("card");
    card(&t);
    let before = snapshot(t.path());
    let o = scan(t.path(), &["--json"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let j = parse_json(&stdout(&o));
    let tot = j.get("totals");
    assert_eq!(tot.get("groups").num(), 2);
    assert_eq!(tot.get("pairs").num(), 1);
    assert_eq!(tot.get("lone_nefs").num(), 1);
    assert_eq!(tot.get("unknown_files").num(), 2);
    assert_eq!(tot.get("excluded").num(), 3);
    let reasons: Vec<(&str, i128)> = tot
        .get("excluded_by_reason")
        .arr()
        .iter()
        .map(|r| (r.get("reason").str(), r.get("count").num()))
        .collect();
    assert_eq!(
        reasons,
        [
            ("rejected_folder", 1),
            ("hidden_or_system", 1),
            ("culla_partial", 1)
        ]
    );
    // The pair has an EXIF time (2025); the lone NEF only the file time (now): pair first.
    assert_eq!(primaries(&j), ["DSC_0001.JPG", "sub\\DSC_0002.NEF"]);
    let pair = j.get("groups").at(0);
    let files: Vec<(&str, &str, Option<&str>)> = pair
        .get("files")
        .arr()
        .iter()
        .map(|f| {
            let follows = match f {
                J::Obj(fields) => fields
                    .iter()
                    .find(|(k, _)| k == "follows")
                    .map(|(_, v)| v.str()),
                _ => None,
            };
            (f.get("path").str(), f.get("role").str(), follows)
        })
        .collect();
    assert_eq!(
        files,
        [
            ("DSC_0001.JPG", "primary", None),
            ("DSC_0001.NEF", "raw", None),
            ("DSC_0001.NEF.xmp", "sidecar", Some("raw")),
            ("DSC_0001.xmp", "sidecar", Some("raw")),
            ("NKSC_PARAM\\DSC_0001.NEF.nksc", "sidecar", Some("raw")),
        ]
    );
    let unknown: Vec<(&str, &str)> = j
        .get("unknown")
        .arr()
        .iter()
        .map(|u| (u.get("path").str(), u.get("kind").str()))
        .collect();
    assert_eq!(
        unknown,
        [
            ("DSC_0001.MOV", "same_basename_unknown"),
            ("notes.txt", "unknown_ext")
        ]
    );
    let excluded = j.get("excluded");
    let rejected = excluded
        .arr()
        .iter()
        .find(|x| x.get("reason").str() == "rejected_folder")
        .unwrap();
    assert_eq!(rejected.get("path").str(), "_rejected");
    assert_eq!(rejected.get("folder"), &J::Bool(true));
    assert_eq!(snapshot(t.path()), before, "the scan changed the tree");
}

#[test]
fn built_tree_without_subfolders_as_text() {
    let t = TempTree::new("nosub");
    card(&t);
    let before = snapshot(t.path());
    let o = scan(t.path(), &["--no-subfolders"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = stdout(&o);
    for line in [
        "subfolders:  not included",
        "  groups          1",
        "  pairs           1",
        "  lone NEFs       0",
        "  unknown files   2",
        "  excluded        4",
        "    1 inside a _rejected folder",
        "    1 in a subfolder, and subfolders are not included",
        "    1 hidden or system",
        "    1 a partial copy left by Culla",
        "1. DSC_0001.JPG",
        "   files        DSC_0001.JPG, DSC_0001.NEF, DSC_0001.NEF.xmp, DSC_0001.xmp, \
         NKSC_PARAM\\DSC_0001.NEF.nksc",
        "unknown files (listed, never touched)",
        "  DSC_0001.MOV  (same_basename_unknown)",
        "  notes.txt  (unknown_ext)",
        "excluded",
        "  sub\\  (in a subfolder, and subfolders are not included)",
        "  _rejected\\  (inside a _rejected folder)",
    ] {
        assert!(out.lines().any(|l| l == line), "missing {line:?} in\n{out}");
    }
    assert!(!out.contains("DSC_0002"), "{out}");
    assert_eq!(snapshot(t.path()), before, "the scan changed the tree");
}

#[test]
fn an_empty_folder_has_no_photos() {
    let t = TempTree::new("empty");
    let o = scan(t.path(), &[]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = stdout(&o);
    assert!(out.contains("  groups          0\n"), "{out}");
    assert!(out.contains("\nno photos found\n"), "{out}");
    let j = parse_json(&stdout(&scan(t.path(), &["--json"])));
    assert_eq!(j.get("groups"), &J::Arr(vec![]));
    assert_eq!(
        snapshot(t.path()),
        Vec::new(),
        "the scan wrote into the folder"
    );
}

#[test]
fn ambiguous_groups_are_counted_and_marked() {
    let t = TempTree::new("ambiguous");
    let jpg = fixture("exif/DSC_0101.JPG");
    t.file("DSC_0001.JPG", &jpg);
    t.file("DSC_0001.JPEG", &jpg);
    let o = scan(t.path(), &[]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = stdout(&o);
    assert!(out.contains("  ambiguous       1\n"), "{out}");
    // Duplicates sort by folded name: `DSC_0001.JPEG` before `DSC_0001.JPG`.
    assert!(out.contains("1. DSC_0001.JPEG  (ambiguous)\n"), "{out}");
    assert!(
        out.contains("   files        DSC_0001.JPEG, DSC_0001.JPG\n"),
        "{out}"
    );
    let j = parse_json(&stdout(&scan(t.path(), &["--json"])));
    let g = j.get("groups").at(0);
    assert_eq!(g.get("status").str(), "ambiguous");
    assert_eq!(g.get("files").at(1).get("role").str(), "duplicate");
}

#[cfg(unix)]
#[test]
fn untrusted_names_are_escaped_in_text_and_json() {
    let t = TempTree::new("names");
    let jpg = fixture("exif/DSC_0101.JPG");
    // A valid name with a bidi override (grouped) and one with an escape sequence (refused).
    t.file("DSC\u{202E}1.JPG", &jpg);
    t.file("DSC\u{1b}[2J.JPG", &jpg);
    let o = scan(t.path(), &[]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = stdout(&o);
    assert!(
        !out.contains('\u{1b}') && !out.contains('\u{202E}'),
        "{out:?}"
    );
    assert!(out.contains("1. DSC\\u{202e}1.JPG\n"), "{out}");
    assert!(
        out.contains("  DSC\\u{1b}[2J.JPG  (invalid name)\n"),
        "{out}"
    );

    let o = scan(t.path(), &["--json"]);
    let raw = stdout(&o);
    assert!(!raw.contains('\u{1b}'), "{raw:?}");
    assert!(raw.contains("DSC\\u001b[2J.JPG"), "{raw}");
    let j = parse_json(&raw);
    assert_eq!(primaries(&j), ["DSC\u{202E}1.JPG"]);
    assert_eq!(
        j.get("excluded").at(0).get("path").str(),
        "DSC\u{1b}[2J.JPG"
    );
    assert_eq!(j.get("excluded").at(0).get("reason").str(), "invalid_name");
}

// ---------------------------------------------------------------------------------------------
// Refused and unreadable folders
// ---------------------------------------------------------------------------------------------

#[test]
fn a_missing_folder_exits_3() {
    let t = TempTree::new("missing");
    let o = scan(&t.path().join("nope"), &[]);
    assert_eq!(code(&o), 3);
    assert!(o.stdout.is_empty());
    let err = stderr(&o);
    assert!(
        err.starts_with("culla: ") && err.contains("nope: cannot resolve the folder"),
        "{err}"
    );
}

#[test]
fn a_file_is_refused_with_exit_2() {
    let o = scan(&synthetic().join("pair/DSC_0001.JPG"), &["--json"]);
    assert_eq!(code(&o), 2);
    assert!(o.stdout.is_empty());
    assert!(
        stderr(&o)
            .trim_end()
            .ends_with("DSC_0001.JPG: not a folder"),
        "{}",
        stderr(&o)
    );
}

#[cfg(unix)]
#[test]
fn a_linked_folder_is_refused_with_exit_2() {
    let t = TempTree::new("link");
    let target = t.dir("real");
    t.file("real/DSC_0001.JPG", &fixture("exif/DSC_0101.JPG"));
    let link = t.path().join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let before = snapshot(t.path());
    let o = scan(&link, &[]);
    assert_eq!(code(&o), 2);
    assert!(o.stdout.is_empty());
    assert!(
        stderr(&o).contains("the folder is a link, junction or mount point"),
        "{}",
        stderr(&o)
    );
    assert_eq!(snapshot(t.path()), before);
}

#[cfg(unix)]
#[test]
fn a_link_inside_the_folder_is_not_followed() {
    let t = TempTree::new("innerlink");
    let outside = TempTree::new("outside");
    outside.file("DSC_0007.JPG", &fixture("exif/DSC_0101.JPG"));
    std::os::unix::fs::symlink(outside.path(), t.path().join("elsewhere")).unwrap();
    let j = parse_json(&stdout(&scan(t.path(), &["--json"])));
    assert_eq!(j.get("totals").get("groups").num(), 0);
    let x = j.get("excluded").at(0);
    assert_eq!(x.get("path").str(), "elsewhere");
    assert_eq!(x.get("reason").str(), "reparse_point");
}

// ---------------------------------------------------------------------------------------------
// The author's samples (ignored by default)
// ---------------------------------------------------------------------------------------------

#[test]
#[ignore = "needs the author's samples: set CULLA_REAL_SAMPLES"]
fn real_samples_give_nine_groups_in_capture_time_order() {
    let Ok(root) = std::env::var("CULLA_REAL_SAMPLES") else {
        println!("skipped: CULLA_REAL_SAMPLES is not set");
        return;
    };
    let root = PathBuf::from(root);
    let before = snapshot(&root);
    let o = scan(&root, &["--json"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let j = parse_json(&stdout(&o));
    let t = j.get("totals");
    assert_eq!(t.get("groups").num(), 9);
    assert_eq!(t.get("pairs").num(), 8);
    assert_eq!(t.get("lone_nefs").num(), 1);
    assert_eq!(t.get("ambiguous").num(), 0);
    assert_eq!(t.get("metadata_errors").num(), 0);
    let groups = j.get("groups").arr();
    let mut last = i128::MIN;
    for g in groups {
        let c = g.get("captured");
        assert_eq!(c.get("source").str(), "date_time_original", "{g:?}");
        let ms = c.get("local_ms").num();
        assert!(
            ms >= last,
            "not in capture-time order at {}",
            g.get("primary").str()
        );
        last = ms;
        assert!(matches!(g.get("orientation").num(), 1 | 6 | 8));
        assert!(!g.get("lens").str().is_empty());
    }
    let text = stdout(&scan(&root, &[]));
    assert!(
        !text.to_ascii_lowercase().contains("gps"),
        "GPS must not be shown"
    );
    println!("{text}");
    assert_eq!(snapshot(&root), before, "the scan changed the samples");
}
