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
  tests only; `envio-ui` the screens' state (list models as row
  operations, selection, paging, signals), Qt-free.
- Dependencies point one way: ui → core → sync → net, store, mime. net,
  store and mime never depend on each other.
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
- Plans (from Plan 6): signatures, invariants and test cases; full code
  only for tricky parts. Code sketches stay in the git-ignored
  `.superpowers/sdd/<plan>/`.

## Commands

- `git submodule update --init`: once per clone or worktree. `qml/bindle` is a
  submodule; CMake fails with "Cannot find source file qml/bindle/bindle.qrc"
  without it.
- `cargo make ci`: fmt, clippy, tests, audit (Qt-free workspace).
- `cargo make app-docker`: the app checks and build in the CI image.
- `cargo make qml-test`: QML tests in the CI image.
- `cargo xtask affected --base origin/master`: what CI would test.
- `cargo make click` / `cargo make click-install`: phone build.
- `cargo make net-it` / `e2e-it`: tests against a throwaway Stalwart named
  `envio-stalwart-<pid>`; `STALWART_NAME=envio-stalwart` gives the fixed name.
  The container is labelled with its owner PID (`envio.owner-*`); `up.sh`
  removes labelled containers of dead owners from this host at the next start,
  never unlabelled or hand-named ones.

## Blast radius

Trunk:
- `crates/envio-store/`, `crates/envio-sync/`, `crates/envio-net/`,
  `crates/envio-ui/`,
  `crates/envio-core/`, `crates/envio-mime/`: user data, migrations, sync,
  TLS and passwords, the Outbox, what mail shows on screen.
- `qt/`, `qml/Main.qml`, `qml/platform/Theme.qml`.
- `Cargo.toml`, `CMakeLists.txt`, `clickable.yaml`, `manifest.json`,
  `envio.apparmor`, `.forgejo/workflows/`, `xtask/`, `tools/ci/`.
- Gate configs: `clippy.toml`, `Makefile.toml`.

Leaf: other QML pages, `qml/platform/ut/` details, docs, translations,
`crates/envio-e2e/` tests.
