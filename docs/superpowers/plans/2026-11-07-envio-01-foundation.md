# Envio Plan 1: Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the qmetaobject "Hello world" into the spec's skeleton: crates under `crates/`, a cxx-qt + CMake app shell (lectern's pattern) that shows an empty Inbox in the Carbon theme, a repo `CLAUDE.md`, and CI that only tests the crates a PR changed.

**Architecture:** The root `Cargo.toml` becomes a virtual, Qt-free workspace (`crates/*`, `xtask`). The Qt glue is its own workspace in `qt/` (a cxx-qt staticlib), linked by CMake into the `envio` binary with `qt/main.cpp`; QML comes from `qml/qml.qrc`. `cargo xtask affected` turns `git diff` + `cargo metadata` into the `cargo test` arguments CI uses.

**Tech Stack:** Rust 1.94 (edition 2024), cxx-qt 0.10.0, Qt 5.15 (Ubuntu Touch 24.04), CMake ≥ 3.24, clickable (cmake builder), `cargo_metadata`, `serde_json`, qmltestrunner, Forgejo Actions.

**Spec:** `docs/superpowers/specs/2026-11-07-envio-design.md` (and `docs/10-design.md` for the colours).

## Plan series

The spec is too big for one plan. Each later plan is written when the one
before it has merged, so it argues from the code that exists:

1. **Foundation** (this plan): layout, app shell, theme roles, CLAUDE.md, modular CI.
2. **envio-store**: schema and numbered migrations, accounts/folders/messages, FTS5, the action-journal file, Outbox and draft files.
3. **envio-mime**: parse, previews, thread refs, HTML sanitizer to Qt rich text, build/reply/forward.
4. **envio-net**: rustls with TOFU pins, IMAP and SMTP clients, autoconfig; Stalwart in CI with the fault-injecting proxy.
5. **envio-sync**: `MailServer` trait, the pure step planner, the driver, CONDSTORE/QRESYNC, IDLE.
6. **envio-core**: commands and events, the worker thread, error kinds, the password file, the Outbox state machine.
7. **UI for 0.1.0**: wizard, inbox, reader, compose, folders, search, selection and undo, settings, Content Hub, `mailto:`, fonts, Airmail.
8. **Release 0.1.0**: phone checklist, memory budgets, `auditing-before-launch`.

## Global Constraints

- `rust-version = "1.94"`, `edition = "2024"` for every crate.
- Qt 5.15 (Ubuntu Touch 24.04, `framework: ubuntu-touch-24.04-1.x`).
- cxx-qt, cxx-qt-lib, cxx-qt-build pinned `=0.10.0`; cxx pinned `=1.0.205`; cxx-qt-cmake `GIT_TAG 0.10.0`.
- App id and gettext domain: `envio.aps`; binary name `envio`.
- Licence: `GPL-3.0-only`.
- The core is Qt-free; `unsafe` only in `qt/` (root workspace has `unsafe_code = "forbid"`).
- Lomiri/Suru APIs only in `qml/platform/ut/`.
- Files around 300 lines; one concept per module.
- CI workflows in `.forgejo/workflows/`; Rust jobs `runs-on: rust-ci`, the app job `runs-on: docker` in `code.austinschaefer.com/schaefera/ut-app-ci:24.04-2.x-1`.
- Default branch is `master`.

## File map

| Path | Responsibility |
| --- | --- |
| `CLAUDE.md` | Repo rules, commands, blast radius |
| `Cargo.toml` | Virtual workspace, shared package fields and lints |
| `.cargo/config.toml` | `cargo xtask` alias |
| `crates/envio-core/` | Moved from `envio-core/`; gains `build_info` |
| `crates/envio-e2e/` | Cross-crate tests (empty for now) |
| `xtask/src/affected.rs` | Pure selection: changed files → crates to test |
| `xtask/src/metadata.rs` | `cargo metadata` → `affected::Package` list |
| `xtask/src/main.rs` | CLI: `cargo xtask affected --base <ref>` |
| `qt/Cargo.toml`, `qt/build.rs` | cxx-qt glue crate `envio_qt`, QML module `Envio 1.0` |
| `qt/src/lib.rs`, `qt/src/build_info.rs` | `BuildInfo` QML singleton |
| `qt/main.cpp` | Start Qt, gettext translator, grid-unit scale, load QML |
| `CMakeLists.txt` | Build the app, compile `.po` files, install |
| `qml/qml.qrc` | QML resources |
| `qml/Main.qml` | Window, theme, first page |
| `qml/platform/Theme.qml`, `qmldir` | Colour roles |
| `qml/platform/ut/SystemColours.qml` | Suru's window colour (system dark flag) |
| `qml/pages/InboxPage.qml` | Empty Inbox |
| `tests/qml/run.sh`, `tests/qml/tst_theme.qml` | QML tests |
| `tools/i18n/update.sh` | Regenerate `po/envio.aps.pot` |
| `tools/ci/app.sh` | App checks (same in CI and `cargo make app-docker`) |
| `.forgejo/workflows/ci.yml` | CI |
| `Makefile.toml`, `clickable.yaml` | Local tasks, click build |
| Delete: `src/`, `build.rs`, `envio-core/` | Replaced |

---

### Task 1: Crates under `crates/`, `BuildInfo`, repo CLAUDE.md

**Files:**
- Move: `envio-core/` → `crates/envio-core/`
- Modify: `Cargo.toml`
- Create: `crates/envio-core/src/build_info.rs`, `CLAUDE.md`
- Modify: `crates/envio-core/src/lib.rs`, `crates/envio-core/Cargo.toml`

**Interfaces:**
- Produces: `envio_core::build_info::BuildInfo { pub version: String, pub commit: String }`,
  `BuildInfo::from_manifest(manifest_json: &str, commit: &str) -> Result<BuildInfo, BuildInfoError>`,
  `enum BuildInfoError { BadManifest(String), NoVersion }`.

- [ ] **Step 1: Move the crate and make the version shared**

```bash
git mv envio-core crates/envio-core
```

In `Cargo.toml`, change the workspace section to:

```toml
[workspace]
members = ["crates/*"]
resolver = "3"

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.94"
license = "GPL-3.0-only"
authors = ["Austin Schaefer <austin.schaefer@mailo.eu>"]
```

Keep `[workspace.lints.clippy]` as it is. (`unsafe_code = "forbid"` joins
in Task 3, once the qmetaobject package with its `cpp!` blocks is gone.) Change the root package's
dependency to `envio-core = { path = "crates/envio-core" }` (the root
package is removed in Task 3).

`crates/envio-core/Cargo.toml`:

```toml
[package]
name = "envio-core"
description = "Email client internals"
version.workspace = true
authors.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[lints]
workspace = true

[dependencies]
serde_json = "1"
```

- [ ] **Step 2: Write the failing tests**

`crates/envio-core/src/lib.rs`:

```rust
//! Envio's logic. Qt-free: the app talks to it through qt/.

pub mod build_info;
```

`crates/envio-core/src/build_info.rs`:

```rust
//! Which build is running, for the About text and the start-up log.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_version_and_commit() {
        let info = BuildInfo::from_manifest(r#"{"name":"envio.aps","version":"1.2.3"}"#, "abc123").unwrap();
        assert_eq!(info, BuildInfo { version: "1.2.3".into(), commit: "abc123".into() });
    }

    #[test]
    fn empty_commit_reads_unknown() {
        let info = BuildInfo::from_manifest(r#"{"version":"1.0.0"}"#, "").unwrap();
        assert_eq!(info.commit, "unknown");
    }

    #[test]
    fn missing_version_is_an_error() {
        assert_eq!(BuildInfo::from_manifest(r#"{"name":"x"}"#, "c"), Err(BuildInfoError::NoVersion));
    }

    #[test]
    fn broken_json_is_an_error() {
        assert!(matches!(BuildInfo::from_manifest("{", "c"), Err(BuildInfoError::BadManifest(_))));
    }

    /// The About text shows manifest.json's version; a release that bumps
    /// only Cargo.toml's (or only the manifest's) fails here.
    #[test]
    fn manifest_version_is_the_workspace_version() {
        let info = BuildInfo::from_manifest(include_str!("../../../manifest.json"), "").unwrap();
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
    }
}
```

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p envio-core`
Expected: compile error, `BuildInfo` not found.

- [ ] **Step 4: Implement**

Add above the tests in `build_info.rs`:

```rust
/// What the About text and the log show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildInfo {
    pub version: String,
    pub commit: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BuildInfoError {
    BadManifest(String),
    NoVersion,
}

impl BuildInfo {
    /// `manifest_json` is the click's manifest.json; `commit` is the git
    /// commit the build script found, empty when there was none.
    pub fn from_manifest(manifest_json: &str, commit: &str) -> Result<Self, BuildInfoError> {
        let manifest: serde_json::Value =
            serde_json::from_str(manifest_json).map_err(|e| BuildInfoError::BadManifest(e.to_string()))?;
        let version = manifest
            .get("version")
            .and_then(serde_json::Value::as_str)
            .ok_or(BuildInfoError::NoVersion)?
            .to_owned();
        let commit = if commit.is_empty() { "unknown" } else { commit }.to_owned();
        Ok(Self { version, commit })
    }
}
```

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p envio-core`
Expected: 5 passed.

- [ ] **Step 6: Write `CLAUDE.md`**

```markdown
# Envio

A native email app for Ubuntu Touch first (24.04, Lomiri, clickable,
AppArmor). Spec: `docs/superpowers/specs/2026-11-07-envio-design.md`; look:
`docs/10-design.md` and `docs/design/`; plans: `docs/superpowers/plans/`.
Hosted on Forgejo (code.austinschaefer.com/schaefera/envio), default branch
`master`.

## Layout

- `crates/envio-core` commands and events, worker thread; `envio-sync`
  planner and driver; `envio-net` IMAP, SMTP, autoconfig, TLS;
  `envio-store` SQLite, migrations, journal, Outbox and draft files;
  `envio-mime` parse, build, HTML to Qt rich text; `envio-e2e` cross-crate
  tests only.
- Dependencies point one way: core → sync → net, store, mime. net, store and
  mime never depend on each other.
- `qt/` cxx-qt glue (its own workspace, holds no logic). `qml/` views;
  Lomiri and Suru APIs only in `qml/platform/ut/`.
- `xtask/` CI helpers (`cargo xtask affected`).

## Rules

- Logic in Rust, Qt and QML only for UI and platform APIs. The core is
  Qt-free; `unsafe` only in `qt/`.
- One concept per module, files around 300 lines, logic kept out of I/O
  code (pure planners, thin drivers).
- Migrations are numbered and never edited after merge.
- Unsent work (Outbox, drafts, pending actions) lives outside the SQLite
  cache and is saved within about 1 s.
- No swipe actions on list rows (Ubuntu Touch edge gestures).
- User-facing text is translated: `qsTr()` in QML, gettext in Rust,
  domain `envio.aps`. Run `cargo make pot` after changing strings.
- Single-crate integration tests go in `crates/<crate>/tests/`; anything
  that needs two crates goes in `envio-e2e`.

## Commands

- `cargo make ci`: fmt, clippy, tests, audit (Qt-free workspace).
- `cargo make app-docker`: the app checks and build in the CI image.
- `cargo make qml-test`: QML tests in the CI image.
- `cargo xtask affected --base origin/master`: what CI would test.
- `cargo make click` / `cargo make click-install`: phone build.

## Blast radius

Trunk:
- `crates/envio-store/`, `crates/envio-sync/`, `crates/envio-net/`,
  `crates/envio-core/`, `crates/envio-mime/`: user data, migrations, sync,
  TLS and passwords, the Outbox, what mail shows on screen.
- `qt/`, `qml/Main.qml`, `qml/platform/Theme.qml`.
- `Cargo.toml`, `CMakeLists.txt`, `clickable.yaml`, `manifest.json`,
  `envio.apparmor`, `.forgejo/workflows/`, `xtask/`, `tools/ci/`.
- Gate configs: `clippy.toml`, `Makefile.toml`.

Leaf: other QML pages, `qml/platform/ut/` details, docs, translations,
`crates/envio-e2e/` tests.
```

- [ ] **Step 7: Check the workspace and commit**

Run: `cargo clippy -p envio-core --all-targets -- -D warnings && cargo fmt --all --check`
Expected: no warnings, no diff.

```bash
git add -A Cargo.toml Cargo.lock crates CLAUDE.md
git commit -m "refactor: move envio-core under crates/ and add BuildInfo"
```

---

### Task 2: `cargo xtask affected`

**Files:**
- Create: `xtask/Cargo.toml`, `xtask/src/main.rs`, `xtask/src/affected.rs`, `xtask/src/metadata.rs`, `.cargo/config.toml`
- Modify: `Cargo.toml` (members)

**Interfaces:**
- Produces: `affected::Package { name: String, dir: PathBuf, deps: Vec<String> }`,
  `affected::Selection { All, Some(BTreeSet<String>), None }`,
  `affected::select(&[Package], &[PathBuf]) -> Selection`,
  `affected::cargo_args(&Selection, always_run: &[&str]) -> String`,
  `metadata::packages(&cargo_metadata::Metadata) -> Vec<Package>`.
- CLI output (one line, used by CI in Task 5): `--workspace --exclude envio-e2e`, or `-p a -p b`, or an empty line.

- [ ] **Step 1: Add the crate and alias**

`Cargo.toml`: `members = ["crates/*", "xtask"]`.

`.cargo/config.toml`:

```toml
[alias]
xtask = "run --quiet --package xtask --"
```

`xtask/Cargo.toml`:

```toml
[package]
name = "xtask"
description = "CI helpers for envio"
version.workspace = true
authors.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish = false

[lints]
workspace = true

[dependencies]
cargo_metadata = "0.19"
```

- [ ] **Step 2: Write the failing tests for the selection**

`xtask/src/affected.rs`:

```rust
//! Which crates a change needs tested: the crates whose files changed, plus
//! every crate that depends on them. Some files change how everything is
//! built, and then everything runs. Pure, so it is tested without git.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(name: &str, dir: &str, deps: &[&str]) -> Package {
        Package { name: name.into(), dir: dir.into(), deps: deps.iter().map(|d| d.to_string()).collect() }
    }

    /// core -> sync -> net, store, mime, as in the spec; e2e uses core.
    fn workspace() -> Vec<Package> {
        vec![
            pkg("envio-core", "crates/envio-core", &["envio-sync"]),
            pkg("envio-sync", "crates/envio-sync", &["envio-net", "envio-store", "envio-mime"]),
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
        assert_eq!(sel, some(&["envio-core", "envio-e2e", "envio-mime", "envio-sync"]));
    }

    #[test]
    fn the_top_crate_brings_only_itself_and_e2e() {
        let sel = select(&workspace(), &changed(&["crates/envio-core/src/lib.rs"]));
        assert_eq!(sel, some(&["envio-core", "envio-e2e"]));
    }

    #[test]
    fn docs_and_qml_test_nothing() {
        let sel = select(&workspace(), &changed(&["docs/10-design.md", "qml/Main.qml", "qt/src/lib.rs"]));
        assert_eq!(sel, Selection::None);
    }

    #[test]
    fn build_inputs_run_everything() {
        for file in ["Cargo.lock", "Cargo.toml", ".cargo/config.toml", ".forgejo/workflows/ci.yml"] {
            assert_eq!(select(&workspace(), &changed(&[file])), Selection::All, "{file}");
        }
    }

    #[test]
    fn a_crate_manifest_is_that_crate_only() {
        let sel = select(&workspace(), &changed(&["crates/envio-store/Cargo.toml"]));
        assert_eq!(sel, some(&["envio-core", "envio-e2e", "envio-store", "envio-sync"]));
    }

    #[test]
    fn args_for_everything_skip_the_always_run_crates() {
        assert_eq!(cargo_args(&Selection::All, &["envio-e2e"]), "--workspace --exclude envio-e2e");
    }

    #[test]
    fn args_for_some_list_packages_without_always_run_ones() {
        let sel = some(&["envio-core", "envio-e2e", "envio-mime"]);
        assert_eq!(cargo_args(&sel, &["envio-e2e"]), "-p envio-core -p envio-mime");
    }

    #[test]
    fn args_are_empty_when_nothing_or_only_always_run_crates() {
        assert_eq!(cargo_args(&Selection::None, &["envio-e2e"]), "");
        assert_eq!(cargo_args(&some(&["envio-e2e"]), &["envio-e2e"]), "");
    }
}
```

`xtask/src/main.rs` (just enough to compile the tests):

```rust
mod affected;

fn main() {}
```

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p xtask`
Expected: compile errors, `Package`, `select`, `cargo_args` not found.

- [ ] **Step 4: Implement the selection**

Add above the tests in `affected.rs`:

```rust
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

/// Files that change how every crate is built.
fn runs_everything(file: &Path) -> bool {
    file == Path::new("Cargo.lock")
        || file == Path::new("Cargo.toml")
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
```

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p xtask`
Expected: 8 passed.

- [ ] **Step 6: Write the failing metadata test**

`xtask/src/metadata.rs`:

```rust
//! The workspace as `affected` sees it, from `cargo metadata`.

use std::collections::BTreeSet;

use crate::affected::Package;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_this_workspace() {
        let meta = cargo_metadata::MetadataCommand::new().no_deps().exec().unwrap();
        let pkgs = packages(&meta);
        let core = pkgs.iter().find(|p| p.name == "envio-core").unwrap();
        assert_eq!(core.dir, std::path::Path::new("crates/envio-core"));
        let xtask = pkgs.iter().find(|p| p.name == "xtask").unwrap();
        assert!(xtask.deps.is_empty(), "outside crates are not members: {:?}", xtask.deps);
    }
}
```

Add `mod metadata;` to `main.rs`.

Run: `cargo test -p xtask metadata`
Expected: compile error, `packages` not found.

- [ ] **Step 7: Implement `packages`**

Add above the tests in `metadata.rs`:

```rust
/// Workspace members with their directories relative to the workspace
/// root and their dependencies on other members.
pub fn packages(meta: &cargo_metadata::Metadata) -> Vec<Package> {
    let root = meta.workspace_root.as_std_path();
    let members: Vec<_> = meta.packages.iter().filter(|p| meta.workspace_members.contains(&p.id)).collect();
    let names: BTreeSet<String> = members.iter().map(|p| p.name.to_string()).collect();
    members
        .iter()
        .map(|p| Package {
            name: p.name.to_string(),
            dir: p
                .manifest_path
                .parent()
                .map(|d| d.as_std_path().strip_prefix(root).unwrap_or(d.as_std_path()).to_path_buf())
                .unwrap_or_default(),
            deps: p.dependencies.iter().map(|d| d.name.to_string()).filter(|n| names.contains(n)).collect(),
        })
        .collect()
}
```

Run: `cargo test -p xtask`
Expected: 9 passed.

- [ ] **Step 8: Write the CLI**

Replace `xtask/src/main.rs`:

```rust
//! `cargo xtask affected --base <git ref>`: prints the `cargo test`
//! arguments for the crates changed since `<ref>` (see affected.rs).

mod affected;
mod metadata;

use std::path::PathBuf;
use std::process::{Command, ExitCode};

/// Crates with their own CI step, never in the printed arguments.
const ALWAYS_RUN: &[&str] = &["envio-e2e"];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [cmd, flag, base] if cmd == "affected" && flag == "--base" => run_affected(base),
        _ => Err("usage: cargo xtask affected --base <git ref>".to_owned()),
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

fn run_affected(base: &str) -> Result<String, String> {
    let meta = cargo_metadata::MetadataCommand::new().no_deps().exec().map_err(|e| e.to_string())?;
    let out = Command::new("git")
        .args(["diff", "--name-only", &format!("{base}...HEAD")])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    let changed: Vec<PathBuf> = String::from_utf8_lossy(&out.stdout).lines().map(PathBuf::from).collect();
    let selection = affected::select(&metadata::packages(&meta), &changed);
    Ok(affected::cargo_args(&selection, ALWAYS_RUN))
}
```

- [ ] **Step 9: Try it**

Run: `cargo xtask affected --base origin/master`
Expected: `--workspace --exclude envio-e2e` (this branch changes `Cargo.toml`).

Run: `cargo xtask affected --base HEAD`
Expected: an empty line.

Run: `cargo xtask nonsense; echo $?`
Expected: the usage line on stderr, then `1`.

- [ ] **Step 10: Lint and commit**

Run: `cargo clippy -p envio-core -p xtask --all-targets -- -D warnings && cargo fmt --all --check`
(not `--workspace` yet: the root qmetaobject package needs Qt until Task 3)

```bash
git add Cargo.toml Cargo.lock .cargo xtask
git commit -m "feat(ci): add cargo xtask affected"
```

---

### Task 3: App shell on cxx-qt and CMake

Replaces qmetaobject. Copies lectern's working setup (`~/dev/lectern/CMakeLists.txt`, `qt/`, `clickable.yaml`).

**Files:**
- Delete: `src/main.rs`, `src/qrc.rs`, `build.rs`
- Modify: `Cargo.toml` (drop the root package), `clickable.yaml`, `Makefile.toml`, `po/envio.aps.pot`
- Create: `qt/Cargo.toml`, `qt/build.rs`, `qt/src/lib.rs`, `qt/src/build_info.rs`, `qt/main.cpp`, `CMakeLists.txt`, `qml/qml.qrc`, `tools/i18n/update.sh`, `tools/ci/app.sh`
- Modify: `qml/Main.qml`

**Interfaces:**
- Consumes: `envio_core::build_info::BuildInfo::from_manifest` (Task 1).
- Produces: QML module `Envio 1.0` with singleton `BuildInfo { version: string, commit: string }`; `qrc:/qml/Main.qml`; `tools/ci/app.sh` (Task 4 adds the QML tests to it).

- [ ] **Step 1: Make the root workspace virtual**

```bash
git rm -q src/main.rs src/qrc.rs build.rs
```

In `Cargo.toml` delete everything from `[package]` to `build = "build.rs"`, the `[lints]` table, the `[[bin]]` table, `[dependencies]` and `[build-dependencies]`. What stays is `[workspace]`, `[workspace.package]`, `[workspace.lints.*]`. Add:

```toml
[workspace.lints.rust]
# The core is plain Rust; the only unsafe in the app is the cxx-qt bridge
# in qt/, which is its own workspace.
unsafe_code = "forbid"

[profile.release]
codegen-units = 1
lto = "thin"
```

Run: `cargo test --workspace`
Expected: passes, with no Qt on the machine needed.

- [ ] **Step 2: The glue crate**

`qt/Cargo.toml`:

```toml
# The Qt glue: QObjects declared in Rust with cxx-qt, built as a static
# library that CMake links into the app. Its own workspace, so the root
# `cargo test --workspace` needs no Qt and cxx stays out of the root
# Cargo.lock. Holds no logic: every call goes to envio-core.
[workspace]

[package]
# No hyphen: cxx-qt-build looks up CXX_QT_EXPORT_CRATE_<package name>,
# which cxx-qt-cmake sets from the (underscored) target name.
name = "envio_qt"
version = "0.1.0"
description = "Qt glue of Envio (cxx-qt). No logic."
edition = "2024"
license = "GPL-3.0-only"
rust-version = "1.94"
publish = false

[lib]
crate-type = ["staticlib"]

[dependencies]
# Exact pins: the bridge symbols carry the cxx version, which must match
# the cxx-gen that cxx-qt-gen generates the C++ with.
cxx = "=1.0.205"
cxx-qt = "=0.10.0"
cxx-qt-lib = { version = "=0.10.0", features = ["qt_gui", "qt_qml"] }
envio-core = { path = "../crates/envio-core" }

[build-dependencies]
cxx-qt-build = "=0.10.0"

# The root workspace's clippy table, repeated (a separate workspace can't
# inherit it). No `unsafe_code = "forbid"`: the bridge needs unsafe.
[lints.clippy]
all = { level = "warn", priority = -1 }
cognitive_complexity = "warn"

[profile.release]
codegen-units = 1
lto = "thin"
```

`qt/build.rs`:

```rust
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
```

`qt/src/lib.rs`:

```rust
//! The QObjects QML uses, registered in the `Envio 1.0` QML module. They
//! translate between Qt and envio-core types and hold no logic.

mod build_info;
```

`qt/src/build_info.rs`:

```rust
//! Which build is running (envio-core's `BuildInfo`). Constant for the
//! app's life.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, version, READ, CONSTANT)]
        #[qproperty(QString, commit, READ, CONSTANT)]
        type BuildInfo = super::BuildInfoRust;
    }
}

use cxx_qt_lib::QString;
use envio_core::build_info::BuildInfo;

pub struct BuildInfoRust {
    version: QString,
    commit: QString,
}

impl Default for BuildInfoRust {
    fn default() -> Self {
        let info = BuildInfo::from_manifest(include_str!("../../manifest.json"), env!("ENVIO_COMMIT"))
            .expect("manifest.json is checked by envio-core's tests");
        Self { version: QString::from(info.version.as_str()), commit: QString::from(info.commit.as_str()) }
    }
}
```

- [ ] **Step 3: `qt/main.cpp`**

```cpp
// Starts Qt and loads the QML. Everything else is in Rust (qt/src,
// crates/) or QML; BuildInfo registers itself in the "Envio" QML module
// that cxx-qt-build generates.

#include <libintl.h>

#include <QByteArray>
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QString>
#include <QTranslator>
#include <QUrl>

namespace {

// qsTr() in QML asks the installed translators; this one answers from the
// gettext catalogue (domain ENVIO_APP_ID), the same one the Rust core uses.
// The disambiguation comment is gettext's msgctxt. A null QString means
// "none", and Qt shows the source text.
class GettextTranslator : public QTranslator
{
public:
    using QTranslator::QTranslator;

    [[nodiscard]] QString translate(
        const char * /*context*/, const char *sourceText, const char *disambiguation, int /*n*/) const override
    {
        if (sourceText == nullptr) {
            return {};
        }
        QByteArray key(sourceText);
        if (disambiguation != nullptr) {
            key = QByteArray(disambiguation) + '\004' + key;
        }
        const char *text = dgettext(ENVIO_APP_ID, key.constData());
        // gettext hands back its argument when there is no translation
        return text == key.constData() ? QString() : QString::fromUtf8(text);
    }

    [[nodiscard]] bool isEmpty() const override { return false; }
};

// Lomiri sizes everything in grid units of GRID_UNIT_PX pixels, 8 at scale
// 1. A Controls 2 app without the Lomiri toolkit gets no scaling of its
// own, so turn the grid unit into Qt's scale factor. Must run before the
// QGuiApplication exists; an explicit QT_SCALE_FACTOR wins.
void scaleFromGridUnit()
{
    bool ok = false;
    const int gridUnit = qEnvironmentVariableIntValue("GRID_UNIT_PX", &ok);
    if (!ok || gridUnit <= 0 || qEnvironmentVariableIsSet("QT_SCALE_FACTOR")) {
        return;
    }
    qputenv("QT_SCALE_FACTOR", QByteArray::number(gridUnit / 8.0));
}

} // namespace

int main(int argc, char *argv[])
{
    scaleFromGridUnit();
    const QGuiApplication app(argc, argv);
    // Must equal the click name: the confined XDG folders are named after it.
    QGuiApplication::setApplicationName(QStringLiteral(ENVIO_APP_ID));
    QGuiApplication::setOrganizationName(QStringLiteral(ENVIO_APP_ID));

    // The click installs share/locale next to the binary.
    const QByteArray locale = (QGuiApplication::applicationDirPath() + QStringLiteral("/share/locale")).toUtf8();
    bindtextdomain(ENVIO_APP_ID, locale.constData());
    bind_textdomain_codeset(ENVIO_APP_ID, "UTF-8");
    textdomain(ENVIO_APP_ID);
    GettextTranslator translator;
    QGuiApplication::installTranslator(&translator);

    QQmlApplicationEngine engine;
    engine.load(QUrl(QStringLiteral("qrc:/qml/Main.qml")));
    if (engine.rootObjects().isEmpty()) {
        return 1;
    }
    return QGuiApplication::exec();
}
```

- [ ] **Step 4: `CMakeLists.txt`, `qml/qml.qrc`, `qml/Main.qml`**

`CMakeLists.txt`:

```cmake
cmake_minimum_required(VERSION 3.24)
project(envio LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

option(ENVIO_WERROR "Treat compiler warnings as errors (on in CI)" OFF)

find_package(Qt5 5.15 REQUIRED COMPONENTS Core Gui Qml Quick QuickControls2)

include(FetchContent)
FetchContent_Declare(CxxQt GIT_REPOSITORY https://github.com/kdab/cxx-qt-cmake.git GIT_TAG 0.10.0)
FetchContent_MakeAvailable(CxxQt)

cxx_qt_import_crate(MANIFEST_PATH qt/Cargo.toml CRATES envio_qt LOCKED
  QT_MODULES Qt5::Core Qt5::Gui Qt5::Qml)
cxx_qt_import_qml_module(envio_qml URI "Envio" SOURCE_CRATE envio_qt)

add_executable(envio qt/main.cpp qml/qml.qrc)
target_link_libraries(envio PRIVATE envio_qml Qt5::Core Qt5::Gui Qt5::Qml Qt5::Quick Qt5::QuickControls2)
target_compile_options(envio PRIVATE -Wall -Wextra -Wpedantic -Wshadow -Wconversion
  -Wold-style-cast -Wnon-virtual-dtor -Woverloaded-virtual -Wformat=2)
target_compile_definitions(envio PRIVATE QT_NO_CAST_FROM_ASCII QT_NO_CAST_TO_ASCII
  QT_NO_KEYWORDS ENVIO_APP_ID="envio.aps")
if(ENVIO_WERROR)
  target_compile_options(envio PRIVATE -Werror)
endif()

# po/*.po -> share/locale/<lang>/LC_MESSAGES/envio.aps.mo
find_program(MSGFMT msgfmt REQUIRED)
file(GLOB ENVIO_PO CONFIGURE_DEPENDS ${CMAKE_SOURCE_DIR}/po/*.po)
set(ENVIO_LOCALE_DIR ${CMAKE_BINARY_DIR}/share/locale)
file(MAKE_DIRECTORY ${ENVIO_LOCALE_DIR})
foreach(po IN LISTS ENVIO_PO)
  get_filename_component(lang ${po} NAME_WE)
  set(mo ${ENVIO_LOCALE_DIR}/${lang}/LC_MESSAGES/envio.aps.mo)
  add_custom_command(OUTPUT ${mo}
    COMMAND ${CMAKE_COMMAND} -E make_directory ${ENVIO_LOCALE_DIR}/${lang}/LC_MESSAGES
    COMMAND ${MSGFMT} --check -o ${mo} ${po}
    DEPENDS ${po} VERBATIM)
  list(APPEND ENVIO_MO ${mo})
endforeach()
add_custom_target(translations ALL DEPENDS ${ENVIO_MO})
add_dependencies(envio translations)

install(TARGETS envio DESTINATION .)
install(DIRECTORY ${ENVIO_LOCALE_DIR} DESTINATION share)
install(FILES LICENSE DESTINATION .)
```

`qml/qml.qrc`:

```xml
<RCC>
    <qresource prefix="/qml">
        <file>Main.qml</file>
    </qresource>
</RCC>
```

`qml/Main.qml`:

```qml
import QtQuick 2.15
import QtQuick.Controls 2.15
import Envio 1.0

ApplicationWindow {
    id: window
    width: 360
    height: 640
    visible: true
    title: "Envio"

    Label {
        anchors.centerIn: parent
        text: qsTr("Envio %1").arg(BuildInfo.version)
    }

    Component.onCompleted: console.info("envio " + BuildInfo.version + " (" + BuildInfo.commit + ")")
}
```

- [ ] **Step 5: `clickable.yaml`**

```yaml
clickable_minimum_required: 8.8.0
builder: cmake
framework: ubuntu-touch-24.04-1.x
kill: envio
env_vars:
  # The image's /opt/rust/cargo is read-only; keep a crate cache in the build.
  CARGO_HOME: ${ROOT}/target/click-cargo
build_args:
- -DRust_CARGO_TARGET=${ARCH_RUST}
- -DENVIO_WERROR=ON
install_root_data:
- manifest.json
- envio.apparmor
- envio.desktop
- assets
```

- [ ] **Step 6: Translations template script**

`tools/i18n/update.sh` (make it executable):

```sh
#!/bin/sh
# Regenerates po/envio.aps.pot from the QML's qsTr() strings and merges it
# into each po/*.po. With --check, changes nothing and fails if the files
# are out of date (CI). Rust strings join in when the core has some (Plan 6).
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"
pot=po/envio.aps.pot
new=$(mktemp)
trap 'rm -f "$new"' EXIT

find qml -name '*.qml' | LC_ALL=C sort | xgettext --files-from=- --output="$new" \
  --language=JavaScript --keyword=qsTr:1 --keyword=qsTr:1,2c \
  --from-code=UTF-8 --add-comments=i18n --package-name=envio.aps

# The creation date changes every run; compare without it.
strip() { grep -v '^"POT-Creation-Date:' "$1" || true; }
if [ "${1:-}" = --check ]; then
  if [ "$(strip "$new")" != "$(strip "$pot")" ]; then
    echo "po/envio.aps.pot is out of date: run cargo make pot" >&2
    exit 1
  fi
  exit 0
fi
cp "$new" "$pot"
for po in po/*.po; do
  [ -e "$po" ] || continue
  msgmerge --quiet --update --backup=none "$po" "$pot"
done
```

Run: `chmod +x tools/i18n/update.sh && tools/i18n/update.sh && tools/i18n/update.sh --check && echo ok`
Expected: `ok`; `po/envio.aps.pot` now holds `msgid "Envio %1"`.

- [ ] **Step 7: The app checks script**

`tools/ci/app.sh` (executable):

```sh
#!/bin/sh
# The Qt side, in the ut-app-ci image (clickable's amd64 image plus node,
# Xvfb, clang tools): translations, qt/ fmt and clippy, qmllint, a -Werror
# CMake build. Same script in CI and `cargo make app-docker`.
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
build=${BUILD_DIR:-$root/build-ci}
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$root/target/ci-qt}

section() { echo "== $1"; }

section "translations: po/ is up to date with the sources"
command -v xgettext >/dev/null || { apt-get update -q && apt-get install -yq --no-install-recommends gettext; }
"$root/tools/i18n/update.sh" --check

section "format: qt/ Rust"
cargo fmt --manifest-path "$root/qt/Cargo.toml" --all --check

section "lint: QML"
qmllint=$(command -v qmllint || echo /usr/lib/qt5/bin/qmllint)
find "$root/qml" -name '*.qml' -exec "$qmllint" {} +

section "lint: qt/ Rust"
cargo clippy --manifest-path "$root/qt/Cargo.toml" --all-targets --locked -- -D warnings

section "build: CMake, warnings as errors"
cmake -S "$root" -B "$build" -DCMAKE_BUILD_TYPE=Release -DENVIO_WERROR=ON \
  ${FETCHCONTENT_BASE_DIR:+"-DFETCHCONTENT_BASE_DIR=$FETCHCONTENT_BASE_DIR"}
cmake --build "$build" -j "$(nproc)"
```

- [ ] **Step 8: `Makefile.toml`**

Replace the whole file:

```toml
[config]
default_to_workspace = false

[env]
# The app job's image: clickable's amd64 UT 24.04 image plus CI tools
# (infrastructure repo, docker/ut-app-ci).
APP_CI_IMAGE = { value = "code.austinschaefer.com/schaefera/ut-app-ci:24.04-2.x-1", condition = { env_not_set = ["APP_CI_IMAGE"] } }

[tasks.format]
command = "cargo"
args = ["fmt", "--all"]

[tasks.format-check]
command = "cargo"
args = ["fmt", "--all", "--check"]

# The Qt-free workspace; qt/ is linted by app-docker.
[tasks.lint]
command = "cargo"
args = ["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]

[tasks.test]
command = "cargo"
args = ["test", "--workspace"]

# What CI's PR run would test.
[tasks.affected]
command = "cargo"
args = ["xtask", "affected", "--base", "origin/master"]

# Named coverage-report: cargo-make reserves "coverage".
[tasks.coverage-report]
command = "cargo"
args = ["llvm-cov", "--workspace", "--summary-only"]

[tasks.audit]
script = '''
cargo audit
cargo audit --file qt/Cargo.lock
'''

[tasks.pot]
description = "Regenerate po/envio.aps.pot after changing strings"
command = "tools/i18n/update.sh"

[tasks.app-docker]
description = "The app checks and build, as CI runs them"
command = "docker"
args = ["run", "--rm", "-v", "${CARGO_MAKE_WORKING_DIRECTORY}:/src", "-w", "/src",
        "-e", "BUILD_DIR=/src/build-docker", "-e", "CARGO_TARGET_DIR=/src/target/docker-qt",
        "${APP_CI_IMAGE}", "tools/ci/app.sh"]

[tasks.ci]
dependencies = ["format-check", "lint", "coverage-report", "audit"]

[tasks.default]
alias = "ci"

# The phone package, cross-compiled in clickable's Docker image.
[tasks.click]
description = "Build the arm64 .click for the phone"
command = "clickable"
args = ["build", "--arch", "arm64"]
env = { CLICKABLE_ENV_ENVIO_COMMIT = { script = ["git rev-parse --short=10 HEAD"] } }

[tasks.click-install]
description = "Build, install on the phone over adb, and launch"
dependencies = ["click"]
script = '''
clickable install --arch arm64
clickable launch --arch arm64
'''
```

Add `build-ci/` and `build-docker/` to `.gitignore`.

- [ ] **Step 9: Build it**

Run: `cargo generate-lockfile --manifest-path qt/Cargo.toml && cargo make app-docker`
Expected: every section passes; the last lines show `[100%] Built target envio`.

Run: `cargo make click`
Expected: `build/aarch64-linux-gnu/app/envio.aps_0.1.0_arm64.click` exists and click-review reports no errors.

If a phone is attached: `cargo make click-install`; the window shows "Envio 0.1.0". Otherwise note in the PR that the phone check is still open.

- [ ] **Step 10: Commit**

```bash
git add -A Cargo.toml Cargo.lock qt CMakeLists.txt qml clickable.yaml Makefile.toml tools po .gitignore
git commit -m "feat: switch the app shell to cxx-qt and CMake"
```

---

### Task 4: Theme roles and the empty Inbox

**Files:**
- Create: `qml/platform/Theme.qml`, `qml/platform/qmldir`, `qml/platform/ut/SystemColours.qml`, `qml/pages/InboxPage.qml`, `tests/qml/run.sh`, `tests/qml/tst_theme.qml`
- Modify: `qml/Main.qml`, `qml/qml.qrc`, `tools/ci/app.sh`, `Makefile.toml`

**Interfaces:**
- Consumes: `BuildInfo` (Task 3).
- Produces: `Theme` (in `qml/platform`, `import "platform"`): properties `airmail: bool`, `darkOverride: var` (null = follow system), read-only `dark` and colour roles `ground, surface, text, heading, secondary, hairline, field, accent, primary, primaryText, tintA, tintB, selected` with the values in `docs/10-design.md`. `InboxPage { theme: Theme }`.

- [ ] **Step 1: The QML test runner and a failing test**

`tests/qml/run.sh` (executable):

```sh
#!/bin/sh
# QML unit tests (tests/qml/tst_*.qml), offscreen. Runs in the ut-app-ci
# image (tools/ci/app.sh, cargo make qml-test).
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
runner=$(command -v qmltestrunner || echo /usr/lib/qt5/bin/qmltestrunner)
QT_QPA_PLATFORM=${QT_QPA_PLATFORM:-offscreen} exec "$runner" -input "$root/tests/qml" "$@"
```

`tests/qml/tst_theme.qml`:

```qml
import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"

TestCase {
    name: "Theme"

    Theme { id: theme }

    function init() {
        theme.airmail = false
        theme.darkOverride = null
    }

    function test_carbon_light() {
        theme.darkOverride = false
        compare(theme.dark, false)
        compare(String(theme.ground), "#f8f6f1")
        compare(String(theme.text), "#1e2a38")
        compare(String(theme.accent), "#b8342c")
    }

    function test_carbon_dark() {
        theme.darkOverride = true
        compare(theme.dark, true)
        compare(String(theme.ground), "#15181d")
        compare(String(theme.accent), "#e36a60")
        compare(String(theme.primaryText), "#15181d")
    }

    function test_airmail_light_has_its_own_heading() {
        theme.airmail = true
        theme.darkOverride = false
        compare(String(theme.ground), "#fbf8f1")
        compare(String(theme.heading), "#1f3a5f")
    }

    function test_airmail_dark() {
        theme.airmail = true
        theme.darkOverride = true
        compare(String(theme.ground), "#141c27")
        compare(String(theme.heading), "#a9c1e3")
    }

    function test_follows_system_when_not_forced() {
        compare(theme.dark, theme.systemWindow.hslLightness < 0.5)
    }

    // WCAG relative luminance and contrast ratio.
    function luminance(c) {
        function channel(v) { return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4) }
        return 0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b)
    }
    function contrast(a, b) {
        const la = luminance(a), lb = luminance(b)
        return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05)
    }

    function test_text_roles_meet_aa_data() {
        return [
            { tag: "carbon light", airmail: false, dark: false },
            { tag: "carbon dark", airmail: false, dark: true },
            { tag: "airmail light", airmail: true, dark: false },
            { tag: "airmail dark", airmail: true, dark: true }
        ]
    }

    // docs/10-design.md: text, secondary and accent on ground reach 4.5:1.
    // A failing value is changed in the doc first, then here and in Theme.qml.
    function test_text_roles_meet_aa(data) {
        theme.airmail = data.airmail
        theme.darkOverride = data.dark
        for (const role of ["text", "heading", "secondary", "accent"]) {
            const ratio = contrast(theme[role], theme.ground)
            verify(ratio >= 4.5, role + " on ground is " + ratio.toFixed(2) + ":1")
        }
    }
}
```

Add to `Makefile.toml`:

```toml
[tasks.qml-test]
description = "Run the QML tests in the CI image"
command = "docker"
args = ["run", "--rm", "-v", "${CARGO_MAKE_WORKING_DIRECTORY}:/src", "-w", "/src",
        "${APP_CI_IMAGE}", "tests/qml/run.sh"]
```

Run: `chmod +x tests/qml/run.sh && cargo make qml-test`
Expected: FAIL, `Theme is not a type`.

- [ ] **Step 2: Implement the theme**

`qml/platform/ut/SystemColours.qml`:

```qml
import QtQuick 2.15
import QtQuick.Controls.Suru 2.2

// Ubuntu Touch's window colour from the Suru style, whose palette stays
// light when the system theme is dark. Suru lives only here.
Item {
    readonly property color windowColour: Suru.backgroundColor
}
```

`qml/platform/qmldir`:

```
Theme 1.0 Theme.qml
```

`qml/platform/Theme.qml`:

```qml
import QtQuick 2.15
import QtQuick.Controls 2.15

// The app's colour roles (docs/10-design.md, "Colour roles"), following
// the system's light/dark switch live. On Ubuntu Touch Suru answers
// (ut/SystemColours.qml); where Suru is missing (desktop) the load fails
// and the palette's window lightness decides. A Control, as Qt 5's Item
// has no palette.
Control {
    id: root
    visible: false

    // Settings: the alternative theme.
    property bool airmail: false
    // null follows the system; true or false forces it (tests, screenshots).
    property var darkOverride: null

    readonly property color systemWindow: loader.item ? loader.item.windowColour : palette.window
    readonly property bool dark: darkOverride !== null ? darkOverride : systemWindow.hslLightness < 0.5

    readonly property var roles: ({
        "carbon": {
            "light": { "ground": "#F8F6F1", "surface": "#FFFFFF", "text": "#1E2A38", "heading": "#1E2A38",
                       "secondary": "#5A5D62", "hairline": "#DAD7CE", "field": "#C9C6BD", "accent": "#B8342C",
                       "primary": "#1E2A38", "primaryText": "#F8F6F1", "tintA": "#F4E3E1", "tintB": "#E3E6EA",
                       "selected": "#ECE9E1" },
            "dark":  { "ground": "#15181D", "surface": "#1F242B", "text": "#E8E5DE", "heading": "#E8E5DE",
                       "secondary": "#A2A5AA", "hairline": "#2E343C", "field": "#3A4049", "accent": "#E36A60",
                       "primary": "#E8E5DE", "primaryText": "#15181D", "tintA": "#3A2526", "tintB": "#26303B",
                       "selected": "#232932" }
        },
        "airmail": {
            "light": { "ground": "#FBF8F1", "surface": "#FFFFFF", "text": "#22303F", "heading": "#1F3A5F",
                       "secondary": "#5D6A78", "hairline": "#DDD5C4", "field": "#CFC6B1", "accent": "#B23A33",
                       "primary": "#1F3A5F", "primaryText": "#FBF8F1", "tintA": "#F3E2DF", "tintB": "#E0E7F0",
                       "selected": "#F1EADB" },
            "dark":  { "ground": "#141C27", "surface": "#1C2633", "text": "#ECE5D6", "heading": "#A9C1E3",
                       "secondary": "#A3AEBB", "hairline": "#2C3747", "field": "#3A475A", "accent": "#E58C84",
                       "primary": "#A9C1E3", "primaryText": "#141C27", "tintA": "#3A2427", "tintB": "#1F2D42",
                       "selected": "#1F2A38" }
        }
    })
    readonly property var current: roles[airmail ? "airmail" : "carbon"][dark ? "dark" : "light"]

    readonly property color ground: current.ground
    readonly property color surface: current.surface
    readonly property color text: current.text
    readonly property color heading: current.heading
    readonly property color secondary: current.secondary
    readonly property color hairline: current.hairline
    readonly property color field: current.field
    readonly property color accent: current.accent
    readonly property color primary: current.primary
    readonly property color primaryText: current.primaryText
    readonly property color tintA: current.tintA
    readonly property color tintB: current.tintB
    readonly property color selected: current.selected

    Loader {
        id: loader
        source: "ut/SystemColours.qml"
    }
}
```

Run: `cargo make qml-test`
Expected: `Totals: ... 0 failed`. If a contrast row fails, fix the value in
`docs/10-design.md` first, then in `Theme.qml` and the test.

- [ ] **Step 3: The empty Inbox**

`qml/pages/InboxPage.qml`:

```qml
import QtQuick 2.15
import QtQuick.Controls 2.15
import "../platform"

// The unified inbox. Empty until accounts exist (Plan 7 fills it).
Page {
    id: page
    required property Theme theme

    background: Rectangle { color: page.theme.ground }

    header: ToolBar {
        height: 56
        background: Rectangle {
            color: page.theme.ground
            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: 1
                color: page.theme.hairline
            }
        }
        Label {
            x: 16
            anchors.verticalCenter: parent.verticalCenter
            text: qsTr("Inbox")
            color: page.theme.heading
            font.pixelSize: 20
            font.weight: Font.DemiBold
        }
    }

    Label {
        anchors.centerIn: parent
        width: parent.width - 32
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.WordWrap
        text: qsTr("No accounts yet.")
        color: page.theme.secondary
    }
}
```

Replace `qml/Main.qml`:

```qml
import QtQuick 2.15
import QtQuick.Controls 2.15
import Envio 1.0
import "platform"
import "pages"

ApplicationWindow {
    id: window
    width: 360
    height: 640
    visible: true
    title: "Envio"
    color: appTheme.ground

    Theme { id: appTheme }

    InboxPage {
        anchors.fill: parent
        theme: appTheme
    }

    Component.onCompleted: console.info("envio " + BuildInfo.version + " (" + BuildInfo.commit + ")")
}
```

`qml/qml.qrc`:

```xml
<RCC>
    <qresource prefix="/qml">
        <file>Main.qml</file>
        <file>pages/InboxPage.qml</file>
        <file>platform/qmldir</file>
        <file>platform/Theme.qml</file>
        <file>platform/ut/SystemColours.qml</file>
    </qresource>
</RCC>
```

Append to `tools/ci/app.sh`:

```sh
section "tests: QML"
"$root/tests/qml/run.sh"
```

- [ ] **Step 4: Check, look, commit**

Run: `cargo make pot && cargo make app-docker`
Expected: all sections pass, the QML tests with `0 failed`.

Look at it: in the image, `QT_QPA_PLATFORM=offscreen` cannot show a window, so run the built binary on the desktop if Qt 5.15 and the cxx-qt build work there, or on the phone with `cargo make click-install`. Expected: a warm off-white page, "Inbox" in dark navy, "No accounts yet." in grey; with the system in dark mode, a near-black page. Compare with `docs/design/carbon-inbox.html` and `carbon-dark.html` (colours only; the list comes in Plan 7).

```bash
git add qml tests tools Makefile.toml po
git commit -m "feat(ui): add theme roles and the empty inbox"
```

---

### Task 5: Modular CI

**Files:**
- Create: `crates/envio-e2e/Cargo.toml`, `crates/envio-e2e/src/lib.rs`
- Modify: `.forgejo/workflows/ci.yml` (rewrite)

**Interfaces:**
- Consumes: `cargo xtask affected --base <ref>` output (Task 2), `tools/ci/app.sh` (Tasks 3, 4).
- Produces: CI jobs `rust`, `app`, `audit`; later plans add steps (Stalwart service in Plan 4).

- [ ] **Step 1: The cross-crate test crate**

`crates/envio-e2e/Cargo.toml`:

```toml
[package]
name = "envio-e2e"
description = "Cross-crate tests of envio; no library code"
version.workspace = true
authors.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish = false

[lints]
workspace = true

[dependencies]
envio-core = { path = "../envio-core" }
```

`crates/envio-e2e/src/lib.rs`:

```rust
//! Cross-crate tests live in `tests/`. CI always runs this crate, whatever
//! changed; `cargo xtask affected` leaves it out of its selection.
```

Run: `cargo test -p envio-e2e && cargo xtask affected --base origin/master`
Expected: 0 tests, ok; then `--workspace --exclude envio-e2e`.

- [ ] **Step 2: Rewrite `.forgejo/workflows/ci.yml`**

```yaml
name: CI

on:
  push:
    branches: [master]
    tags: ["v*"]
  pull_request:

# A newer push to a PR (or to master) cancels that ref's older run.
concurrency:
  group: ci-${{ github.event.pull_request.number || github.ref }}
  cancel-in-progress: true

env:
  # No debuginfo in CI builds: nothing reads it, and it is most of target/.
  CARGO_PROFILE_DEV_DEBUG: "0"
  # sccache can't cache incremental compilations.
  CARGO_INCREMENTAL: "0"
  SCCACHE_CACHE_SIZE: "2G"

jobs:
  # The Qt-free workspace. On a PR, unit and single-crate integration tests
  # run only for the crates the PR changed plus their dependents (cargo
  # xtask affected); fmt, clippy and the cross-crate tests always run.
  # On master and tags everything runs.
  rust:
    runs-on: rust-ci
    steps:
      # Full history: affected diffs against the merge base with master.
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
          persist-credentials: false

      # CARGO_HOME is /usr/local/cargo in the rust image, not ~/.cargo
      - name: Cache cargo registry
        uses: actions/cache@v4
        with:
          path: |
            /usr/local/cargo/registry/index
            /usr/local/cargo/registry/cache
            /usr/local/cargo/git/db
          key: cargo-registry-${{ hashFiles('Cargo.lock') }}
          restore-keys: |
            cargo-registry-

      - name: Cache sccache compilation objects
        uses: actions/cache@v4
        with:
          path: /root/.cache/sccache
          key: sccache-${{ hashFiles('Cargo.lock', '**/Cargo.toml', '.forgejo/workflows/ci.yml') }}
          restore-keys: |
            sccache-

      - name: Format check
        run: cargo fmt --all --check

      - name: Lint
        run: cargo clippy --workspace --all-targets --locked -- -D warnings

      - name: Pick crates to test
        id: pick
        run: |
          if [ "${{ github.event_name }}" = pull_request ]; then
            args=$(cargo xtask affected --base "origin/${{ github.base_ref }}")
          else
            args="--workspace --exclude envio-e2e"
          fi
          echo "args=$args" >>"$GITHUB_OUTPUT"
          echo "Testing: ${args:-no crates changed}"

      - name: Unit and crate integration tests (changed crates)
        if: steps.pick.outputs.args != ''
        run: cargo llvm-cov ${{ steps.pick.outputs.args }} --locked --summary-only

      - name: Cross-crate tests
        run: cargo test -p envio-e2e --locked

      - name: sccache stats
        if: always()
        run: sccache --show-stats

  # The Qt side in the ut-app-ci image: translations, qt/ fmt and clippy,
  # qmllint, the -Werror CMake build and the QML tests. Always runs.
  # After `rust`: the runner takes two jobs at once, and this image's
  # multi-GB pull slows a build it shares the box with.
  app:
    needs: rust
    runs-on: docker
    container:
      image: code.austinschaefer.com/schaefera/ut-app-ci:24.04-2.x-1
    env:
      FETCHCONTENT_BASE_DIR: /root/.cache/envio-fetchcontent
    steps:
      - uses: actions/checkout@v4
        with:
          persist-credentials: false

      # CARGO_HOME is /opt/rust/cargo in the clickable image.
      - name: Cache cargo registry and CMake FetchContent
        uses: actions/cache@v4
        with:
          path: |
            /opt/rust/cargo/registry/index
            /opt/rust/cargo/registry/cache
            /root/.cache/envio-fetchcontent
          key: app-deps-${{ hashFiles('qt/Cargo.lock', 'CMakeLists.txt') }}
          restore-keys: |
            app-deps-

      - name: Cache app build output
        uses: actions/cache@v4
        with:
          path: |
            target/ci-qt
            build-ci
          key: app-build-v1-${{ hashFiles('qt/Cargo.lock', 'qt/Cargo.toml', 'crates/*/Cargo.toml', 'qt/build.rs', 'CMakeLists.txt') }}
          restore-keys: |
            app-build-v1-

      - name: App checks
        run: tools/ci/app.sh

  audit:
    needs: rust
    runs-on: rust-ci
    steps:
      - uses: actions/checkout@v4
        with:
          persist-credentials: false

      - name: cargo audit
        run: |
          cargo audit
          cargo audit --file qt/Cargo.lock
```

- [ ] **Step 3: Check the selection on this branch, then commit**

Run: `cargo make ci`
Expected: fmt, clippy, tests and audit pass.

```bash
git add crates/envio-e2e Cargo.lock .forgejo/workflows/ci.yml
git commit -m "ci: test only changed crates on PRs; add the app job"
```

- [ ] **Step 4: Push and watch CI**

Follow the user's shipping rules: `git fetch origin`, check
`git merge-base --is-ancestor origin/master HEAD` (rebase if it fails), push
the branch, open the PR with the `shipping-a-pr` skill.

Expected on the PR run: `rust` prints `Testing: --workspace --exclude envio-e2e`
(the PR changes `Cargo.toml`); `app` and `audit` pass.

- [ ] **Step 5: Show that the selection works**

Make a docs-only change on a scratch branch off this one (for example, add
a line to `docs/design/README.md`), push it as a draft PR against this
branch's PR base, and check the `rust` job prints `Testing: no crates
changed` and skips the test step while fmt, clippy, cross-crate tests and
`app` still run. Close the scratch PR and delete its branch.
