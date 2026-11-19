//! `cargo xtask affected --base <git ref>`: prints the `cargo test`
//! arguments for the crates changed since `<ref>` (see affected.rs).
//! `cargo xtask bindle-qrc [dir]`: fails when bindle.qrc misses a bindle QML
//! file (see bindle_qrc.rs).

mod affected;
mod bindle_qrc;
mod metadata;

use std::path::PathBuf;
use std::process::{Command, ExitCode};

/// Crates with their own CI step, never in the printed arguments.
const ALWAYS_RUN: &[&str] = &["envio-e2e"];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [cmd, flag, base] if cmd == "affected" && flag == "--base" => run_affected(base),
        [cmd] if cmd == "bindle-qrc" => run_bindle_qrc(None),
        [cmd, dir] if cmd == "bindle-qrc" => run_bindle_qrc(Some(dir)),
        _ => Err(
            "usage: cargo xtask affected --base <git ref> | cargo xtask bindle-qrc [dir]"
                .to_owned(),
        ),
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

fn run_bindle_qrc(dir: Option<&String>) -> Result<String, String> {
    let dir = dir.map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../qml/bindle"),
        PathBuf::from,
    );
    bindle_qrc::check(&dir)?;
    Ok("bindle.qrc lists every bindle QML file".to_owned())
}

fn run_affected(base: &str) -> Result<String, String> {
    let meta = cargo_metadata::MetadataCommand::new()
        .no_deps()
        .exec()
        .map_err(|e| e.to_string())?;
    let out = Command::new("git")
        .args([
            "diff",
            "--name-only",
            "--no-renames",
            &format!("{base}...HEAD"),
        ])
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
