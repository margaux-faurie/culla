//! The text output of `culla scan`, and escaping for anything printed to a terminal.

use std::borrow::Cow;
use std::fmt::Write;

use culla_core::volume::VolumeId;

use crate::labels;
use crate::scan::{Identity, Report, Totals, exclusion_detail, group_files, is_folder};

/// `s` with every character that could change what a terminal shows escaped as `\u{..}`:
/// control characters (escape sequences, carriage returns), bidi controls and isolates,
/// zero-width characters, the BOM and the line and paragraph separators. File names and
/// volume labels come from the card, so they are untrusted (threat model T6).
#[must_use]
pub fn shown(s: &str) -> Cow<'_, str> {
    if !s.chars().any(needs_escape) {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        if needs_escape(c) {
            let _ = write!(out, "\\u{{{:x}}}", u32::from(c));
        } else {
            out.push(c);
        }
    }
    Cow::Owned(out)
}

/// `n` and the noun, plural unless `n` is 1 (`entry` becomes `entries`).
#[must_use]
pub fn count(n: u64, noun: &str) -> String {
    match (n, noun.strip_suffix('y')) {
        (1, _) => format!("1 {noun}"),
        (_, Some(stem)) => format!("{n} {stem}ies"),
        (_, None) => format!("{n} {noun}s"),
    }
}

fn needs_escape(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{200B}'..='\u{200F}'
                | '\u{2028}'..='\u{202E}'
                | '\u{2060}'..='\u{2069}'
                | '\u{FEFF}'
                | '\u{FFF9}'..='\u{FFFB}'
        )
}

/// The report as text, ending with a newline.
#[must_use]
pub fn render(report: &Report) -> String {
    let mut o = String::new();
    let line = |o: &mut String, s: &str| {
        o.push_str(s);
        o.push('\n');
    };
    line(
        &mut o,
        &format!("folder:      {}", shown(&report.folder.to_string_lossy())),
    );
    if report.scanned != report.folder {
        line(
            &mut o,
            &format!("resolved:    {}", shown(&report.scanned.to_string_lossy())),
        );
    }
    line(
        &mut o,
        &format!(
            "subfolders:  {}",
            if report.include_subfolders {
                "included"
            } else {
                "not included"
            }
        ),
    );
    match &report.identity {
        Identity::Resolved(r) => {
            let v = &r.volume;
            let label = v.label.as_deref().map_or_else(
                || "no label".to_owned(),
                |l| format!("label \"{}\"", shown(l)),
            );
            line(
                &mut o,
                &format!(
                    "volume:      serial {}, {label}, drive kind {}, mounted at {}, {}{}",
                    v.id,
                    labels::drive_kind(v.drive_kind),
                    shown(&v.mount_point),
                    shown(&v.fs_name),
                    if v.read_only() { ", read-only" } else { "" }
                ),
            );
            let key = r.source.key();
            line(
                &mut o,
                &format!("source:      \\{} on {}", shown(&r.source.rel_path()), v.id),
            );
            line(
                &mut o,
                &format!(
                    "source key:  {}\\{}",
                    VolumeId::new(key.serial),
                    shown(&key.rel_key)
                ),
            );
        }
        Identity::Unavailable(why) => {
            line(&mut o, &format!("volume:      unavailable ({why})"));
            line(&mut o, "source key:  unavailable");
        }
    }
    line(
        &mut o,
        &format!(
            "scanned:     {}, {}",
            count(report.summary.folders, "folder"),
            count(report.summary.entries, "entry")
        ),
    );

    let t = Totals::of(report);
    o.push('\n');
    line(&mut o, "totals");
    for (name, n) in [
        ("groups", t.groups),
        ("pairs", t.pairs),
        ("lone NEFs", t.lone_nefs),
        ("JPEG only", t.jpeg_only),
        ("ambiguous", t.ambiguous),
        ("held back", t.held_back),
        ("unknown files", t.unknown_files),
        ("excluded", t.excluded),
    ] {
        line(&mut o, &format!("  {name:<16}{n}"));
    }
    for (r, n) in &t.excluded_by_reason {
        line(&mut o, &format!("    {n} {r}"));
    }
    if t.metadata_errors > 0 {
        line(
            &mut o,
            &format!("  {:<16}{}", "metadata errors", t.metadata_errors),
        );
    }

    let g = &report.grouping;
    o.push('\n');
    if g.groups.is_empty() {
        line(&mut o, "no photos found");
    } else {
        line(&mut o, "photos in capture-time order");
    }
    let width = g.groups.len().to_string().len();
    for (pos, &i) in report.order.iter().enumerate() {
        let group = &g.groups[i];
        let ambiguous = if group.status == culla_core::grouping::GroupStatus::Ambiguous {
            "  (ambiguous)"
        } else {
            ""
        };
        line(
            &mut o,
            &format!(
                "{:>width$}. {}{ambiguous}",
                pos + 1,
                shown(&group.primary.entry.rel_path())
            ),
        );
        let pad = " ".repeat(width + 2);
        match &report.meta[i] {
            Ok(m) => {
                let captured = m.captured.map_or_else(
                    || "unknown".to_owned(),
                    |c| {
                        format!(
                            "{} ({})",
                            c.to_iso8601(),
                            labels::time_source_text(c.source)
                        )
                    },
                );
                line(&mut o, &format!("{pad}captured     {captured}"));
                line(
                    &mut o,
                    &format!(
                        "{pad}orientation  {} ({})",
                        m.orientation.exif_value(),
                        labels::orientation_text(m.orientation)
                    ),
                );
                line(
                    &mut o,
                    &format!(
                        "{pad}lens         {}",
                        m.lens.as_deref().map_or(Cow::Borrowed("unknown"), shown)
                    ),
                );
                if !m.issues.is_empty() {
                    let issues: Vec<String> = m.issues.iter().map(|i| labels::issue(*i)).collect();
                    line(&mut o, &format!("{pad}issues       {}", issues.join(", ")));
                }
            }
            Err(e) => line(&mut o, &format!("{pad}metadata     unreadable: {e}")),
        }
        let files: Vec<String> = group_files(group)
            .into_iter()
            .map(|(p, _, _)| shown(&p).into_owned())
            .collect();
        line(&mut o, &format!("{pad}files        {}", files.join(", ")));
    }

    if !g.unknown.is_empty() {
        o.push('\n');
        line(&mut o, "unknown files (listed, never touched)");
        for u in &g.unknown {
            line(
                &mut o,
                &format!(
                    "  {}  ({})",
                    shown(&u.entry.rel_path()),
                    u.kind.issue_kind()
                ),
            );
        }
    }
    if !g.excluded.is_empty() {
        o.push('\n');
        line(&mut o, "excluded");
        for x in &g.excluded {
            let slash = if is_folder(&x.entry.kind) { "\\" } else { "" };
            let detail = exclusion_detail(&x.entry.kind)
                .map(|d| format!(": {}", shown(d)))
                .unwrap_or_default();
            line(
                &mut o,
                &format!(
                    "  {}{slash}  ({}{detail})",
                    shown(&x.entry.rel_path()),
                    x.reason
                ),
            );
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_are_plural_unless_one() {
        assert_eq!(count(1, "folder"), "1 folder");
        assert_eq!(count(0, "folder"), "0 folders");
        assert_eq!(count(1, "entry"), "1 entry");
        assert_eq!(count(14, "entry"), "14 entries");
    }

    #[test]
    fn terminal_controls_are_escaped() {
        assert!(matches!(shown("DSC_0001.JPG"), Cow::Borrowed(_)));
        assert_eq!(shown("é 📷 ok"), "é 📷 ok");
        assert_eq!(shown("a\u{1b}[31mb"), "a\\u{1b}[31mb");
        assert_eq!(shown("x\r\ny\t"), "x\\u{d}\\u{a}y\\u{9}");
        assert_eq!(shown("\u{9b}"), "\\u{9b}");
        assert_eq!(shown("gpj.\u{202E}EXE"), "gpj.\\u{202e}EXE");
        assert_eq!(
            shown("a\u{200B}\u{2066}\u{FEFF}\u{2028}"),
            "a\\u{200b}\\u{2066}\\u{feff}\\u{2028}"
        );
    }
}
