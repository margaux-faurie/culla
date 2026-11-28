//! `culla plan` run as a process (ADR 0020): a strictly read-only dry run. Every test that
//! runs it checks that the source tree, the destination and the decision log are exactly as
//! they were (bytes, size, mtime, read-only flag, and no entry added or removed), and that
//! neither `_rejected` nor the destination is created.
//!
//! Temporary trees live under `std::env::temp_dir()` with a unique name, are built with
//! `create_new` only, and are removed by [`TempTree`] on drop: the only removal in this file,
//! inside its own temp folder (the lint may be switched off in `crates/*/tests/`,
//! scripts/ci/check_lint_allows.py).
//!
//! The author's samples are read only by the ignored test at the end, when
//! `CULLA_REAL_SAMPLES` names their folder:
//! `CULLA_REAL_SAMPLES=/path/to/samples cargo test -p culla-cli --test plan -- --ignored`

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
            "culla-plan-{test}-{}-{nanos}-{}",
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

    #[cfg(unix)]
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

    fn bool(&self) -> bool {
        match self {
            J::Bool(b) => *b,
            _ => panic!("not a bool: {self:?}"),
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

// ---------------------------------------------------------------------------------------------
// Plan helpers
// ---------------------------------------------------------------------------------------------

fn plan(folder: &Path, log: &Path, extra: &[&std::ffi::OsStr]) -> Output {
    let mut args: Vec<&std::ffi::OsStr> = vec!["plan".as_ref(), "--decisions".as_ref()];
    args.push(log.as_os_str());
    args.extend(extra.iter().copied());
    args.push("--".as_ref());
    args.push(folder.as_os_str());
    culla(&args)
}

fn os(s: &str) -> &std::ffi::OsStr {
    std::ffi::OsStr::new(s)
}

/// Group ids as `culla plan` reads them: the positions `culla scan --json` prints, by primary.
fn positions(folder: &Path, extra: &[&str]) -> Vec<(String, i128)> {
    let mut args = vec!["--json"];
    args.extend(extra);
    let o = scan(folder, &args);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    parse_json(&stdout(&o))
        .get("groups")
        .arr()
        .iter()
        .map(|g| (g.get("primary").str().to_owned(), g.get("position").num()))
        .collect()
}

fn id_of(positions: &[(String, i128)], primary: &str) -> i128 {
    positions
        .iter()
        .find(|(p, _)| p == primary)
        .unwrap_or_else(|| panic!("no group {primary} in {positions:?}"))
        .1
}

fn decided(seq: i128, group: i128, verdict: &str) -> String {
    format!(
        "{{\"seq\":{seq},\"ts\":{seq},\"cause\":\"user\",\"type\":\"decided\",\"group_id\":{group},\"verdict\":\"{verdict}\"}}\n"
    )
}

fn undo(seq: i128, target: i128) -> String {
    format!(
        "{{\"seq\":{seq},\"ts\":{seq},\"cause\":\"user\",\"type\":\"undo\",\"target_seq\":{target}}}\n"
    )
}

/// Runs `f` and checks that every tree in `trees` is exactly as before.
fn unchanged<T>(trees: &[&Path], f: impl FnOnce() -> T) -> T {
    let before: Vec<_> = trees.iter().map(|t| snapshot(t)).collect();
    let out = f();
    for (t, b) in trees.iter().zip(before) {
        assert_eq!(snapshot(t), b, "{} changed", t.display());
    }
    out
}

fn ops(j: &J) -> Vec<(String, String, String, String, String)> {
    j.get("operations")
        .arr()
        .iter()
        .map(|o| {
            (
                o.get("action").str().to_owned(),
                o.get("kind").str().to_owned(),
                o.get("src").str().to_owned(),
                o.get("root").str().to_owned(),
                o.get("dst").str().to_owned(),
            )
        })
        .collect()
}

fn op(
    action: &str,
    kind: &str,
    src: &str,
    root: &str,
    dst: &str,
) -> (String, String, String, String, String) {
    (
        action.to_owned(),
        kind.to_owned(),
        src.to_owned(),
        root.to_owned(),
        dst.to_owned(),
    )
}

/// A source with a pair and its bare sidecar, a JPEG-only group and a lone NEF in `100/`, and
/// a `_rejected/100/DSC_0101.JPG` already there.
fn source_tree(test: &str) -> TempTree {
    let t = TempTree::new(test);
    t.file("100/DSC_0001.JPG", &fixture("pair/DSC_0001.JPG"));
    t.file("100/DSC_0001.NEF", &fixture("pair/DSC_0001.NEF"));
    t.file("100/DSC_0001.xmp", b"<x:xmpmeta/>");
    t.file("100/DSC_0101.JPG", &fixture("exif/DSC_0101.JPG"));
    t.file("100/DSC_0002.NEF", &fixture("lone/DSC_0002.NEF"));
    t.file("_rejected/100/DSC_0101.JPG", b"an earlier reject");
    t
}

// ---------------------------------------------------------------------------------------------
// Help and usage
// ---------------------------------------------------------------------------------------------

#[test]
fn help_describes_plan() {
    for args in [&["help", "plan"][..], &["plan", "--help"]] {
        let o = run(args);
        assert_eq!(code(&o), 0);
        let out = stdout(&o);
        assert!(
            out.starts_with("Usage: culla plan <folder> --decisions <log-file>"),
            "{out}"
        );
        assert!(out.contains("Reads only"), "{out}");
        assert!(out.contains("\"schema\": \"unstable\""), "{out}");
    }
    let out = stdout(&run(&["help"]));
    assert!(
        out.contains("culla plan <folder> --decisions <log-file>"),
        "{out}"
    );
    assert!(out.contains("4  the decision log is not valid"), "{out}");
}

#[test]
fn usage_errors_exit_1() {
    for (args, msg) in [
        (&["plan", "d"][..], "culla: no decision log given"),
        (
            &["plan", "d", "--decisions", "l", "--keep", "move"],
            "culla: --keep needs --dest",
        ),
        (
            &["plan", "d", "--decisions", "l", "--dest", "x"],
            "culla: --dest needs --keep",
        ),
    ] {
        let o = run(args);
        assert_eq!(code(&o), 1, "{args:?}");
        assert!(o.stdout.is_empty());
        assert!(stderr(&o).starts_with(msg), "{args:?}: {}", stderr(&o));
    }
}

// ---------------------------------------------------------------------------------------------
// The plan, read-only
// ---------------------------------------------------------------------------------------------

#[test]
fn keep_by_copy_and_rejects_with_suffixes_change_nothing() {
    let src = source_tree("copy");
    let dest = TempTree::new("copy-dest");
    dest.file("100/DSC_0001.JPG", b"already kept");
    let logs = TempTree::new("copy-log");
    let pos = positions(src.path(), &[]);
    let log = logs.file(
        "decisions.jsonl",
        format!(
            "{}{}{}{}",
            decided(1, id_of(&pos, "100\\DSC_0001.JPG"), "keep"),
            decided(2, id_of(&pos, "100\\DSC_0101.JPG"), "reject"),
            decided(3, id_of(&pos, "100\\DSC_0002.NEF"), "reject"),
            undo(4, 3),
        )
        .as_bytes(),
    );
    let o = unchanged(&[src.path(), dest.path(), logs.path()], || {
        plan(
            src.path(),
            &log,
            &[
                os("--keep"),
                os("copy"),
                os("--dest"),
                dest.path().as_os_str(),
                os("--json"),
            ],
        )
    });
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let j = parse_json(&stdout(&o));
    assert_eq!(j.get("schema").str(), "unstable");
    assert!(j.get("dry_run").bool());
    assert_eq!(j.get("decisions").get("events").num(), 4);
    assert_eq!(
        ops(&j),
        [
            op(
                "reject",
                "rename",
                "100\\DSC_0101.JPG",
                "rejected",
                "100\\DSC_0101_1.JPG"
            ),
            op(
                "keep",
                "copy",
                "100\\DSC_0001.JPG",
                "dest",
                "100\\DSC_0001_1.JPG"
            ),
            op(
                "keep",
                "copy",
                "100\\DSC_0001.NEF",
                "dest",
                "100\\DSC_0001_1.NEF"
            ),
            op(
                "keep",
                "copy",
                "100\\DSC_0001.xmp",
                "dest",
                "100\\DSC_0001_1.xmp"
            ),
        ]
    );
    let suffixes: Vec<i128> = j
        .get("planned_groups")
        .arr()
        .iter()
        .map(|g| g.get("suffix").num())
        .collect();
    assert_eq!(suffixes, [1, 1]);
    let t = j.get("totals");
    assert_eq!(t.get("kept").num(), 1);
    assert_eq!(t.get("rejected").num(), 1);
    assert_eq!(t.get("operations").num(), 4);
    assert_eq!(t.get("copies").num(), 3);
    assert_eq!(t.get("renames").num(), 1);
    let unchecked: Vec<&str> = j
        .get("preflight")
        .get("unchecked")
        .arr()
        .iter()
        .map(|u| u.get("id").str())
        .collect();
    assert!(unchecked.contains(&"free_space"), "{unchecked:?}");
    assert!(unchecked.contains(&"locks"), "{unchecked:?}");

    // The same as text.
    let o = unchanged(&[src.path(), dest.path(), logs.path()], || {
        plan(
            src.path(),
            &log,
            &[
                os("--keep"),
                os("copy"),
                os("--dest"),
                dest.path().as_os_str(),
            ],
        )
    });
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = stdout(&o);
    assert!(
        out.contains("This is a dry run: nothing was moved, copied or written."),
        "{out}"
    );
    assert!(
        out.contains("reject rename 100\\DSC_0101.JPG -> _rejected\\100\\DSC_0101_1.JPG"),
        "{out}"
    );
    assert!(
        out.contains("keep   copy   100\\DSC_0001.NEF -> <dest>\\100\\DSC_0001_1.NEF"),
        "{out}"
    );
    assert!(
        out.contains(
            "  planned:     2 groups, 4 operations (1 rename, 3 copy, 0 cross-volume move)"
        ),
        "{out}"
    );
    assert!(out.contains("  unknown, preflight fails closed:"), "{out}");
    assert!(out.contains("  not checked:"), "{out}");
    assert!(
        out.contains("locks held by other programs are not checked: P12 never warns"),
        "{out}"
    );
    let closed: Vec<(&str, bool)> = j
        .get("preflight")
        .get("unchecked")
        .arr()
        .iter()
        .map(|u| (u.get("id").str(), u.get("fails_closed").bool()))
        .collect();
    assert!(closed.contains(&("free_space", true)), "{closed:?}");
    assert!(closed.contains(&("locks", false)), "{closed:?}");
}

#[test]
fn raw_follows_no_leaves_the_nef_and_its_bare_sidecar() {
    let src = source_tree("raw");
    let dest = TempTree::new("raw-dest");
    let logs = TempTree::new("raw-log");
    let pos = positions(src.path(), &[]);
    let log = logs.file(
        "d.jsonl",
        decided(1, id_of(&pos, "100\\DSC_0001.JPG"), "keep").as_bytes(),
    );
    let o = unchanged(&[src.path(), dest.path(), logs.path()], || {
        plan(
            src.path(),
            &log,
            &[
                os("--keep"),
                os("move"),
                os("--dest"),
                dest.path().as_os_str(),
                os("--raw-follows"),
                os("no"),
                os("--json"),
            ],
        )
    });
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let j = parse_json(&stdout(&o));
    let got = ops(&j);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0].2, "100\\DSC_0001.JPG");
    assert!(!j.get("settings").get("raw_follows").bool());
}

#[test]
fn missing_targets_are_not_created() {
    // No `_rejected` and no destination yet: the dry run creates neither.
    let src = TempTree::new("fresh");
    src.file("DSC_0101.JPG", &fixture("exif/DSC_0101.JPG"));
    src.file("DSC_0102.JPG", &fixture("exif/DSC_0102.JPG"));
    let parent = TempTree::new("fresh-parent");
    let dest = parent.path().join("kept");
    let logs = TempTree::new("fresh-log");
    let pos = positions(src.path(), &[]);
    let log = logs.file(
        "d.jsonl",
        format!(
            "{}{}",
            decided(1, id_of(&pos, "DSC_0101.JPG"), "reject"),
            decided(2, id_of(&pos, "DSC_0102.JPG"), "keep")
        )
        .as_bytes(),
    );
    for extra in [&[][..], &[os("--json")]] {
        let mut args = vec![os("--keep"), os("copy"), os("--dest"), dest.as_os_str()];
        args.extend(extra);
        let o = unchanged(&[src.path(), parent.path(), logs.path()], || {
            plan(src.path(), &log, &args)
        });
        assert_eq!(code(&o), 0, "{}", stderr(&o));
    }
    assert!(!src.path().join("_rejected").exists());
    assert!(!dest.exists());
}

#[test]
fn a_destination_inside_the_source_is_numbered_like_scan_and_not_planned() {
    // The destination's photo sorts first in capture-time order, so the log's ids only match
    // `culla scan` if the destination is scanned and numbered too.
    let src = TempTree::new("inside");
    src.file("kept/DSC_0101.JPG", &fixture("exif/DSC_0101.JPG"));
    src.file("DSC_0102.JPG", &fixture("exif/DSC_0102.JPG"));
    let pos = positions(src.path(), &[]);
    let in_dest = id_of(&pos, "kept\\DSC_0101.JPG");
    let outside = id_of(&pos, "DSC_0102.JPG");
    assert!(in_dest < outside, "{pos:?}");
    let logs = TempTree::new("inside-log");
    let log = logs.file(
        "d.jsonl",
        format!(
            "{}{}",
            decided(1, outside, "keep"),
            decided(2, in_dest, "reject")
        )
        .as_bytes(),
    );
    let dest = src.path().join("kept");
    let args = [os("--keep"), os("copy"), os("--dest"), dest.as_os_str()];
    let mut json_args = args.to_vec();
    json_args.push(os("--json"));
    let o = unchanged(&[src.path(), logs.path()], || {
        plan(src.path(), &log, &json_args)
    });
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let j = parse_json(&stdout(&o));
    assert!(j.get("settings").get("dest_inside_source").bool());
    assert_eq!(j.get("totals").get("groups").num(), 1);
    assert_eq!(j.get("totals").get("kept").num(), 1);
    assert_eq!(j.get("totals").get("rejected").num(), 0);
    assert_eq!(
        ops(&j),
        [op("keep", "copy", "DSC_0102.JPG", "dest", "DSC_0102.JPG")]
    );
    let ignored: Vec<i128> = j
        .get("decisions")
        .get("destination_groups")
        .arr()
        .iter()
        .map(J::num)
        .collect();
    assert_eq!(ignored, [in_dest]);
    assert!(j.get("decisions").get("unknown_groups").arr().is_empty());

    let o = unchanged(&[src.path(), logs.path()], || plan(src.path(), &log, &args));
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = stdout(&o);
    assert!(
        out.contains(&format!(
            "decisions for groups inside the destination (ignored): {in_dest}"
        )),
        "{out}"
    );
    assert!(!out.contains("_rejected"), "{out}");
}

#[test]
fn unknown_group_ids_are_listed_and_ignored() {
    let src = TempTree::new("unknown");
    src.file("DSC_0101.JPG", &fixture("exif/DSC_0101.JPG"));
    let logs = TempTree::new("unknown-log");
    let log = logs.file("d.jsonl", decided(1, 42, "reject").as_bytes());
    let o = unchanged(&[src.path(), logs.path()], || {
        plan(src.path(), &log, &[os("--json")])
    });
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let j = parse_json(&stdout(&o));
    let unknown: Vec<i128> = j
        .get("decisions")
        .get("unknown_groups")
        .arr()
        .iter()
        .map(J::num)
        .collect();
    assert_eq!(unknown, [42]);
    assert!(j.get("operations").arr().is_empty());
    let out = stdout(&plan(src.path(), &log, &[]));
    assert!(
        out.contains("decisions for groups the scan does not have (ignored): 42"),
        "{out}"
    );
}

// ---------------------------------------------------------------------------------------------
// Failures
// ---------------------------------------------------------------------------------------------

#[test]
fn bad_logs_and_missing_destinations_have_their_exit_codes() {
    let src = TempTree::new("fail");
    src.file("DSC_0101.JPG", &fixture("exif/DSC_0101.JPG"));
    let logs = TempTree::new("fail-log");
    let check = |log: &Path, extra: &[&std::ffi::OsStr], want: i32, msg: &str| {
        let o = unchanged(&[src.path(), logs.path()], || plan(src.path(), log, extra));
        assert_eq!(code(&o), want, "{msg}: {}", stderr(&o));
        assert!(o.stdout.is_empty(), "{msg}");
        assert!(stderr(&o).contains(msg), "{msg}: {}", stderr(&o));
    };
    check(
        &logs.path().join("nope.jsonl"),
        &[],
        3,
        "cannot read the file",
    );
    check(logs.path(), &[], 3, "not a regular file");
    let bad = logs.file("bad.jsonl", b"{\"seq\":1}\n");
    check(&bad, &[], 4, "line 1:");
    let no_newline = logs.file("nonl.jsonl", decided(1, 1, "keep").trim_end().as_bytes());
    check(&no_newline, &[], 4, "line 1:");
    let corrupt = logs.file(
        "corrupt.jsonl",
        format!("{}{}", decided(1, 1, "keep"), decided(2, 1, "reject")).as_bytes(),
    );
    check(&corrupt, &[], 4, "the log is corrupt at");
    let not_utf8 = logs.file("latin1.jsonl", b"\xff\n");
    check(&not_utf8, &[], 4, "not valid UTF-8");
    let keep = logs.file("keep.jsonl", decided(1, 1, "keep").as_bytes());
    check(&keep, &[], 5, "no destination is given");
    // A missing source folder.
    let o = plan(&src.path().join("nope"), &keep, &[]);
    assert_eq!(code(&o), 3, "{}", stderr(&o));
}

#[cfg(unix)]
#[test]
fn a_destination_that_is_a_link_is_refused() {
    let src = TempTree::new("link");
    src.file("DSC_0101.JPG", &fixture("exif/DSC_0101.JPG"));
    let elsewhere = TempTree::new("link-target");
    let logs = TempTree::new("link-log");
    let link = logs.path().join("dest");
    std::os::unix::fs::symlink(elsewhere.path(), &link).unwrap();
    let log = logs.file("d.jsonl", decided(1, 1, "keep").as_bytes());
    let o = unchanged(&[src.path(), elsewhere.path(), logs.path()], || {
        plan(
            src.path(),
            &log,
            &[os("--keep"), os("copy"), os("--dest"), link.as_os_str()],
        )
    });
    assert_eq!(code(&o), 2, "{}", stderr(&o));
    assert!(stderr(&o).contains("destination:"), "{}", stderr(&o));
}

#[cfg(unix)]
#[test]
fn a_link_in_rejected_holds_the_group_and_is_not_entered() {
    let src = TempTree::new("rejlink");
    src.file("100/DSC_0101.JPG", &fixture("exif/DSC_0101.JPG"));
    let elsewhere = TempTree::new("rejlink-target");
    elsewhere.file("DSC_0101.JPG", b"not to be listed");
    let _ = src.dir("_rejected");
    std::os::unix::fs::symlink(elsewhere.path(), src.path().join("_rejected/100")).unwrap();
    let logs = TempTree::new("rejlink-log");
    let log = logs.file("d.jsonl", decided(1, 1, "reject").as_bytes());
    let o = unchanged(&[src.path(), elsewhere.path(), logs.path()], || {
        plan(src.path(), &log, &[os("--json")])
    });
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let j = parse_json(&stdout(&o));
    // Not entered: the name behind the link does not give a suffix.
    assert_eq!(
        ops(&j),
        [op(
            "reject",
            "rename",
            "100\\DSC_0101.JPG",
            "rejected",
            "100\\DSC_0101.JPG"
        )]
    );
    let held = j.get("preflight").get("held").arr();
    assert_eq!(held.len(), 1, "{held:?}");
    assert_eq!(held[0].get("reason").str(), "link");
    assert_eq!(j.get("totals").get("runnable").num(), 0);
}

#[cfg(unix)]
#[test]
fn a_log_that_is_a_link_is_refused() {
    let src = TempTree::new("loglink");
    src.file("DSC_0101.JPG", &fixture("exif/DSC_0101.JPG"));
    let logs = TempTree::new("loglink-log");
    let real = logs.file("real.jsonl", decided(1, 1, "reject").as_bytes());
    let link = logs.path().join("link.jsonl");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let o = unchanged(&[src.path(), logs.path()], || plan(src.path(), &link, &[]));
    assert_eq!(code(&o), 3, "{}", stderr(&o));
    assert!(stderr(&o).contains("not a regular file"), "{}", stderr(&o));
}

#[cfg(unix)]
#[test]
fn untrusted_names_are_escaped_in_text_and_json() {
    let src = TempTree::new("names");
    let _ = src.dir("a\u{202E}b");
    src.file("a\u{202E}b/DSC_0101.JPG", &fixture("exif/DSC_0101.JPG"));
    let logs = TempTree::new("names-log");
    let log = logs.file("d.jsonl", decided(1, 1, "reject").as_bytes());
    let o = unchanged(&[src.path(), logs.path()], || plan(src.path(), &log, &[]));
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = stdout(&o);
    assert!(!out.contains('\u{202E}'), "{out:?}");
    assert!(
        out.contains("a\\u{202e}b\\DSC_0101.JPG -> _rejected\\a\\u{202e}b\\DSC_0101.JPG"),
        "{out}"
    );
    let raw = stdout(&plan(src.path(), &log, &[os("--json")]));
    assert!(!raw.contains('\u{202E}'), "{raw:?}");
    assert!(raw.contains("a\\u202eb\\\\DSC_0101.JPG"), "{raw}");
    let j = parse_json(&raw);
    assert_eq!(ops(&j)[0].2, "a\u{202E}b\\DSC_0101.JPG");
}

// ---------------------------------------------------------------------------------------------
// The author's samples
// ---------------------------------------------------------------------------------------------

#[test]
#[ignore = "needs the author's samples: set CULLA_REAL_SAMPLES"]
fn real_samples_plan_changes_nothing() {
    let Ok(root) = std::env::var("CULLA_REAL_SAMPLES") else {
        println!("skipped: CULLA_REAL_SAMPLES is not set");
        return;
    };
    let root = PathBuf::from(root);
    let pos = positions(&root, &[]);
    assert!(pos.len() >= 3, "{pos:?}");
    let logs = TempTree::new("real-log");
    let log = logs.file(
        "d.jsonl",
        format!(
            "{}{}{}",
            decided(1, pos[0].1, "keep"),
            decided(2, pos[1].1, "reject"),
            decided(3, pos[2].1, "keep")
        )
        .as_bytes(),
    );
    let dest = TempTree::new("real-dest");
    for mode in ["copy", "move"] {
        for extra in [&[][..], &[os("--json")]] {
            let mut args = vec![
                os("--keep"),
                os(mode),
                os("--dest"),
                dest.path().as_os_str(),
            ];
            args.extend(extra);
            let o = unchanged(&[&root, dest.path(), logs.path()], || {
                plan(&root, &log, &args)
            });
            assert_eq!(code(&o), 0, "{}", stderr(&o));
        }
    }
    let j = parse_json(&stdout(&plan(
        &root,
        &log,
        &[
            os("--keep"),
            os("copy"),
            os("--dest"),
            dest.path().as_os_str(),
            os("--json"),
        ],
    )));
    assert_eq!(j.get("totals").get("kept").num(), 2);
    assert_eq!(j.get("totals").get("rejected").num(), 1);
    assert!(!j.get("operations").arr().is_empty());
    assert!(!root.join("_rejected").exists());
}
