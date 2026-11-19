//! `cargo xtask bindle-qrc [dir]`: fails when a QML, JS or qmldir file of a
//! bindle tier exists in `qml/bindle` but `bindle.qrc` does not list it (#229).
//! The app loads bindle from qrc on the device, so an unlisted file builds
//! fine and fails only at runtime. rcc already fails on a listed file that
//! is missing; this is the other direction.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

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

/// The prefix bindle's tools/lint.py requires on every `<qresource>`.
const QRC_PREFIX: &str = "/qml/bindle";

/// What a qrc lists, and what is wrong with how it lists it.
struct Listing {
    files: BTreeSet<String>,
    problems: Vec<String>,
}

/// Reads the `<file>` elements inside `<qresource prefix="/qml/bindle">`.
/// Comments are not elements, so commented-out entries don't count. A
/// `<qresource>` with another prefix lists nothing (it is a problem), and
/// so does a `<file alias=...>` whose alias is not its path, since the app
/// would load it under another name.
fn parse(qrc: &str) -> Listing {
    let mut out = Listing {
        files: BTreeSet::new(),
        problems: Vec::new(),
    };
    let mut reader = Reader::from_str(qrc);
    let mut prefix_ok = false;
    let mut alias: Option<String> = None;
    let mut text: Option<String> = None;
    loop {
        match reader.read_event() {
            Err(e) => {
                out.problems.push(format!("bindle.qrc: {e}"));
                break;
            }
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) if e.local_name().as_ref() == "qresource" => {
                let prefix = attr(&e, "prefix");
                prefix_ok = prefix.as_deref() == Some(QRC_PREFIX);
                if !prefix_ok {
                    out.problems.push(format!(
                        "bindle.qrc: qresource prefix {prefix:?}, want {QRC_PREFIX:?}"
                    ));
                }
            }
            Ok(Event::End(e)) if e.local_name().as_ref() == "qresource" => prefix_ok = false,
            Ok(Event::Start(e)) if e.local_name().as_ref() == "file" => {
                alias = attr(&e, "alias");
                text = Some(String::new());
            }
            Ok(Event::Text(t)) => {
                if let Some(buf) = text.as_mut() {
                    buf.push_str(&t.xml10_content());
                }
            }
            Ok(Event::End(e)) if e.local_name().as_ref() == "file" => {
                let path = text.take().unwrap_or_default().trim().to_owned();
                match alias.take() {
                    Some(a) if a != path => out.problems.push(format!(
                        "bindle.qrc: {path} has alias {a:?}; the app loads it under that name"
                    )),
                    _ if prefix_ok => {
                        out.files.insert(path);
                    }
                    _ => {}
                }
            }
            Ok(_) => {}
        }
    }
    out
}

/// The value of attribute `name` on `e`.
fn attr(e: &BytesStart, name: &str) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == name)
        .and_then(|a| {
            a.normalized_value(XmlVersion::Implicit1_0)
                .ok()
                .map(|v| v.into_owned())
        })
}

/// What is wrong with how `qrc` lists the tier files in `files` (paths
/// relative to the bindle directory, `/` separated): a malformed qrc, a
/// wrong prefix, an alias, then each unlisted file, sorted.
pub fn problems(files: &[String], qrc: &str) -> Vec<String> {
    let listing = parse(qrc);
    let mut out = listing.problems;
    let mut missing: Vec<&str> = files
        .iter()
        .map(String::as_str)
        .filter(|f| is_tier_file(f) && !listing.files.contains(*f))
        .collect();
    missing.sort_unstable();
    missing.dedup();
    out.extend(
        missing
            .iter()
            .map(|f| format!("bindle.qrc: does not list {f}")),
    );
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
    let problems = problems(&files, &qrc);
    if problems.is_empty() {
        return Ok(());
    }
    Err(problems.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const QRC: &str = r#"<RCC><qresource prefix="/qml/bindle">
        <file>Platform/Uri.qml</file>
        <file>Platform/qmldir</file>
        <file alias="Behaviours/Hold.qml">Behaviours/Hold.qml</file>
    </qresource></RCC>"#;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_owned()).collect()
    }

    fn unlisted(path: &str) -> String {
        format!("bindle.qrc: does not list {path}")
    }

    #[test]
    fn all_listed_is_clean() {
        let files = s(&["Platform/Uri.qml", "Platform/qmldir", "Behaviours/Hold.qml"]);
        assert!(problems(&files, QRC).is_empty());
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
            problems(&files, QRC),
            [
                unlisted("Controls/a.js"),
                unlisted("Controls/qmldir"),
                unlisted("Platform/New.qml")
            ]
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
        assert!(problems(&files, QRC).is_empty());
    }

    #[test]
    fn a_prefix_match_is_not_a_listing() {
        let files = s(&["Platform/Uri.qml.bak.qml", "Platform/Ur.qml"]);
        assert_eq!(
            problems(&files, QRC),
            [
                unlisted("Platform/Ur.qml"),
                unlisted("Platform/Uri.qml.bak.qml")
            ]
        );
    }

    #[test]
    fn nested_tier_files_count() {
        let files = s(&["Platform/ut/LomiriTheme.qml"]);
        assert_eq!(
            problems(&files, QRC),
            [unlisted("Platform/ut/LomiriTheme.qml")]
        );
    }

    #[test]
    fn an_empty_qrc_misses_everything() {
        assert_eq!(
            problems(&s(&["Platform/Uri.qml"]), "<RCC/>"),
            [unlisted("Platform/Uri.qml")]
        );
    }

    #[test]
    fn a_commented_out_entry_is_not_listed() {
        let qrc = r#"<RCC><qresource prefix="/qml/bindle">
            <!-- <file>Platform/Uri.qml</file> -->
        </qresource></RCC>"#;
        assert_eq!(
            problems(&s(&["Platform/Uri.qml"]), qrc),
            [unlisted("Platform/Uri.qml")]
        );
    }

    #[test]
    fn a_wrong_prefix_is_a_problem_and_lists_nothing() {
        let qrc = r#"<RCC><qresource prefix="/qml/other">
            <file>Platform/Uri.qml</file>
        </qresource></RCC>"#;
        let got = problems(&s(&["Platform/Uri.qml"]), qrc);
        assert_eq!(got.len(), 2, "{got:?}");
        assert!(
            got[0].contains("prefix") && got[0].contains("/qml/other"),
            "{got:?}"
        );
        assert_eq!(got[1], unlisted("Platform/Uri.qml"));
    }

    #[test]
    fn a_missing_prefix_is_a_problem() {
        let qrc = "<RCC><qresource><file>Platform/Uri.qml</file></qresource></RCC>";
        let got = problems(&s(&["Platform/Uri.qml"]), qrc);
        assert!(got.iter().any(|p| p.contains("prefix")), "{got:?}");
    }

    #[test]
    fn a_file_outside_any_qresource_is_not_listed() {
        let qrc = "<RCC><file>Platform/Uri.qml</file></RCC>";
        assert_eq!(
            problems(&s(&["Platform/Uri.qml"]), qrc),
            [unlisted("Platform/Uri.qml")]
        );
    }

    #[test]
    fn an_alias_that_differs_from_the_path_is_a_problem() {
        let qrc = r#"<RCC><qresource prefix="/qml/bindle">
            <file alias="Platform/Other.qml">Platform/Uri.qml</file>
        </qresource></RCC>"#;
        let got = problems(&s(&["Platform/Uri.qml"]), qrc);
        assert_eq!(got.len(), 2, "{got:?}");
        assert!(
            got[0].contains("alias") && got[0].contains("Platform/Other.qml"),
            "{got:?}"
        );
    }

    #[test]
    fn malformed_xml_is_a_problem() {
        let got = problems(&s(&[]), "<RCC><qresource></RCC>");
        assert!(!got.is_empty());
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
