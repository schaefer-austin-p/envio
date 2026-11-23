//! Which crates a change needs tested: the crates whose files changed, plus
//! every crate that depends on them. Some files change how everything is
//! built, and then everything runs. Pure, so it is tested without git.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// A workspace member: its name, its directory relative to the workspace
/// root, and the names of the other members it depends on.
#[derive(Debug, Clone)]
pub struct Package {
    pub name: String,
    pub dir: PathBuf,
    pub deps: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Selection {
    All,
    Some(BTreeSet<String>),
    None,
}

/// Files that change how every crate is built, linted or tested.
/// manifest.json carries the version, which envio-core's tests check
/// against the workspace version; tools/ci/stalwart.sh (with tools/ci/dind.sh,
/// which it sources) is the server envio-net's integration tests run against.
fn runs_everything(file: &Path) -> bool {
    const FILES: &[&str] = &[
        "Cargo.lock",
        "Cargo.toml",
        "manifest.json",
        "clippy.toml",
        "rustfmt.toml",
        "rust-toolchain",
        "rust-toolchain.toml",
        "Makefile.toml",
        "tools/ci/stalwart.sh",
        "tools/ci/dind.sh",
    ];
    FILES.iter().any(|f| file == Path::new(f))
        || file.starts_with(".cargo")
        || file.starts_with(".forgejo")
}

pub fn select(packages: &[Package], changed: &[PathBuf]) -> Selection {
    if changed.iter().any(|f| runs_everything(f)) {
        return Selection::All;
    }
    let mut direct: BTreeSet<String> = changed.iter().filter_map(|f| owner(packages, f)).collect();
    // The QML sources and their tests are checked against envio-ui's name
    // table (`tests/qml_names.rs`), so a QML-only change runs it.
    if changed.iter().any(|f| is_qml(f)) && packages.iter().any(|p| p.name == UI_CRATE) {
        direct.insert(UI_CRATE.to_owned());
    }
    if direct.is_empty() {
        return Selection::None;
    }
    Selection::Some(with_dependents(packages, direct))
}

const UI_CRATE: &str = "envio-ui";

fn is_qml(file: &Path) -> bool {
    file.starts_with("qml") || file.starts_with("tests/qml")
}

/// The member whose directory holds the file (the deepest, if nested).
fn owner(packages: &[Package], file: &Path) -> Option<String> {
    packages
        .iter()
        .filter(|p| !p.dir.as_os_str().is_empty() && file.starts_with(&p.dir))
        .max_by_key(|p| p.dir.components().count())
        .map(|p| p.name.clone())
}

fn with_dependents(packages: &[Package], mut selected: BTreeSet<String>) -> BTreeSet<String> {
    loop {
        let before = selected.len();
        for p in packages {
            if p.deps.iter().any(|d| selected.contains(d)) {
                selected.insert(p.name.clone());
            }
        }
        if selected.len() == before {
            return selected;
        }
    }
}

/// Arguments for `cargo test` / `cargo llvm-cov`. `always_run` crates have
/// their own CI step, so they are left out here.
pub fn cargo_args(selection: &Selection, always_run: &[&str]) -> String {
    match selection {
        Selection::All => {
            let mut args = vec!["--workspace".to_owned()];
            args.extend(always_run.iter().map(|c| format!("--exclude {c}")));
            args.join(" ")
        }
        Selection::Some(names) => names
            .iter()
            .filter(|n| !always_run.contains(&n.as_str()))
            .map(|n| format!("-p {n}"))
            .collect::<Vec<_>>()
            .join(" "),
        Selection::None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(name: &str, dir: &str, deps: &[&str]) -> Package {
        Package {
            name: name.into(),
            dir: dir.into(),
            deps: deps.iter().map(|d| d.to_string()).collect(),
        }
    }

    /// core -> sync -> net, store, mime, as in the spec; e2e uses core.
    fn workspace() -> Vec<Package> {
        vec![
            pkg("envio-core", "crates/envio-core", &["envio-sync"]),
            pkg(
                "envio-sync",
                "crates/envio-sync",
                &["envio-net", "envio-store", "envio-mime"],
            ),
            pkg("envio-net", "crates/envio-net", &[]),
            pkg("envio-store", "crates/envio-store", &[]),
            pkg("envio-mime", "crates/envio-mime", &[]),
            pkg("envio-ui", "crates/envio-ui", &["envio-core"]),
            pkg("envio-e2e", "crates/envio-e2e", &["envio-core"]),
            pkg("xtask", "xtask", &[]),
        ]
    }

    fn changed(files: &[&str]) -> Vec<PathBuf> {
        files.iter().map(PathBuf::from).collect()
    }

    fn some(names: &[&str]) -> Selection {
        Selection::Some(names.iter().map(|n| n.to_string()).collect())
    }

    #[test]
    fn a_leaf_crate_brings_its_dependents() {
        let sel = select(&workspace(), &changed(&["crates/envio-mime/src/lib.rs"]));
        assert_eq!(
            sel,
            some(&[
                "envio-core",
                "envio-e2e",
                "envio-mime",
                "envio-sync",
                "envio-ui"
            ])
        );
    }

    #[test]
    fn the_top_crate_brings_only_itself_and_e2e() {
        let sel = select(&workspace(), &changed(&["crates/envio-core/src/lib.rs"]));
        assert_eq!(sel, some(&["envio-core", "envio-e2e", "envio-ui"]));
    }

    #[test]
    fn docs_and_the_qt_glue_test_nothing() {
        let sel = select(
            &workspace(),
            &changed(&["docs/10-design.md", "qt/src/lib.rs"]),
        );
        assert_eq!(sel, Selection::None);
    }

    #[test]
    fn a_qml_only_change_selects_envio_ui() {
        for file in [
            "qml/WizardPage.qml",
            "qml/pages/accounts/WizardPage.qml",
            "tests/qml/tst_wizard.qml",
        ] {
            let sel = select(&workspace(), &changed(&[file]));
            assert_eq!(sel, some(&["envio-ui"]), "{file}");
        }
    }

    #[test]
    fn build_inputs_run_everything() {
        for file in [
            "Cargo.lock",
            "Cargo.toml",
            "manifest.json",
            "clippy.toml",
            "rustfmt.toml",
            "rust-toolchain",
            "rust-toolchain.toml",
            "Makefile.toml",
            "tools/ci/stalwart.sh",
            "tools/ci/dind.sh",
            ".cargo/config.toml",
            ".forgejo/workflows/ci.yml",
        ] {
            assert_eq!(
                select(&workspace(), &changed(&[file])),
                Selection::All,
                "{file}"
            );
        }
    }

    #[test]
    fn a_crate_manifest_is_that_crate_only() {
        let sel = select(&workspace(), &changed(&["crates/envio-store/Cargo.toml"]));
        assert_eq!(
            sel,
            some(&[
                "envio-core",
                "envio-e2e",
                "envio-store",
                "envio-sync",
                "envio-ui"
            ])
        );
    }

    #[test]
    fn args_for_everything_skip_the_always_run_crates() {
        assert_eq!(
            cargo_args(&Selection::All, &["envio-e2e"]),
            "--workspace --exclude envio-e2e"
        );
    }

    #[test]
    fn args_for_some_list_packages_without_always_run_ones() {
        let sel = some(&["envio-core", "envio-e2e", "envio-mime"]);
        assert_eq!(
            cargo_args(&sel, &["envio-e2e"]),
            "-p envio-core -p envio-mime"
        );
    }

    #[test]
    fn args_are_empty_when_nothing_or_only_always_run_crates() {
        assert_eq!(cargo_args(&Selection::None, &["envio-e2e"]), "");
        assert_eq!(cargo_args(&some(&["envio-e2e"]), &["envio-e2e"]), "");
    }
}
