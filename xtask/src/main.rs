//! Culla dev tasks (docs/testing.md §11). Run with `cargo xtask <task>` once the alias is in
//! `.cargo/config.toml`, or `cargo run -p xtask -- <task>` meanwhile.
//!
//! Std only. xtask is outside the product crates but still inherits the workspace lints, so it
//! never truncates, overwrites, renames or removes a file: it only creates new files with
//! `create_new` inside the repository's `testdata/` folder.

mod fixtures;
mod golden;
mod json;

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
usage: cargo xtask <task>

tasks:
  fixtures              write the synthetic fixtures into testdata/synthetic/ (refuses to overwrite)
  fixtures --check      compare testdata/synthetic/ with what the generator produces, write nothing
  golden --check        check the golden decision-log fixtures in testdata/event-log/
  crash-loop --iterations N
                        kill and resume `culla apply` N times (Windows, from M2)";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let root = repo_root();
    let result = match args.as_slice() {
        ["fixtures"] => fixtures::write(&root),
        ["fixtures", "--check"] => fixtures::check(&root),
        ["golden", "--check"] => golden::check(&root),
        ["crash-loop", "--iterations", n] => crash_loop(n),
        ["help" | "--help" | "-h"] => {
            println!("{USAGE}");
            Ok(())
        }
        _ => Err(format!("unknown or incomplete task\n\n{USAGE}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("xtask: {message}");
            ExitCode::FAILURE
        }
    }
}

/// The workspace root: xtask lives one level below it.
fn repo_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map_or(manifest_dir.clone(), PathBuf::from)
}

fn crash_loop(iterations: &str) -> Result<(), String> {
    let n: u32 = iterations
        .parse()
        .map_err(|_| format!("--iterations needs a whole number, got {iterations:?}"))?;
    println!(
        "crash-loop: available from M2 (needs `culla apply` and `culla resume`); {n} iterations requested, nothing run"
    );
    Ok(())
}
