/* Copyright (C) 2018 Olivier Goffart <ogoffart@woboq.com>
Permission is hereby granted, free of charge, to any person obtaining a copy of this software and
associated documentation files (the "Software"), to deal in the Software without restriction,
including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense,
and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so,
subject to the following conditions:
The above copyright notice and this permission notice shall be included in all copies or substantial
portions of the Software.
THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT
NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES
OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
*/
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use walkdir::{DirEntry, WalkDir};

/// Perform qmake query
fn qmake_query(qmake: &str, args: &str, var: &str) -> String {
    let mut qmake_cmd_list: Vec<&str> = qmake.split(' ').collect();
    qmake_cmd_list.push("-query");
    qmake_cmd_list.push(var);

    if !args.is_empty() {
        qmake_cmd_list.append(&mut args.split(' ').collect());
    }

    String::from_utf8(
        Command::new(qmake_cmd_list[0])
            .args(&qmake_cmd_list[1..])
            .output()
            .expect("Failed to execute qmake. Make sure 'qmake' is in your path")
            .stdout,
    )
        .expect("UTF-8 conversion failed")
}

/// Generates the shell command to call for qmake
fn qmake_call() -> String {
    env::var("QMAKE").unwrap_or(String::from("qmake"))
}

/// Generates the arguments for the call to qmake
fn qmake_args() -> String {
    env::var("QMAKE_ARGS").unwrap_or_default()
}

/// Generate gettext translation files
fn update_language_files() {
    let pot_file = "po/envio.aps.pot";
    let source_files = source_files();

    let mut child = Command::new("xgettext")
        .args([
            &format!("--output={}", pot_file),
            "--language=javascript",
            "--qt",
            "--keyword=tr",
            "--keyword=tr:1,2",
            "--add-comments=i18n",
            "--from-code=UTF-8",
        ])
        .args(&source_files)
        .spawn()
        .unwrap();

    let exit_status = child.wait().unwrap();
    assert_eq!(exit_status.code(), Some(0));

    for po_file in po_files() {
        let po_file_name = po_file
            .to_str()
            .expect("po language file name contains invalid characters");

        let mut child = Command::new("msgmerge")
            .args(["--update", po_file_name, pot_file])
            .spawn()
            .unwrap();

        let exit_status = child.wait().unwrap();
        assert_eq!(exit_status.code(), Some(0));

        let install_dir = env::var("INSTALL_DIR").expect("No env var INSTALL_DIR provided");
        let lang = po_file.file_stem().unwrap().to_str().unwrap();
        let mo_dir = format!("{install_dir}/share/locale/{lang}/LC_MESSAGES");
        let mo_file = format!("{}/envio.aps.mo", mo_dir);

        fs::create_dir_all(&mo_dir)
            .expect("Failed to create directory for compiled language files");

        let mut child = Command::new("msgfmt")
            .args([po_file_name, "-o", &mo_file])
            .spawn()
            .unwrap();

        let exit_status = child.wait().unwrap();
        assert_eq!(exit_status.code(), Some(0));
    }
}

/// Obtains a list of all QML files
fn source_files() -> Vec<PathBuf> {
    // Directory in which to search for QML files and file extension
    walk_dir(PathBuf::from("qml"), "qml")
}

/// Obtains a list of all translation files
fn po_files() -> Vec<PathBuf> {
    // Directory in which to search for translation files
    walk_dir(PathBuf::from("po"), "po")
}

/// Recursively searches for files in a directory and
/// returns a list of paths to the files, sorted by name so that the
/// generated template is the same on every machine
fn walk_dir(dir: PathBuf, ext: &str) -> Vec<PathBuf> {
    WalkDir::new(dir)
        .sort_by_file_name()
        .into_iter()
        .map(|walked| walked.expect("Failed to iterate over directory"))
        .filter(|directory_entry| {
            directory_entry.file_type().is_file()
                && directory_entry.path().extension() == Some(OsStr::new(ext))
        })
        .map(DirEntry::into_path)
        .collect()
}

/// The commit this is built from, for the About dialog: `ENVIO_COMMIT` if
/// set, else git's HEAD, else "unknown" (no git, or a worktree whose git
/// directory clickable's container doesn't see)
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
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|commit| commit.trim().to_owned())
        .filter(|commit| !commit.is_empty())
        .unwrap_or_else(|| {
            println!("cargo:warning=no git commit found; the About dialog will say \"unknown\"");
            "unknown".to_owned()
        })
}

/// The About dialog shows Cargo.toml's version, the phone and the store
/// manifest.json's; a release that bumps only one of them fails here
fn check_manifest_version() {
    println!("cargo:rerun-if-changed=manifest.json");
    let manifest = fs::read_to_string("manifest.json").expect("Failed to read manifest.json");
    let cargo_version = env::var("CARGO_PKG_VERSION").expect("No env var CARGO_PKG_VERSION");
    // its only "version" key; no JSON parser among the build dependencies
    let manifest_version = manifest
        .split("\"version\"")
        .nth(1)
        .and_then(|rest| rest.trim_start().strip_prefix(':'))
        .and_then(|rest| rest.trim_start().strip_prefix('"'))
        .and_then(|rest| rest.split('"').next());
    assert_eq!(
        manifest_version,
        Some(cargo_version.as_str()),
        "manifest.json's version must be Cargo.toml's"
    );
}

fn main() {
    check_manifest_version();
    update_language_files();
    println!("cargo:rustc-env=ENVIO_COMMIT={}", build_commit());

    let qmake_cmd = qmake_call();
    let args = qmake_args();

    let qt_include_path = qmake_query(&qmake_cmd, &args, "QT_INSTALL_HEADERS");
    let qt_library_path = qmake_query(&qmake_cmd, &args, "QT_INSTALL_LIBS");

    cpp_build::Config::new()
        .include(qt_include_path.trim())
        .build("src/main.rs");

    let macos_lib_search = if cfg!(target_os = "macos") {
        "=framework"
    } else {
        ""
    };
    let lib_framework = if cfg!(target_os = "macos") { "" } else { "5" };
    let qt_library_path = qt_library_path.trim();

    println!("cargo:rustc-link-search{macos_lib_search}={qt_library_path}");
    println!("cargo:rustc-link-lib{macos_lib_search}=Qt{lib_framework}Widgets");
    println!("cargo:rustc-link-lib{macos_lib_search}=Qt{lib_framework}Gui");
    println!("cargo:rustc-link-lib{macos_lib_search}=Qt{lib_framework}Core");
    println!("cargo:rustc-link-lib{macos_lib_search}=Qt{lib_framework}Quick");
    println!("cargo:rustc-link-lib{macos_lib_search}=Qt{lib_framework}Qml");
    println!("cargo:rustc-link-lib{macos_lib_search}=Qt{lib_framework}QuickControls2");

    // Hack to make the build script run unconditionally to ensure translations are generated
    println!("cargo:rerun-if-changed=REBUILD");
}