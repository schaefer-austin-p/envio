//! `cargo xtask bindle-qrc [dir]`: fails when a QML, JS or qmldir file of a
//! bindle tier exists in `qml/bindle` but `bindle.qrc` does not list it (#229).
//! The app loads bindle from qrc on the device, so an unlisted file builds
//! fine and fails only at runtime. rcc already fails on a listed file that
//! is missing; this is the other direction.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The tiers whose files bindle's own lint requires in the qrc.
const TIERS: &[&str] = &["Platform", "Behaviours", "Controls"];

/// A file the qrc must list: QML, JS or qmldir under a tier directory.
fn is_tier_file(rel: &str) -> bool {
    let Some((top, _)) = rel.split_once('/') else {
        return false;
    };
    let name = rel.rsplit('/').next().unwrap_or(rel);
    TIERS.contains(&top) && (name == "qmldir" || name.ends_with(".qml") || name.ends_with(".js"))
}

/// The paths named by the `<file>` elements of a qrc.
fn listed(qrc: &str) -> BTreeSet<&str> {
    let mut out = BTreeSet::new();
    let mut rest = qrc;
    while let Some(at) = rest.find("<file") {
        rest = &rest[at + "<file".len()..];
        // `<file>` or `<file alias="x">`, never `<filesomething>`.
        if !rest.starts_with(['>', ' ']) {
            continue;
        }
        let Some(open) = rest.find('>') else { break };
        rest = &rest[open + 1..];
        let Some(close) = rest.find("</file>") else {
            break;
        };
        out.insert(rest[..close].trim());
        rest = &rest[close..];
    }
    out
}

/// The tier files in `files` (paths relative to the bindle directory, `/`
/// separated) that `qrc` does not list, sorted.
pub fn missing<'a>(files: &'a [String], qrc: &str) -> Vec<&'a str> {
    let listed = listed(qrc);
    let mut out: Vec<&str> = files
        .iter()
        .map(String::as_str)
        .filter(|f| is_tier_file(f) && !listed.contains(f))
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Every file under `dir`, relative and `/` separated.
fn walk(dir: &Path, rel: &Path, out: &mut Vec<String>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir.join(rel))? {
        let entry = entry?;
        let name = entry.file_name();
        let path: PathBuf = rel.join(&name);
        if entry.file_type()?.is_dir() {
            if name != ".git" {
                walk(dir, &path, out)?;
            }
        } else {
            out.push(
                path.components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/"),
            );
        }
    }
    Ok(())
}

/// Checks the bindle directory `dir`; the error lists what is missing.
pub fn check(dir: &Path) -> Result<(), String> {
    let qrc_path = dir.join("bindle.qrc");
    let qrc = std::fs::read_to_string(&qrc_path)
        .map_err(|e| format!("{}: {e} (git submodule update --init?)", qrc_path.display()))?;
    let mut files = Vec::new();
    walk(dir, Path::new(""), &mut files).map_err(|e| format!("{}: {e}", dir.display()))?;
    // An empty tree is a missing submodule, never a pass.
    if !files.iter().any(|f| is_tier_file(f)) {
        return Err(format!("no tier files under {}", dir.display()));
    }
    let missing = missing(&files, &qrc);
    if missing.is_empty() {
        return Ok(());
    }
    let list: Vec<String> = missing.iter().map(|f| format!("  {f}")).collect();
    Err(format!(
        "bindle.qrc does not list {} file(s):\n{}",
        missing.len(),
        list.join("\n")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const QRC: &str = r#"<RCC><qresource prefix="/qml/bindle">
        <file>Platform/Uri.qml</file>
        <file>Platform/qmldir</file>
        <file alias="x">Behaviours/Hold.qml</file>
    </qresource></RCC>"#;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_owned()).collect()
    }

    #[test]
    fn all_listed_is_clean() {
        let files = s(&["Platform/Uri.qml", "Platform/qmldir", "Behaviours/Hold.qml"]);
        assert!(missing(&files, QRC).is_empty());
    }

    #[test]
    fn an_unlisted_file_is_reported() {
        let files = s(&[
            "Platform/Uri.qml",
            "Platform/New.qml",
            "Controls/a.js",
            "Controls/qmldir",
        ]);
        assert_eq!(
            missing(&files, QRC),
            ["Controls/a.js", "Controls/qmldir", "Platform/New.qml"]
        );
    }

    #[test]
    fn files_outside_the_tiers_are_ignored() {
        let files = s(&[
            "tests/tst_uri.qml",
            "tests/stubs/Lomiri/Content/qmldir",
            "README.md",
            "Platform/notes.md",
            "Top.qml",
        ]);
        assert!(missing(&files, QRC).is_empty());
    }

    #[test]
    fn a_prefix_match_is_not_a_listing() {
        let files = s(&["Platform/Uri.qml.bak.qml", "Platform/Ur.qml"]);
        assert_eq!(
            missing(&files, QRC),
            ["Platform/Ur.qml", "Platform/Uri.qml.bak.qml"]
        );
    }

    #[test]
    fn nested_tier_files_count() {
        let files = s(&["Platform/ut/LomiriTheme.qml"]);
        assert_eq!(missing(&files, QRC), ["Platform/ut/LomiriTheme.qml"]);
    }

    #[test]
    fn an_empty_qrc_misses_everything() {
        assert_eq!(
            missing(&s(&["Platform/Uri.qml"]), "<RCC/>"),
            ["Platform/Uri.qml"]
        );
    }

    #[test]
    fn check_reports_a_missing_file_in_a_directory() {
        let tmp = std::env::temp_dir().join(format!("xtask-qrc-{}", std::process::id()));
        std::fs::create_dir_all(tmp.join("Platform")).unwrap();
        std::fs::write(tmp.join("bindle.qrc"), QRC).unwrap();
        std::fs::write(tmp.join("Platform/Uri.qml"), "").unwrap();
        assert!(check(&tmp).is_ok());
        std::fs::write(tmp.join("Platform/New.qml"), "").unwrap();
        let err = check(&tmp).unwrap_err();
        assert!(err.contains("Platform/New.qml"), "{err}");
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn an_empty_directory_fails() {
        let tmp = std::env::temp_dir().join(format!("xtask-qrc-empty-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("bindle.qrc"), QRC).unwrap();
        assert!(check(&tmp).is_err());
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
