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

/// Files that change how every crate is built. manifest.json carries the
/// version, which envio-core's tests check against the workspace version.
fn runs_everything(file: &Path) -> bool {
    file == Path::new("Cargo.lock")
        || file == Path::new("Cargo.toml")
        || file == Path::new("manifest.json")
        || file.starts_with(".cargo")
        || file.starts_with(".forgejo")
}

pub fn select(packages: &[Package], changed: &[PathBuf]) -> Selection {
    if changed.iter().any(|f| runs_everything(f)) {
        return Selection::All;
    }
    let direct: BTreeSet<String> = changed.iter().filter_map(|f| owner(packages, f)).collect();
    if direct.is_empty() {
        return Selection::None;
    }
    Selection::Some(with_dependents(packages, direct))
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
            some(&["envio-core", "envio-e2e", "envio-mime", "envio-sync"])
        );
    }

    #[test]
    fn the_top_crate_brings_only_itself_and_e2e() {
        let sel = select(&workspace(), &changed(&["crates/envio-core/src/lib.rs"]));
        assert_eq!(sel, some(&["envio-core", "envio-e2e"]));
    }

    #[test]
    fn docs_and_qml_test_nothing() {
        let sel = select(
            &workspace(),
            &changed(&["docs/10-design.md", "qml/Main.qml", "qt/src/lib.rs"]),
        );
        assert_eq!(sel, Selection::None);
    }

    #[test]
    fn build_inputs_run_everything() {
        for file in [
            "Cargo.lock",
            "Cargo.toml",
            "manifest.json",
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
            some(&["envio-core", "envio-e2e", "envio-store", "envio-sync"])
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
