//! Stamps a development build with its source (tools/stamp.sh, or
//! ENVIO_GIT when the caller ran it) and the time. Runs again only when the
//! sources or the checkout change, so an unchanged rebuild compiles
//! nothing. ENVIO_RELEASE=1, an official release, stamps nothing.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// What the app is built from, relative to the repo root.
const SOURCES: [&str; 11] = [
    "crates",
    "qt/src",
    "qt/main.cpp",
    "qt/build.rs",
    "qt/Cargo.toml",
    "qt/stamp/src",
    "qml",
    "assets",
    "CMakeLists.txt",
    "manifest.json",
    "Cargo.toml",
];

fn main() {
    for var in [
        "ENVIO_RELEASE",
        "ENVIO_GIT",
        "SOURCE_DATE_EPOCH",
        "GITHUB_HEAD_REF",
    ] {
        println!("cargo:rerun-if-env-changed={var}");
    }
    if std::env::var_os("ENVIO_RELEASE").is_some_and(|v| v == "1") {
        return;
    }
    let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    for s in SOURCES {
        println!("cargo:rerun-if-changed={}", root.join(s).display());
    }
    let source = match std::env::var("ENVIO_GIT") {
        Ok(s) if !s.is_empty() => s,
        _ => {
            watch_checkout(&root);
            run(&root, "sh", &["tools/stamp.sh"]).unwrap_or_else(|| "unknown".into())
        }
    };
    let built = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs())
                .to_string()
        });
    println!("cargo:rustc-env=ENVIO_STAMP_SOURCE={source}");
    println!("cargo:rustc-env=ENVIO_STAMP_BUILT={built}");
}

/// A commit, a branch switch or `git add` changes one of these. Branch
/// refs are watched by their folder, which cargo scans: a packed branch
/// has no ref file until the next commit writes one. Only existing paths,
/// as cargo reruns on every build for a missing one.
fn watch_checkout(root: &Path) {
    for p in ["HEAD", "index", "packed-refs", "refs/heads"] {
        let args = [
            "-c",
            "safe.directory=*",
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            p,
        ];
        if let Some(path) = run(root, "git", &args).filter(|p| Path::new(p).exists()) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
}

/// The command's trimmed output, if it ran and succeeded.
fn run(root: &Path, cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd)
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}
