//! Command line parsing, std only: `culla scan <folder> [--no-subfolders] [--json]`,
//! `culla preview <folder> [--no-subfolders]`, `culla plan <folder> --decisions <log-file>
//! [--keep move|copy --dest <folder>] [--raw-follows yes|no] [--no-subfolders] [--json]`,
//! `culla help [scan|preview|plan]`, `culla --version`.

use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

use culla_core::plan::KeepMode;

/// What to run.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    /// Print help: the overview, or one command's.
    Help(Topic),
    /// Print the version.
    Version,
    /// Scan a folder.
    Scan(ScanArgs),
    /// Scan a folder and build its previews.
    Preview(PreviewArgs),
    /// Scan a folder, fold a decision log and print the plan and its preflight (a dry run).
    Plan(PlanArgs),
}

/// A help page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Topic {
    /// Every command.
    Overview,
    /// `culla scan`.
    Scan,
    /// `culla preview`.
    Preview,
    /// `culla plan`.
    Plan,
}

/// The arguments of `culla scan`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ScanArgs {
    /// The folder to scan, as given.
    pub folder: PathBuf,
    /// Scan subfolders too (default on, off with `--no-subfolders`).
    pub include_subfolders: bool,
    /// Print JSON instead of text.
    pub json: bool,
}

/// The arguments of `culla preview`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PreviewArgs {
    /// The folder to scan, as given.
    pub folder: PathBuf,
    /// Scan subfolders too (default on, off with `--no-subfolders`).
    pub include_subfolders: bool,
}

/// The arguments of `culla plan`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PlanArgs {
    /// The folder to scan, as given.
    pub folder: PathBuf,
    /// The decision log, in the core text form (`culla_core::decisions::codec`).
    pub decisions: PathBuf,
    /// Keep by move or copy, and the destination: both given or neither.
    pub keep: Option<(KeepMode, PathBuf)>,
    /// "RAW follows JPEG" (default on, as on the start screen).
    pub raw_follows: bool,
    /// Scan subfolders too (default on, off with `--no-subfolders`).
    pub include_subfolders: bool,
    /// Print JSON instead of text.
    pub json: bool,
}

/// The command line is not valid. The text says why; usage follows it on stderr.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct UsageError {
    /// What is wrong.
    pub message: String,
    /// The help page to point to.
    pub topic: Topic,
}

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

fn usage(topic: Topic, message: impl Into<String>) -> UsageError {
    UsageError {
        message: message.into(),
        topic,
    }
}

/// Parses the arguments after the program name.
///
/// # Errors
///
/// [`UsageError`] for no command, an unknown command or option, an option given twice, a
/// missing or extra folder.
pub(crate) fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, UsageError> {
    let mut args = args.into_iter();
    let Some(first) = args.next() else {
        return Err(usage(Topic::Overview, "no command given"));
    };
    let rest: Vec<OsString> = args.collect();
    match first.to_str() {
        Some("help" | "--help" | "-h") => parse_help(&rest),
        Some("--version" | "-V" | "version") => {
            if let Some(extra) = rest.first() {
                return Err(usage(
                    Topic::Overview,
                    format!("unexpected argument {}", quoted(extra)),
                ));
            }
            Ok(Command::Version)
        }
        Some("scan") => parse_scan(rest),
        Some("preview") => parse_preview(rest),
        Some("plan") => parse_plan(rest),
        _ => Err(usage(
            Topic::Overview,
            format!("unknown command {}", quoted(&first)),
        )),
    }
}

fn parse_help(rest: &[OsString]) -> Result<Command, UsageError> {
    match rest {
        [] => Ok(Command::Help(Topic::Overview)),
        [topic] if topic == "scan" => Ok(Command::Help(Topic::Scan)),
        [topic] if topic == "preview" => Ok(Command::Help(Topic::Preview)),
        [topic] if topic == "plan" => Ok(Command::Help(Topic::Plan)),
        [topic] => Err(usage(
            Topic::Overview,
            format!("no help for {}", quoted(topic)),
        )),
        [_, extra, ..] => Err(usage(
            Topic::Overview,
            format!("unexpected argument {}", quoted(extra)),
        )),
    }
}

fn parse_scan(rest: Vec<OsString>) -> Result<Command, UsageError> {
    let mut folder: Option<OsString> = None;
    let mut no_subfolders = false;
    let mut json = false;
    let mut options_done = false;
    for arg in rest {
        let is_option = !options_done && arg.to_str().is_some_and(|s| s.starts_with('-'));
        if is_option {
            let flag = match arg.to_str() {
                Some("--") => {
                    options_done = true;
                    continue;
                }
                Some("--help" | "-h") => return Ok(Command::Help(Topic::Scan)),
                Some("--no-subfolders") => &mut no_subfolders,
                Some("--json") => &mut json,
                _ => {
                    return Err(usage(
                        Topic::Scan,
                        format!("unknown option {}", quoted(&arg)),
                    ));
                }
            };
            if *flag {
                return Err(usage(Topic::Scan, format!("{} given twice", quoted(&arg))));
            }
            *flag = true;
        } else if folder.is_some() {
            return Err(usage(
                Topic::Scan,
                format!("unexpected argument {}", quoted(&arg)),
            ));
        } else if arg.is_empty() {
            return Err(usage(Topic::Scan, "the folder is empty"));
        } else {
            folder = Some(arg);
        }
    }
    let Some(folder) = folder else {
        return Err(usage(Topic::Scan, "no folder given"));
    };
    Ok(Command::Scan(ScanArgs {
        folder: PathBuf::from(folder),
        include_subfolders: !no_subfolders,
        json,
    }))
}

fn parse_preview(rest: Vec<OsString>) -> Result<Command, UsageError> {
    let mut folder: Option<OsString> = None;
    let mut no_subfolders = false;
    let mut options_done = false;
    for arg in rest {
        let is_option = !options_done && arg.to_str().is_some_and(|s| s.starts_with('-'));
        if is_option {
            match arg.to_str() {
                Some("--") => options_done = true,
                Some("--help" | "-h") => return Ok(Command::Help(Topic::Preview)),
                Some("--no-subfolders") => {
                    if no_subfolders {
                        return Err(usage(
                            Topic::Preview,
                            format!("{} given twice", quoted(&arg)),
                        ));
                    }
                    no_subfolders = true;
                }
                _ => {
                    return Err(usage(
                        Topic::Preview,
                        format!("unknown option {}", quoted(&arg)),
                    ));
                }
            }
        } else if folder.is_some() {
            return Err(usage(
                Topic::Preview,
                format!("unexpected argument {}", quoted(&arg)),
            ));
        } else if arg.is_empty() {
            return Err(usage(Topic::Preview, "the folder is empty"));
        } else {
            folder = Some(arg);
        }
    }
    let Some(folder) = folder else {
        return Err(usage(Topic::Preview, "no folder given"));
    };
    Ok(Command::Preview(PreviewArgs {
        folder: PathBuf::from(folder),
        include_subfolders: !no_subfolders,
    }))
}

fn parse_plan(rest: Vec<OsString>) -> Result<Command, UsageError> {
    let mut folder: Option<OsString> = None;
    let mut decisions: Option<OsString> = None;
    let mut keep: Option<KeepMode> = None;
    let mut dest: Option<OsString> = None;
    let mut raw_follows: Option<bool> = None;
    let mut no_subfolders = false;
    let mut json = false;
    let mut options_done = false;
    let mut args = rest.into_iter();
    let twice = |arg: &OsString| usage(Topic::Plan, format!("{} given twice", quoted(arg)));
    while let Some(arg) = args.next() {
        let is_option = !options_done && arg.to_str().is_some_and(|s| s.starts_with('-'));
        if !is_option {
            if folder.is_some() {
                return Err(usage(
                    Topic::Plan,
                    format!("unexpected argument {}", quoted(&arg)),
                ));
            }
            if arg.is_empty() {
                return Err(usage(Topic::Plan, "the folder is empty"));
            }
            folder = Some(arg);
            continue;
        }
        let mut value = |what: &str| -> Result<OsString, UsageError> {
            match args.next() {
                Some(v) if !v.is_empty() => Ok(v),
                Some(_) => Err(usage(Topic::Plan, format!("{} is empty", quoted(&arg)))),
                None => Err(usage(Topic::Plan, format!("{} needs {what}", quoted(&arg)))),
            }
        };
        match arg.to_str() {
            Some("--") => options_done = true,
            Some("--help" | "-h") => return Ok(Command::Help(Topic::Plan)),
            Some("--no-subfolders") => {
                if no_subfolders {
                    return Err(twice(&arg));
                }
                no_subfolders = true;
            }
            Some("--json") => {
                if json {
                    return Err(twice(&arg));
                }
                json = true;
            }
            Some("--decisions") => {
                let v = value("a file")?;
                if decisions.replace(v).is_some() {
                    return Err(twice(&arg));
                }
            }
            Some("--dest") => {
                let v = value("a folder")?;
                if dest.replace(v).is_some() {
                    return Err(twice(&arg));
                }
            }
            Some("--keep") => {
                let v = value("move or copy")?;
                let mode = match v.to_str() {
                    Some("move") => KeepMode::Move,
                    Some("copy") => KeepMode::Copy,
                    _ => {
                        return Err(usage(
                            Topic::Plan,
                            format!("--keep takes move or copy, not {}", quoted(&v)),
                        ));
                    }
                };
                if keep.replace(mode).is_some() {
                    return Err(twice(&arg));
                }
            }
            Some("--raw-follows") => {
                let v = value("yes or no")?;
                let on = match v.to_str() {
                    Some("yes") => true,
                    Some("no") => false,
                    _ => {
                        return Err(usage(
                            Topic::Plan,
                            format!("--raw-follows takes yes or no, not {}", quoted(&v)),
                        ));
                    }
                };
                if raw_follows.replace(on).is_some() {
                    return Err(twice(&arg));
                }
            }
            _ => {
                return Err(usage(
                    Topic::Plan,
                    format!("unknown option {}", quoted(&arg)),
                ));
            }
        }
    }
    let Some(folder) = folder else {
        return Err(usage(Topic::Plan, "no folder given"));
    };
    let Some(decisions) = decisions else {
        return Err(usage(
            Topic::Plan,
            "no decision log given (--decisions <log-file>)",
        ));
    };
    let keep = match (keep, dest) {
        (Some(mode), Some(dest)) => Some((mode, PathBuf::from(dest))),
        (None, None) => None,
        (Some(_), None) => {
            return Err(usage(Topic::Plan, "--keep needs --dest <folder>"));
        }
        (None, Some(_)) => {
            return Err(usage(
                Topic::Plan,
                "--dest needs --keep move or --keep copy",
            ));
        }
    };
    Ok(Command::Plan(PlanArgs {
        folder: PathBuf::from(folder),
        decisions: PathBuf::from(decisions),
        keep,
        raw_follows: raw_follows.unwrap_or(true),
        include_subfolders: !no_subfolders,
        json,
    }))
}

/// An argument for an error message, quoted and with control characters escaped.
fn quoted(arg: &OsString) -> String {
    format!("'{}'", crate::text::shown(&arg.to_string_lossy()))
}

/// Help text for a topic.
#[must_use]
pub(crate) fn help(topic: Topic) -> &'static str {
    match topic {
        Topic::Overview => OVERVIEW,
        Topic::Scan => SCAN,
        Topic::Preview => PREVIEW,
        Topic::Plan => PLAN,
    }
}

const OVERVIEW: &str = "\
Culla, headless. Reads only: nothing is written anywhere but stdout and stderr.

Usage:
  culla scan <folder> [--no-subfolders] [--json]
  culla preview <folder> [--no-subfolders]
  culla plan <folder> --decisions <log-file> [--keep move|copy --dest <folder>]
             [--raw-follows yes|no] [--no-subfolders] [--json]
  culla help [scan|preview|plan]
  culla --version

Commands:
  scan       list the photos of a folder, grouped and in capture-time order
  preview    build the screen preview of every photo of a folder and time each step
  plan       show what applying a decision log would do, and what preflight finds
             (a dry run: nothing is moved, copied or written)
  help       show this help, or the help of a command

Exit codes:
  0  done
  1  usage error
  2  the folder is refused (a link, a cloud placeholder, not a folder, too large,
     a path or volume Culla does not support)
  3  the folder, or the decision log, cannot be read
  4  the decision log is not valid (plan)
  5  no plan can be built: kept photos but no destination (plan)
";

const SCAN: &str = "\
Usage: culla scan <folder> [--no-subfolders] [--json]

Lists the folder (and its subfolders), groups each JPEG with its NEF and sidecars,
reads the EXIF of each group's primary and prints the groups in capture-time order,
with the volume identity, the source key and the totals. Progress goes to stderr.
Reads only: no cache, no session, nothing is written.

Options:
  --no-subfolders  scan the folder itself only (its NKSC_PARAM is still read)
  --json           print one JSON object instead of text
  -h, --help       show this help
  --               treat what follows as the folder, even if it starts with '-'

Off Windows the volume cannot be looked up: the folder is scanned all the same and the
identity is reported as unavailable.

The JSON fields are unstable until M2 and may change: the object says so with
\"schema\": \"unstable\".

Exit codes: 0 done, 1 usage error, 2 folder refused, 3 folder unreadable.
";

const PREVIEW: &str = "\
Usage: culla preview <folder> [--no-subfolders]

Scans the folder like culla scan, then builds the screen preview of each group in
capture-time order (the camera JPEG, or a lone NEF's embedded JPEG; 2880x1800 box,
upright) on the preview pipeline: reader thread, decode workers, prefetch of 5 ahead
and 2 behind, in-memory cache. Prints the time of each step per photo and the totals.
Progress goes to stderr. A photo that fails is listed and the others go on.
Reads only: the previews are decoded, timed and dropped; nothing is written.

Options:
  --no-subfolders  scan the folder itself only
  -h, --help       show this help
  --               treat what follows as the folder, even if it starts with '-'

The output is unstable until M2 and may change.

Exit codes: 0 done (failed previews are listed), 1 usage error, 2 folder refused,
3 folder unreadable.
";

const PLAN: &str = "\
Usage: culla plan <folder> --decisions <log-file> [--keep move|copy --dest <folder>]
                  [--raw-follows yes|no] [--no-subfolders] [--json]

A dry run of apply. Scans the folder like culla scan, reads the decision log, folds it,
builds the plan (rejects into <folder>/_rejected, kept photos into the destination, with
collision suffixes), runs preflight on it and prints the operations, the groups held
back, the preflight findings and the totals. Progress goes to stderr.

Reads only: nothing is moved, copied, created or written, not even a session; the
destination and _rejected are only looked at.

The decision log is the core text form, one JSON event per line, for example
  {\"seq\":1,\"ts\":0,\"cause\":\"user\",\"type\":\"decided\",\"group_id\":3,\"verdict\":\"keep\"}
A group id is the position culla scan prints for the same folder and options
(1 is the first photo in capture-time order). Decisions for ids the scan does not have
are listed and ignored. A destination inside the folder is scanned and numbered like
culla scan does, but its photos are never planned: decisions on them are listed and
ignored.

Options:
  --decisions <file>    the decision log (required; read only, at most 64 MiB)
  --keep move|copy      keep by move or by copy (needs --dest)
  --dest <folder>       where kept photos go (needs --keep); without both, only
                        rejects can be planned
  --raw-follows yes|no  the NEF goes with its JPEG (default yes)
  --no-subfolders       scan the folder itself only
  --json                print one JSON object instead of text
  -h, --help            show this help
  --                    treat what follows as the folder, even if it starts with '-'

Some facts the dry run does not look up are treated as unknown and fail closed: free
space (copies and cross-volume moves then show a blocker), whether the destination is in
a cloud-synced folder (a warning) and, off Windows, the volumes (the volume checks then
block). Others are not checked at all: locks held by other programs (no lock warning is
ever shown) and, on Windows, a second mounted volume with the same serial as the source
or the destination (only those two are looked up, by path). The output lists both apart.

The output is unstable until M2 and may change: the JSON says so with
\"schema\": \"unstable\".

Exit codes: 0 done (whatever preflight finds), 1 usage error, 2 folder refused,
3 folder or decision log unreadable, 4 decision log not valid, 5 kept photos but no
destination.
";

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Result<Command, UsageError> {
        parse(args.iter().map(OsString::from))
    }

    fn scan(folder: &str, include_subfolders: bool, json: bool) -> Command {
        Command::Scan(ScanArgs {
            folder: PathBuf::from(folder),
            include_subfolders,
            json,
        })
    }

    #[test]
    fn scan_with_options_in_any_order() {
        assert_eq!(p(&["scan", "E:\\DCIM"]), Ok(scan("E:\\DCIM", true, false)));
        assert_eq!(
            p(&["scan", "--json", "d", "--no-subfolders"]),
            Ok(scan("d", false, true))
        );
        assert_eq!(p(&["scan", "--", "-d"]), Ok(scan("-d", true, false)));
        assert_eq!(
            p(&["scan", "d", "--", "--json"]).unwrap_err().topic,
            Topic::Scan
        );
    }

    #[test]
    fn preview_with_options() {
        let pv = |folder: &str, include_subfolders| {
            Command::Preview(PreviewArgs {
                folder: PathBuf::from(folder),
                include_subfolders,
            })
        };
        assert_eq!(p(&["preview", "d"]), Ok(pv("d", true)));
        assert_eq!(p(&["preview", "--no-subfolders", "d"]), Ok(pv("d", false)));
        assert_eq!(p(&["preview", "--", "-d"]), Ok(pv("-d", true)));
        assert_eq!(p(&["help", "preview"]), Ok(Command::Help(Topic::Preview)));
        assert_eq!(p(&["preview", "-h"]), Ok(Command::Help(Topic::Preview)));
    }

    #[test]
    fn plan_with_options_in_any_order() {
        let got = p(&[
            "plan",
            "--json",
            "--keep",
            "copy",
            "E:\\DCIM",
            "--dest",
            "-K",
            "--decisions",
            "log.jsonl",
            "--raw-follows",
            "no",
            "--no-subfolders",
        ]);
        assert_eq!(
            got,
            Ok(Command::Plan(PlanArgs {
                folder: PathBuf::from("E:\\DCIM"),
                decisions: PathBuf::from("log.jsonl"),
                keep: Some((KeepMode::Copy, PathBuf::from("-K"))),
                raw_follows: false,
                include_subfolders: false,
                json: true,
            }))
        );
        assert_eq!(
            p(&["plan", "--decisions", "l", "--", "-d"]),
            Ok(Command::Plan(PlanArgs {
                folder: PathBuf::from("-d"),
                decisions: PathBuf::from("l"),
                keep: None,
                raw_follows: true,
                include_subfolders: true,
                json: false,
            }))
        );
        assert_eq!(p(&["help", "plan"]), Ok(Command::Help(Topic::Plan)));
        assert_eq!(p(&["plan", "-h"]), Ok(Command::Help(Topic::Plan)));
    }

    #[test]
    fn help_and_version() {
        for h in [&["help"][..], &["--help"], &["-h"]] {
            assert_eq!(p(h), Ok(Command::Help(Topic::Overview)));
        }
        assert_eq!(p(&["help", "scan"]), Ok(Command::Help(Topic::Scan)));
        assert_eq!(p(&["scan", "--help"]), Ok(Command::Help(Topic::Scan)));
        assert_eq!(p(&["scan", "d", "-h"]), Ok(Command::Help(Topic::Scan)));
        assert_eq!(p(&["--version"]), Ok(Command::Version));
        assert_eq!(p(&["-V"]), Ok(Command::Version));
    }

    #[test]
    fn usage_errors() {
        let msg = |a: &[&str]| p(a).unwrap_err().message;
        assert_eq!(msg(&[]), "no command given");
        assert_eq!(msg(&["sacn"]), "unknown command 'sacn'");
        assert_eq!(msg(&["scan"]), "no folder given");
        assert_eq!(msg(&["scan", ""]), "the folder is empty");
        assert_eq!(msg(&["scan", "a", "b"]), "unexpected argument 'b'");
        assert_eq!(msg(&["scan", "-x", "a"]), "unknown option '-x'");
        assert_eq!(
            msg(&["scan", "a", "--json", "--json"]),
            "'--json' given twice"
        );
        assert_eq!(msg(&["help", "apply"]), "no help for 'apply'");
        assert_eq!(
            msg(&["plan", "d"]),
            "no decision log given (--decisions <log-file>)"
        );
        assert_eq!(msg(&["plan", "--decisions", "l"]), "no folder given");
        assert_eq!(
            msg(&["plan", "d", "--decisions"]),
            "'--decisions' needs a file"
        );
        assert_eq!(
            msg(&["plan", "d", "--decisions", ""]),
            "'--decisions' is empty"
        );
        assert_eq!(
            msg(&["plan", "d", "--decisions", "l", "--keep", "move"]),
            "--keep needs --dest <folder>"
        );
        assert_eq!(
            msg(&["plan", "d", "--decisions", "l", "--dest", "x"]),
            "--dest needs --keep move or --keep copy"
        );
        assert_eq!(
            msg(&["plan", "d", "--decisions", "l", "--keep", "mv"]),
            "--keep takes move or copy, not 'mv'"
        );
        assert_eq!(
            msg(&["plan", "d", "--decisions", "l", "--raw-follows", "on"]),
            "--raw-follows takes yes or no, not 'on'"
        );
        assert_eq!(
            msg(&["plan", "d", "--decisions", "l", "--decisions", "m"]),
            "'--decisions' given twice"
        );
        assert_eq!(
            msg(&["plan", "d", "--decisions", "l", "--out", "o"]),
            "unknown option '--out'"
        );
        assert_eq!(msg(&["preview"]), "no folder given");
        assert_eq!(
            msg(&["preview", "a", "--out", "o"]),
            "unknown option '--out'"
        );
        assert_eq!(msg(&["preview", "a", "--json"]), "unknown option '--json'");
        assert_eq!(msg(&["preview", "a", "b"]), "unexpected argument 'b'");
        assert_eq!(
            p(&["preview", "a", "--no-subfolders", "--no-subfolders"])
                .unwrap_err()
                .topic,
            Topic::Preview
        );
        assert_eq!(msg(&["help", "scan", "x"]), "unexpected argument 'x'");
        assert_eq!(msg(&["--version", "x"]), "unexpected argument 'x'");
        assert_eq!(msg(&["scan\u{1b}[2J"]), "unknown command 'scan\\u{1b}[2J'");
    }

    #[cfg(unix)]
    #[test]
    fn a_non_unicode_folder_is_kept_as_given() {
        use std::os::unix::ffi::OsStringExt;
        let raw = OsString::from_vec(vec![b'f', 0xFF]);
        let got = parse([OsString::from("scan"), raw.clone()]).unwrap();
        assert_eq!(
            got,
            Command::Scan(ScanArgs {
                folder: PathBuf::from(raw),
                include_subfolders: true,
                json: false,
            })
        );
    }
}
