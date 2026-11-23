//! `cargo xtask app-qrc [dir]`: fails when `qml/qml.qrc` misses a QML, JS or
//! qmldir file of the app under `qml/` (bindle has its own check, see
//! bindle_qrc.rs). The phone build loads the app from qrc, so an unlisted
//! file passes the QML tests (they read from disk) and fails only at start:
//! "PullIndicator is not a type" (#125). Files under `qml/bindle/` belong to
//! bindle.qrc and are skipped. Nothing else is exempt: the qrc lists every
//! `qml/platform/` file too (CMake picks by import, not by file list).

use std::path::Path;

use crate::bindle_qrc::{parse_with, walk};

/// The prefix the app's qresource uses; the qrc's other resource
/// (`/third_party`, fonts) is not QML and is not checked.
const QRC_PREFIX: &str = "/qml";

/// A file the qrc must list: QML, JS or qmldir, outside bindle.
fn is_app_file(rel: &str) -> bool {
    if rel == "bindle" || rel.starts_with("bindle/") {
        return false;
    }
    let name = rel.rsplit('/').next().unwrap_or(rel);
    name == "qmldir" || name.ends_with(".qml") || name.ends_with(".js")
}

/// What is wrong with how `qrc` lists the files in `files` (relative to
/// `qml/`, `/` separated): a malformed qrc or alias first, then each
/// unlisted file, sorted.
pub fn problems(files: &[String], qrc: &str) -> Vec<String> {
    let listing = parse_with(qrc, QRC_PREFIX, "qml.qrc");
    let mut out: Vec<String> = listing
        .problems
        .into_iter()
        .filter(|p| !p.contains("\"/third_party\""))
        .collect();
    let mut missing: Vec<&str> = files
        .iter()
        .map(String::as_str)
        .filter(|f| is_app_file(f) && !listing.files.contains(*f))
        .collect();
    missing.sort_unstable();
    missing.dedup();
    out.extend(
        missing
            .iter()
            .map(|f| format!("qml.qrc: does not list {f}")),
    );
    out
}

/// Checks the `qml` directory `dir`; the error lists what is missing.
pub fn check(dir: &Path) -> Result<(), String> {
    let qrc_path = dir.join("qml.qrc");
    let qrc =
        std::fs::read_to_string(&qrc_path).map_err(|e| format!("{}: {e}", qrc_path.display()))?;
    let mut files = Vec::new();
    walk(dir, Path::new(""), &mut files).map_err(|e| format!("{}: {e}", dir.display()))?;
    // An empty tree is a wrong directory, never a pass.
    if !files.iter().any(|f| is_app_file(f)) {
        return Err(format!("no QML files under {}", dir.display()));
    }
    let problems = problems(&files, &qrc);
    if problems.is_empty() {
        return Ok(());
    }
    Err(problems.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const QRC: &str = r#"<RCC>
      <qresource prefix="/qml">
        <file>Main.qml</file>
        <file>pages/inbox/MessageList.qml</file>
        <file>platform/qmldir</file>
      </qresource>
      <qresource prefix="/third_party">
        <file alias="fonts/a.ttf">../third_party/fonts/a.ttf</file>
      </qresource></RCC>"#;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_owned()).collect()
    }

    fn unlisted(path: &str) -> String {
        format!("qml.qrc: does not list {path}")
    }

    #[test]
    fn all_listed_is_clean() {
        let files = s(&["Main.qml", "pages/inbox/MessageList.qml", "platform/qmldir"]);
        assert_eq!(problems(&files, QRC), Vec::<String>::new());
    }

    #[test]
    fn an_unlisted_page_is_reported() {
        let files = s(&["Main.qml", "pages/inbox/PullIndicator.qml", "platform/x.js"]);
        assert_eq!(
            problems(&files, QRC),
            [
                unlisted("pages/inbox/PullIndicator.qml"),
                unlisted("platform/x.js")
            ]
        );
    }

    #[test]
    fn bindle_and_other_files_are_ignored() {
        let files = s(&[
            "bindle/Platform/Uri.qml",
            "bindle/qmldir",
            "README.md",
            "pages/a.txt",
        ]);
        assert!(problems(&files, QRC).is_empty());
    }

    #[test]
    fn a_name_that_only_starts_with_bindle_is_an_app_file() {
        let files = s(&["bindleish/A.qml"]);
        assert_eq!(problems(&files, QRC), [unlisted("bindleish/A.qml")]);
    }

    #[test]
    fn a_prefix_match_is_not_a_listing() {
        let files = s(&["Main.qml.bak.qml", "Mai.qml"]);
        assert_eq!(
            problems(&files, QRC),
            [unlisted("Mai.qml"), unlisted("Main.qml.bak.qml")]
        );
    }

    #[test]
    fn a_commented_out_entry_is_not_listed() {
        let qrc = r#"<RCC><qresource prefix="/qml"><!-- <file>A.qml</file> --></qresource></RCC>"#;
        assert_eq!(problems(&s(&["A.qml"]), qrc), [unlisted("A.qml")]);
    }

    #[test]
    fn a_file_in_another_resource_is_not_a_qml_listing() {
        let qrc = r#"<RCC><qresource prefix="/third_party"><file>A.qml</file></qresource></RCC>"#;
        assert_eq!(problems(&s(&["A.qml"]), qrc), [unlisted("A.qml")]);
    }

    #[test]
    fn malformed_xml_is_a_problem() {
        assert!(!problems(&s(&[]), "<RCC><qresource></RCC>").is_empty());
    }

    #[test]
    fn check_reports_a_missing_file_in_a_directory() {
        let tmp = std::env::temp_dir().join(format!("xtask-appqrc-{}", std::process::id()));
        std::fs::create_dir_all(tmp.join("pages")).unwrap();
        std::fs::write(tmp.join("qml.qrc"), QRC).unwrap();
        std::fs::write(tmp.join("Main.qml"), "").unwrap();
        assert!(check(&tmp).is_ok());
        std::fs::write(tmp.join("pages/New.qml"), "").unwrap();
        let err = check(&tmp).unwrap_err();
        assert!(err.contains("pages/New.qml"), "{err}");
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn an_empty_directory_fails() {
        let tmp = std::env::temp_dir().join(format!("xtask-appqrc-empty-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("qml.qrc"), QRC).unwrap();
        assert!(check(&tmp).is_err());
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
