use std::env;
use std::path::Path;
use std::process::Command;

use cxx_qt_build::{CxxQtBuilder, QmlModule};

/// The commit for the About text: `ENVIO_COMMIT` if set (clickable passes
/// it into its container), else git's HEAD, else empty (BuildInfo then
/// says "unknown").
fn build_commit() -> String {
    println!("cargo:rerun-if-env-changed=ENVIO_COMMIT");
    if let Ok(commit) = env::var("ENVIO_COMMIT")
        && !commit.is_empty()
    {
        return commit;
    }
    // Rebuild when HEAD moves: a checkout changes HEAD, a commit changes the
    // branch's ref, loose or packed (--git-path finds them in a worktree's
    // common dir too). Only existing files are watched: cargo treats a
    // missing one as always changed.
    let mut watched = vec!["HEAD".to_owned(), "packed-refs".to_owned()];
    let branch = git(&["symbolic-ref", "-q", "HEAD"]);
    if !branch.is_empty() {
        watched.push(branch);
    }
    for path in watched {
        let resolved = git(&["rev-parse", "--git-path", &path]);
        if !resolved.is_empty() && Path::new(&resolved).exists() {
            println!("cargo:rerun-if-changed={resolved}");
        }
    }
    git(&["rev-parse", "--short=10", "HEAD"])
}

/// Runs git, returning its trimmed stdout, or empty on any failure.
fn git(args: &[&str]) -> String {
    // the container's user doesn't own the mounted checkout
    Command::new("git")
        .args(["-c", "safe.directory=*"])
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|c| c.trim().to_owned())
        .unwrap_or_default()
}

fn main() {
    println!("cargo:rustc-env=ENVIO_COMMIT={}", build_commit());
    println!("cargo:rerun-if-changed=../manifest.json");
    CxxQtBuilder::new_qml_module(QmlModule::new("Envio").version(1, 0))
        .files(["src/build_info.rs"])
        .build();
}
