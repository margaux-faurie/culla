//! The plain-text apply log (design/file-engine.md §9.8; NFR-REL-14; US-20; threat model T15):
//! one file per run of a plan (an apply, a resume, a redo), written by the adapter when the run
//! ends to `%LOCALAPPDATA%\Culla\logs`, append-only with `create_new`. This module only renders
//! the text; it does no I/O.
//!
//! The text is a pure function of the [`Header`], the [`Journal`] and the [`Footer`]: the same
//! inputs give the same bytes (no clock, no locale, no map iteration order). Layout:
//!
//! - a header: Culla's version, the plan id and purpose, whether this run was the first one or a
//!   resume, the start time as the caller formatted it, and the source and destination as the
//!   display strings the route view shows;
//! - the alarms (`failed (lost)`, `failed (fs_inconsistent)`), first, with the advice to run
//!   `chkdsk` (§10.2), only when there is one;
//! - one line per operation, in plan order, with its final state for this run: `op <id>`, group,
//!   action, kind, `source "<path>" → rejected|destination "<path>"`, the state (`failed` with
//!   its reason), the attempts when more than one, and H1 (`blake3 <hex>`) once the journal has
//!   it (H1 = H2 = H3 for a committed copy, §9.3);
//! - a footer: the count of operations per state, the alarms, how the run ended, and the end time
//!   as the caller formatted it.
//!
//! Every untrusted string is escaped with the shared set of [`crate::text`] (control, bidi,
//! zero-width and other invisible characters), so a name cannot add a line, hide text or
//! reorder a line (T6b). Paths are quoted, and a quote or a `\` inside a name is escaped too, so
//! a name can neither end its quotes nor pose as a folder. The header's display strings are
//! escaped but not quoted: each is alone on its line.
//!
//! What it never holds (NFR-PRIV-06, SPEC "Sample photos"): no EXIF (so no GPS position and no
//! body serial), no volume serial, no hash of anything but the file contents, no user name
//! beyond what the display strings already show. The types have no field for any of it.

use std::fmt::Write;

use crate::journal::{FailReason, Hash, Journal, Op, OpState, PauseReason, Progress};
use crate::plan::Root;
use crate::text;

/// What kind of plan this is (`plan.purpose`, §9.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Purpose {
    /// The plan built from the decisions.
    Apply,
    /// A redo plan: "Redo moves" or "Finish moves" after the plan closed.
    Redo,
}

impl Purpose {
    /// The `plan.purpose` value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Apply => "apply",
            Self::Redo => "redo",
        }
    }
}

/// Whether this run started the plan or picked it up again (§10.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RunKind {
    /// The plan's first run, after the user confirmed Apply.
    First,
    /// A resume: after a pause, a crash or a pulled card, or "Retry failed" and "Finish moves".
    Resume,
}

impl RunKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::First => "first run",
            Self::Resume => "resume",
        }
    }
}

/// The log's first lines. The strings come from the caller: display strings for the folders
/// (as the route view shows them, for example `E:\DCIM\100NIKON`) and the start time already
/// formatted (the adapter chooses the clock and the time zone). They are escaped, never
/// trusted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header<'a> {
    /// Culla's version, normally [`crate::VERSION`].
    pub version: &'a str,
    /// `plan.id`.
    pub plan: i64,
    /// `plan.purpose`.
    pub purpose: Purpose,
    /// First run or resume.
    pub run: RunKind,
    /// When this run started.
    pub started: &'a str,
    /// The source folder.
    pub source: &'a str,
    /// The destination folder; `None` when the plan has no keeps.
    pub destination: Option<&'a str>,
}

/// How the run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RunEnd {
    /// Nothing left to do ([`crate::journal::End::Finished`]).
    Finished,
    /// Paused, to resume later.
    Paused(PauseReason),
    /// Cancelled by the user.
    Cancelled,
    /// The journal could not be written: the run stopped as after a crash and resumes on the
    /// next launch ([`crate::journal::RunError::Store`]).
    Stopped,
}

impl From<crate::journal::End> for RunEnd {
    fn from(e: crate::journal::End) -> Self {
        use crate::journal::End;
        match e {
            End::Finished => Self::Finished,
            End::Paused(r) => Self::Paused(r),
            End::Cancelled => Self::Cancelled,
        }
    }
}

/// The log's last lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Footer<'a> {
    /// How the run ended.
    pub end: RunEnd,
    /// When it ended, formatted by the caller.
    pub ended: &'a str,
}

/// The log of one run of a plan, ending with a newline. Lines end with `\n` only.
#[must_use]
pub fn render(header: &Header<'_>, journal: &Journal, footer: &Footer<'_>) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "Culla apply log");
    let _ = writeln!(o, "version      {}", text::shown(header.version));
    let _ = writeln!(
        o,
        "plan         {} ({}, {})",
        header.plan,
        header.purpose.as_str(),
        header.run.as_str()
    );
    let _ = writeln!(o, "started      {}", text::shown(header.started));
    let _ = writeln!(o, "source       {}", text::shown(header.source));
    let _ = writeln!(
        o,
        "destination  {}",
        header.destination.map_or("none".into(), text::shown)
    );
    let _ = writeln!(o, "rejected     the _rejected folder in the source");

    let rows: Vec<(&Op, &Progress)> = journal
        .ops()
        .iter()
        .enumerate()
        .map(|(i, op)| (op, journal.progress(i)))
        .collect();

    let alarms: Vec<&(&Op, &Progress)> = rows.iter().filter(|(_, p)| is_alarm(p)).collect();
    if !alarms.is_empty() {
        o.push('\n');
        let _ = writeln!(
            o,
            "alarms: a file is not where the journal says. Run chkdsk on the volume before \
             anything else."
        );
        for (op, p) in alarms {
            let _ = writeln!(o, "  {}", line(op, p));
        }
    }

    o.push('\n');
    if rows.is_empty() {
        let _ = writeln!(o, "operations: none");
    } else {
        let _ = writeln!(o, "operations, in plan order");
        for (op, p) in &rows {
            let _ = writeln!(o, "  {}", line(op, p));
        }
    }

    o.push('\n');
    let _ = writeln!(o, "summary");
    let _ = writeln!(o, "  operations          {}", rows.len());
    for state in OpState::ALL {
        let n = rows.iter().filter(|(_, p)| p.state() == state).count();
        if n > 0 {
            let _ = writeln!(o, "  {:<20}{n}", state.as_str());
        }
    }
    let n_alarms = rows.iter().filter(|(_, p)| is_alarm(p)).count();
    if n_alarms > 0 {
        let _ = writeln!(o, "  {:<20}{n_alarms}", "alarms");
    }
    let _ = writeln!(o, "end          {}", end_text(footer.end));
    let _ = writeln!(o, "ended        {}", text::shown(footer.ended));
    o
}

fn is_alarm(p: &Progress) -> bool {
    p.state() == OpState::Failed && p.last_error.is_some_and(FailReason::is_alarm)
}

/// One operation: `op 3  group 2  keep  xmove  source "a\b.NEF" → destination "a\b.NEF"  done
/// blake3 <hex>`.
fn line(op: &Op, p: &Progress) -> String {
    let mut s = format!(
        "op {}  group {}  {}  {}  source {} → {} {}  {}",
        op.id,
        op.group,
        op.action.as_str(),
        op.kind.as_str(),
        quoted(&op.src),
        root_text(op.root),
        quoted(&op.dst),
        state_text(p),
    );
    if p.attempts > 1 {
        let _ = write!(s, "  attempts {}", p.attempts);
    }
    if let Some(h) = p.hash {
        let _ = write!(s, "  blake3 {}", hex(&h));
    }
    s
}

fn root_text(r: Root) -> &'static str {
    match r {
        Root::Dest => "destination",
        Root::Rejected => "rejected",
    }
}

fn state_text(p: &Progress) -> String {
    let state = p.state();
    if state != OpState::Failed {
        return state.as_str().to_owned();
    }
    match p.last_error {
        Some(FailReason::Io(code)) => format!("failed (io {code})"),
        Some(r) => format!("failed ({})", r.as_str()),
        None => "failed".to_owned(),
    }
}

fn end_text(e: RunEnd) -> String {
    match e {
        RunEnd::Finished => "finished".to_owned(),
        RunEnd::Paused(r) => format!("paused ({})", pause_text(r)),
        RunEnd::Cancelled => "cancelled".to_owned(),
        RunEnd::Stopped => {
            "stopped: the journal could not be written; resumes on the next launch".to_owned()
        }
    }
}

fn pause_text(r: PauseReason) -> &'static str {
    match r {
        PauseReason::User => "by the user",
        PauseReason::VolumeLost => "a volume went away",
        PauseReason::WriteProtected => "the volume is write-protected",
        PauseReason::DiskFull => "the destination is full",
    }
}

/// Components joined with `\`, in quotes. Each component is escaped with the shared set plus
/// the quote and the separator, so it cannot close the quotes or fake a folder.
fn quoted(components: &[String]) -> String {
    let parts: Vec<String> = components
        .iter()
        .map(|c| text::escaped(c, |ch| matches!(ch, '"' | '\\')).into_owned())
        .collect();
    format!("\"{}\"", parts.join("\\"))
}

fn hex(h: &Hash) -> String {
    let mut s = String::with_capacity(64);
    for b in h {
        let _ = write!(s, "{b:02x}");
    }
    s
}

#[cfg(test)]
mod tests;
