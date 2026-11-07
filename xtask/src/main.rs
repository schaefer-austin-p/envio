//! `cargo xtask affected --base <git ref>`: prints the `cargo test`
//! arguments for the crates changed since `<ref>` (see affected.rs).

mod affected;
mod metadata;

use std::path::PathBuf;
use std::process::{Command, ExitCode};

/// Crates with their own CI step, never in the printed arguments.
const ALWAYS_RUN: &[&str] = &["envio-e2e"];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [cmd, flag, base] if cmd == "affected" && flag == "--base" => run_affected(base),
        _ => Err("usage: cargo xtask affected --base <git ref>".to_owned()),
    };
    match result {
        Ok(line) => {
            println!("{line}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_affected(base: &str) -> Result<String, String> {
    let meta = cargo_metadata::MetadataCommand::new()
        .no_deps()
        .exec()
        .map_err(|e| e.to_string())?;
    let out = Command::new("git")
        .args(["diff", "--name-only", &format!("{base}...HEAD")])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    let changed: Vec<PathBuf> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(PathBuf::from)
        .collect();
    let selection = affected::select(&metadata::packages(&meta), &changed);
    Ok(affected::cargo_args(&selection, ALWAYS_RUN))
}
