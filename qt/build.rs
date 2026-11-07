use std::env;
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
    // the container's user doesn't own the mounted checkout
    Command::new("git")
        .args(["-c", "safe.directory=*", "rev-parse", "--short=10", "HEAD"])
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
