//! Apply log rendering: the whole text for a mixed plan (the format, pinned byte for byte), one
//! line per kind and outcome, escaping of untrusted names and display strings, determinism, and
//! what the log never holds.

use super::*;
use crate::decisions::Verdict;
use crate::journal::{Detail, End, Entry, JournalError, Kind, OpId, RunError, StoreError};

fn op(id: OpId, group: i64, kind: Kind, root: Root, src: &[&str], dst: &[&str]) -> Op {
    Op {
        id,
        ord: 0,
        group,
        member: id * 10,
        action: if root == Root::Rejected {
            Verdict::Reject
        } else {
            Verdict::Keep
        },
        kind,
        src: src.iter().map(|s| (*s).to_owned()).collect(),
        root,
        dst: dst.iter().map(|s| (*s).to_owned()).collect(),
        expected_size: 1000,
        src_mtime_ns: Some(1_700_000_000_000_000_000),
        finish_hash: (kind == Kind::Finish).then_some([0xF0; 32]),
    }
}

fn journal(mut ops: Vec<Op>) -> Journal {
    for (i, o) in ops.iter_mut().enumerate() {
        o.ord = u32::try_from(i).unwrap();
    }
    Journal::new(ops).unwrap()
}

/// Moves operation `id` through `steps` (target state and detail), each a legal entry.
fn walk(j: &mut Journal, id: OpId, steps: &[(OpState, Detail)]) {
    for (to, detail) in steps {
        let from = j.get(id).unwrap().1.state();
        j.apply(&Entry {
            op: id,
            from,
            to: *to,
            detail: detail.clone(),
        })
        .unwrap();
    }
}

fn copied(b: u8) -> Detail {
    Detail::Copied {
        hash: [b; 32],
        size: 1000,
    }
}

fn attempt(n: u32) -> Detail {
    Detail::Attempt { attempt: n }
}

const N: Detail = Detail::None;

fn rename_done() -> Vec<(OpState, Detail)> {
    vec![
        (OpState::Renaming, N),
        (OpState::Committed, N),
        (OpState::Done, N),
    ]
}

fn copy_committed(b: u8) -> Vec<(OpState, Detail)> {
    vec![
        (OpState::Copying, attempt(1)),
        (OpState::Verifying, copied(b)),
        (OpState::Committed, N),
    ]
}

fn header<'a>() -> Header<'a> {
    Header {
        version: "0.1.0",
        plan: 12,
        purpose: Purpose::Apply,
        run: RunKind::First,
        started: "2026-11-28 09:14:03 +01:00",
        source: "E:\\DCIM\\100NIKON",
        destination: Some("D:\\Photos\\2026"),
    }
}

fn footer<'a>(end: RunEnd) -> Footer<'a> {
    Footer {
        end,
        ended: "2026-11-28 09:15:41 +01:00",
    }
}

/// A plan with every outcome a run can leave: renames done and held at `committed`, a verified
/// move, a copy, a move whose group failed (source kept), a verify mismatch after a retry, an
/// I/O error, an alarm, and an operation not started.
fn mixed() -> Journal {
    let mut j = journal(vec![
        op(
            1,
            1,
            Kind::Rename,
            Root::Rejected,
            &["DSC_0001.JPG"],
            &["DSC_0001.JPG"],
        ),
        op(
            2,
            1,
            Kind::Rename,
            Root::Rejected,
            &["DSC_0001.NEF"],
            &["DSC_0001.NEF"],
        ),
        op(
            3,
            2,
            Kind::XMove,
            Root::Dest,
            &["DSC_0002.JPG"],
            &["DSC_0002.JPG"],
        ),
        op(
            4,
            3,
            Kind::XMove,
            Root::Dest,
            &["DSC_0003.JPG"],
            &["DSC_0003.JPG"],
        ),
        op(
            5,
            3,
            Kind::XMove,
            Root::Dest,
            &["DSC_0003.NEF"],
            &["DSC_0003.NEF"],
        ),
        op(
            6,
            4,
            Kind::Copy,
            Root::Dest,
            &["sub", "DSC_0004.JPG"],
            &["sub", "DSC_0004_1.JPG"],
        ),
        op(
            7,
            5,
            Kind::Rename,
            Root::Rejected,
            &["DSC_0005.JPG"],
            &["DSC_0005.JPG"],
        ),
        op(
            8,
            6,
            Kind::Rename,
            Root::Rejected,
            &["DSC_0006.JPG"],
            &["DSC_0006.JPG"],
        ),
        op(
            9,
            7,
            Kind::Rename,
            Root::Rejected,
            &["DSC_0007.JPG"],
            &["DSC_0007.JPG"],
        ),
    ]);
    walk(&mut j, 1, &rename_done());
    walk(
        &mut j,
        2,
        &[(OpState::Renaming, N), (OpState::Committed, N)],
    );
    let mut moved = copy_committed(0xAB);
    moved.extend([(OpState::SourceRemoved, N), (OpState::Done, N)]);
    walk(&mut j, 3, &moved);
    let mut kept = copy_committed(0x01);
    kept.push((OpState::CopiedSourceKept, N));
    walk(&mut j, 4, &kept);
    walk(
        &mut j,
        5,
        &[
            (OpState::Copying, attempt(1)),
            (OpState::Pending, N),
            (OpState::Copying, attempt(2)),
            (OpState::Verifying, copied(0x02)),
            (OpState::Failed, Detail::Failed(FailReason::VerifyMismatch)),
        ],
    );
    let mut copy = copy_committed(0xCD);
    copy.push((OpState::Done, N));
    walk(&mut j, 6, &copy);
    walk(
        &mut j,
        7,
        &[
            (OpState::Renaming, N),
            (OpState::Failed, Detail::Failed(FailReason::Io(1392))),
        ],
    );
    walk(
        &mut j,
        8,
        &[
            (OpState::Renaming, N),
            (OpState::Failed, Detail::Failed(FailReason::Lost)),
        ],
    );
    j
}

#[test]
fn whole_log_for_a_mixed_plan() {
    let ab = "ab".repeat(32);
    let one = "01".repeat(32);
    let two = "02".repeat(32);
    let cd = "cd".repeat(32);
    let want = format!(
        "\
Culla apply log
version      0.1.0
plan         12 (apply, first run)
started      2026-11-28 09:14:03 +01:00
source       E:\\DCIM\\100NIKON
destination  D:\\Photos\\2026
rejected     the _rejected folder in the source

alarms: a file is not where the journal says. Run chkdsk on the volume before anything else.
  op 8  group 6  reject  rename  source \"DSC_0006.JPG\" → rejected \"DSC_0006.JPG\"  failed (lost)

operations, in plan order
  op 1  group 1  reject  rename  source \"DSC_0001.JPG\" → rejected \"DSC_0001.JPG\"  done
  op 2  group 1  reject  rename  source \"DSC_0001.NEF\" → rejected \"DSC_0001.NEF\"  committed
  op 3  group 2  keep  xmove  source \"DSC_0002.JPG\" → destination \"DSC_0002.JPG\"  done  blake3 {ab}
  op 4  group 3  keep  xmove  source \"DSC_0003.JPG\" → destination \"DSC_0003.JPG\"  copied_source_kept  blake3 {one}
  op 5  group 3  keep  xmove  source \"DSC_0003.NEF\" → destination \"DSC_0003.NEF\"  failed (verify_mismatch)  attempts 2  blake3 {two}
  op 6  group 4  keep  copy  source \"sub\\DSC_0004.JPG\" → destination \"sub\\DSC_0004_1.JPG\"  done  blake3 {cd}
  op 7  group 5  reject  rename  source \"DSC_0005.JPG\" → rejected \"DSC_0005.JPG\"  failed (io 1392)
  op 8  group 6  reject  rename  source \"DSC_0006.JPG\" → rejected \"DSC_0006.JPG\"  failed (lost)
  op 9  group 7  reject  rename  source \"DSC_0007.JPG\" → rejected \"DSC_0007.JPG\"  pending

summary
  operations          9
  pending             1
  committed           1
  done                3
  copied_source_kept  1
  failed              3
  alarms              1
end          paused (the destination is full)
ended        2026-11-28 09:15:41 +01:00
"
    );
    let got = render(
        &header(),
        &mixed(),
        &footer(RunEnd::Paused(PauseReason::DiskFull)),
    );
    assert_eq!(got, want);
}

#[test]
fn one_line_per_kind_and_outcome() {
    struct Case {
        kind: Kind,
        root: Root,
        steps: Vec<(OpState, Detail)>,
        want: &'static str,
    }
    let fin = |r| (OpState::Failed, Detail::Failed(r));
    let mut xmove_done = copy_committed(7);
    xmove_done.extend([(OpState::SourceRemoved, N), (OpState::Done, N)]);
    let mut copy_done = copy_committed(7);
    copy_done.push((OpState::Done, N));
    let mut xmove_kept = copy_committed(7);
    xmove_kept.push((OpState::CopiedSourceKept, N));
    let h = "07".repeat(32);
    let f0 = "f0".repeat(32);
    let cases = vec![
        Case {
            kind: Kind::Rename,
            root: Root::Rejected,
            steps: vec![],
            want: "pending",
        },
        Case {
            kind: Kind::Rename,
            root: Root::Dest,
            steps: vec![(OpState::Renaming, N)],
            want: "renaming",
        },
        Case {
            kind: Kind::Rename,
            root: Root::Dest,
            steps: rename_done(),
            want: "done",
        },
        Case {
            kind: Kind::Rename,
            root: Root::Rejected,
            steps: vec![fin(FailReason::Conflict)],
            want: "failed (conflict)",
        },
        Case {
            kind: Kind::Rename,
            root: Root::Rejected,
            steps: vec![(OpState::Renaming, N), fin(FailReason::FsInconsistent)],
            want: "failed (fs_inconsistent)",
        },
        Case {
            kind: Kind::Copy,
            root: Root::Dest,
            steps: vec![(OpState::Copying, attempt(1))],
            want: "copying",
        },
        Case {
            kind: Kind::Copy,
            root: Root::Dest,
            steps: copy_done,
            want: "done  blake3 H",
        },
        Case {
            kind: Kind::Copy,
            root: Root::Dest,
            steps: vec![(OpState::Copying, attempt(1)), fin(FailReason::ShortCopy)],
            want: "failed (short_copy)",
        },
        Case {
            kind: Kind::XMove,
            root: Root::Dest,
            steps: vec![
                (OpState::Copying, attempt(1)),
                (OpState::Verifying, copied(7)),
            ],
            want: "verifying  blake3 H",
        },
        Case {
            kind: Kind::XMove,
            root: Root::Dest,
            steps: xmove_done,
            want: "done  blake3 H",
        },
        Case {
            kind: Kind::XMove,
            root: Root::Dest,
            steps: xmove_kept,
            want: "copied_source_kept  blake3 H",
        },
        Case {
            kind: Kind::XMove,
            root: Root::Dest,
            steps: vec![fin(FailReason::GroupFailed)],
            want: "failed (group_failed)",
        },
        Case {
            kind: Kind::XMove,
            root: Root::Dest,
            steps: vec![fin(FailReason::SourceChanged)],
            want: "failed (source_changed)",
        },
        Case {
            kind: Kind::Finish,
            root: Root::Dest,
            steps: vec![],
            want: "pending  blake3 F",
        },
        Case {
            kind: Kind::Finish,
            root: Root::Dest,
            steps: vec![
                (OpState::Verifying, N),
                (OpState::Committed, N),
                (OpState::SourceRemoved, N),
                (OpState::Done, N),
            ],
            want: "done  blake3 F",
        },
        Case {
            kind: Kind::Finish,
            root: Root::Dest,
            steps: vec![(OpState::Verifying, N), fin(FailReason::VerifyMismatch)],
            want: "failed (verify_mismatch)  blake3 F",
        },
    ];
    for c in cases {
        let mut j = journal(vec![op(
            5,
            2,
            c.kind,
            c.root,
            &["a", "X.JPG"],
            &["a", "X_1.JPG"],
        )]);
        walk(&mut j, 5, &c.steps);
        let log = render(&header(), &j, &footer(RunEnd::Finished));
        let action = if c.root == Root::Rejected {
            "reject"
        } else {
            "keep"
        };
        let root = if c.root == Root::Rejected {
            "rejected"
        } else {
            "destination"
        };
        let want = format!(
            "  op 5  group 2  {action}  {}  source \"a\\X.JPG\" → {root} \"a\\X_1.JPG\"  {}\n",
            c.kind.as_str(),
            c.want.replace('H', &h).replace('F', &f0)
        );
        assert!(log.contains(&want), "{:?} {}: {log}", c.kind, c.want);
        let state = c
            .want
            .split("  ")
            .next()
            .unwrap()
            .split(' ')
            .next()
            .unwrap();
        assert!(
            log.contains(&format!("\n  {state:<20}1\n")),
            "summary for {state}: {log}"
        );
    }
}

#[test]
fn every_run_end_is_named() {
    let j = journal(vec![]);
    for (end, want) in [
        (RunEnd::Finished, "end          finished\n"),
        (RunEnd::Cancelled, "end          cancelled\n"),
        (
            RunEnd::Paused(PauseReason::User),
            "end          paused (by the user)\n",
        ),
        (
            RunEnd::Paused(PauseReason::VolumeLost),
            "end          paused (a volume went away)\n",
        ),
        (
            RunEnd::Paused(PauseReason::WriteProtected),
            "end          paused (the volume is write-protected)\n",
        ),
        (
            RunEnd::Stopped,
            "end          stopped: the journal could not be written; resumes on the next launch\n",
        ),
        (
            RunEnd::InternalError,
            "end          stopped by an internal error in Culla; report it with this log\n",
        ),
    ] {
        assert!(render(&header(), &j, &footer(end)).contains(want), "{want}");
    }
    assert_eq!(RunEnd::from(End::Finished), RunEnd::Finished);
    assert_eq!(RunEnd::from(End::Cancelled), RunEnd::Cancelled);
    assert_eq!(
        RunEnd::from(End::Paused(PauseReason::VolumeLost)),
        RunEnd::Paused(PauseReason::VolumeLost)
    );
    assert_eq!(RunEnd::from(&RunError::Store(StoreError)), RunEnd::Stopped);
    assert_eq!(
        RunEnd::from(&RunError::Journal(JournalError::UnknownOp(1))),
        RunEnd::InternalError
    );
}

#[test]
fn empty_plan_redo_and_no_destination() {
    let h = Header {
        purpose: Purpose::Redo,
        run: RunKind::Resume,
        destination: None,
        ..header()
    };
    let log = render(&h, &journal(vec![]), &footer(RunEnd::Finished));
    assert!(log.contains("plan         12 (redo, resume)\n"));
    assert!(log.contains("destination  none\n"));
    assert!(log.contains("\noperations: none\n"));
    assert!(log.contains("  operations          0\nend"));
    assert!(!log.contains("alarms"));
}

#[test]
fn untrusted_names_cannot_add_lines_hide_text_or_close_quotes() {
    let nasty = [
        "a\nop 99  group 1  keep  copy  source \"x\" → destination \"x\"  done",
        "gpj.\u{202E}EXE",
        "a\u{200B}b\u{2066}c\u{FEFF}d\u{2028}e",
        "\u{1b}[2Kfake",
        "x\" → rejected \"y",
        "fake\\folder",
        "tag\u{E0041}s\r",
    ];
    let ops: Vec<Op> = nasty
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let id = OpId::try_from(i).unwrap() + 1;
            op(
                id,
                id,
                Kind::Rename,
                Root::Rejected,
                &["d\u{202D}ir", n],
                &[n],
            )
        })
        .collect();
    let mut j = journal(ops);
    walk(&mut j, 1, &rename_done());
    let h = Header {
        started: "09:00\n\u{1b}]0;title\u{7}",
        source: "E:\\\u{202E}MICD",
        destination: Some("D:\\Ph\u{200D}otos\nsource       F:\\"),
        ..header()
    };
    let f = Footer {
        end: RunEnd::Finished,
        ended: "10:00\u{2029}",
    };
    let log = render(&h, &j, &f);

    for c in log.chars() {
        assert!(
            c == '\n' || !text::needs_escape(c),
            "unescaped {c:?} in {log}"
        );
    }
    // Header 7 lines, blank, title, 7 operations, blank, summary 4 lines, end, ended.
    assert_eq!(log.lines().count(), 7 + 1 + 1 + 7 + 1 + 4 + 2, "{log}");
    assert_eq!(log.matches("\nsource ").count(), 1, "{log}");
    for line in log.lines().filter(|l| l.starts_with("  op ")) {
        // The quotes are the path delimiters: a quote inside a name is escaped.
        assert_eq!(line.matches('"').count(), 4, "{line}");
    }
    assert!(log.contains(
        "  op 2  group 2  reject  rename  source \"d\\u{202d}ir\\gpj.\\u{202e}EXE\" → rejected \"gpj.\\u{202e}EXE\"  pending\n"
    ));
    assert!(log.contains("source \"d\\u{202d}ir\\x\\u{22} → rejected \\u{22}y\""));
    assert!(log.contains("\"d\\u{202d}ir\\fake\\u{5c}folder\""));
    assert!(log.contains("started      09:00\\u{a}\\u{1b}]0;title\\u{7}\n"));
    assert!(log.contains("source       E:\\\\u{202e}MICD\n"));
    assert!(log.contains("destination  D:\\Ph\\u{200d}otos\\u{a}source       F:\\\n"));
    assert!(log.contains("ended        10:00\\u{2029}\n"));

    // `\u{` only ever starts an escape: a `{` in a name is escaped, so a name that spells an
    // escape cannot render like a separator plus an escaped character, or like a `\` inside a
    // name.
    let pairs: [(&[&str], &[&str]); 2] = [
        (&["dir", "u{a}x"], &["dir\nx"]),
        (&["a", "u{5c}b"], &["a\\b"]),
    ];
    for (left, right) in pairs {
        let render_one = |src: &[&str]| {
            let mut j = journal(vec![op(1, 1, Kind::Rename, Root::Rejected, src, &["x"])]);
            walk(&mut j, 1, &rename_done());
            render(&header(), &j, &footer(RunEnd::Finished))
        };
        let (a, b) = (render_one(left), render_one(right));
        assert_ne!(a, b, "{left:?} and {right:?} render alike");
    }
    let mut k = journal(vec![
        op(
            1,
            1,
            Kind::Rename,
            Root::Rejected,
            &["dir", "u{a}x"],
            &["x"],
        ),
        op(2, 2, Kind::Rename, Root::Rejected, &["dir\nx"], &["y"]),
        op(3, 3, Kind::Rename, Root::Rejected, &["a", "u{5c}b"], &["z"]),
        op(4, 4, Kind::Rename, Root::Rejected, &["a\\b"], &["w"]),
    ]);
    walk(&mut k, 1, &rename_done());
    let h = Header {
        source: "E:\\u{a}x",
        destination: Some("E:\n"),
        ..header()
    };
    let log = render(&h, &k, &footer(RunEnd::Finished));
    assert!(log.contains("source \"dir\\u\\u{7b}a}x\""), "{log}");
    assert!(log.contains("source \"dir\\u{a}x\""), "{log}");
    assert!(log.contains("source \"a\\u\\u{7b}5c}b\""), "{log}");
    assert!(log.contains("source \"a\\u{5c}b\""), "{log}");
    assert!(log.contains("source       E:\\u\\u{7b}a}x\n"), "{log}");
    assert!(log.contains("destination  E:\\u{a}\n"), "{log}");
}

#[test]
fn rendering_is_deterministic() {
    let a = render(&header(), &mixed(), &footer(RunEnd::Cancelled));
    let b = render(&header(), &mixed(), &footer(RunEnd::Cancelled));
    assert_eq!(a, b);
    // A journal rebuilt from the same entries renders the same bytes.
    let mut j = journal(vec![
        op(1, 1, Kind::Copy, Root::Dest, &["A.JPG"], &["A.JPG"]),
        op(2, 1, Kind::Copy, Root::Dest, &["A.NEF"], &["A.NEF"]),
    ]);
    let entries = [
        Entry {
            op: 1,
            from: OpState::Pending,
            to: OpState::Copying,
            detail: attempt(1),
        },
        Entry {
            op: 2,
            from: OpState::Pending,
            to: OpState::Copying,
            detail: attempt(1),
        },
        Entry {
            op: 1,
            from: OpState::Copying,
            to: OpState::Verifying,
            detail: copied(9),
        },
    ];
    let mut k = j.clone();
    j.replay(&entries).unwrap();
    for e in &entries {
        k.apply(e).unwrap();
    }
    assert_eq!(
        render(&header(), &j, &footer(RunEnd::Finished)),
        render(&header(), &k, &footer(RunEnd::Finished))
    );
    assert!(!a.contains('\r'));
    assert!(a.ends_with('\n'));
}

#[test]
fn the_log_holds_no_exif_or_serial() {
    // The inputs carry none (Header has display strings only, Op has no EXIF), so the only
    // numbers in the log are op and group ids, the plan id, attempts, Win32 codes, the version,
    // the caller's times and hashes. Pin the field names so a new one is a deliberate change.
    let log = render(&header(), &mixed(), &footer(RunEnd::Finished));
    let fields: Vec<&str> = log
        .lines()
        .filter(|l| !l.starts_with(' ') && !l.is_empty())
        .map(|l| l.split(&[' ', ':', ','][..]).next().unwrap())
        .collect();
    assert_eq!(
        fields,
        [
            "Culla",
            "version",
            "plan",
            "started",
            "source",
            "destination",
            "rejected",
            "alarms",
            "operations",
            "summary",
            "end",
            "ended"
        ]
    );
    for word in ["serial", "gps", "GPS", "latitude", "lens", "camera"] {
        assert!(!log.contains(word), "{word}");
    }
}
