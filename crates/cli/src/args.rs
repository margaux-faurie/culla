//! Command line parsing, std only: `culla scan <folder> [--no-subfolders] [--json]`,
//! `culla help [scan]`, `culla --version`.

use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

/// What to run.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// Print help: the overview, or one command's.
    Help(Topic),
    /// Print the version.
    Version,
    /// Scan a folder.
    Scan(ScanArgs),
}

/// A help page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Topic {
    /// Every command.
    Overview,
    /// `culla scan`.
    Scan,
}

/// The arguments of `culla scan`.
#[derive(Debug, PartialEq, Eq)]
pub struct ScanArgs {
    /// The folder to scan, as given.
    pub folder: PathBuf,
    /// Scan subfolders too (default on, off with `--no-subfolders`).
    pub include_subfolders: bool,
    /// Print JSON instead of text.
    pub json: bool,
}

/// The command line is not valid. The text says why; usage follows it on stderr.
#[derive(Debug, PartialEq, Eq)]
pub struct UsageError {
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
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, UsageError> {
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

/// An argument for an error message, quoted and with control characters escaped.
fn quoted(arg: &OsString) -> String {
    format!("'{}'", crate::text::shown(&arg.to_string_lossy()))
}

/// Help text for a topic.
#[must_use]
pub fn help(topic: Topic) -> &'static str {
    match topic {
        Topic::Overview => OVERVIEW,
        Topic::Scan => SCAN,
    }
}

const OVERVIEW: &str = "\
Culla, headless. Reads only: nothing is written anywhere.

Usage:
  culla scan <folder> [--no-subfolders] [--json]
  culla help [scan]
  culla --version

Commands:
  scan       list the photos of a folder, grouped and in capture-time order
  help       show this help, or the help of a command

Exit codes:
  0  done
  1  usage error
  2  the folder is refused (a link, a cloud placeholder, not a folder, too large,
     a path or volume Culla does not support)
  3  the folder cannot be read
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
