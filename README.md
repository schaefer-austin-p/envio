# Envio

A native email app for Ubuntu Touch first. Spec, design and plans are in
`docs/`; the working rules are in `CLAUDE.md`.

## Run on the desktop

Envio runs on an x86_64 Linux desktop with its own Qt 5.15, without click or
a container:

    git submodule update --init
    cargo make run-desktop

The task configures CMake in `build/desktop`, builds through cxx-qt, and
starts `build/desktop/envio`. Prerequisites: git, a Rust toolchain,
`cargo-make`, `cmake` 3.24+, gettext (`msgfmt`), a C++17 compiler, and Qt 5.15
base, declarative, quickcontrols2 and dbus development files (Manjaro:
`qt5-base qt5-declarative qt5-quickcontrols2`). The first CMake run downloads
cxx-qt-cmake. `cargo make app-docker` is the container route (build and
checks only, no window).

The window starts at the phone's 360x640 and is resizable. From 90 grid units
wide (a 720 px window at scale 1) it switches to the wide layout, the list
and a pane side by side, with keyboard shortcuts.

Data: the app keeps it in `$XDG_DATA_HOME/envio.aps` (Outbox, drafts,
`passwords.json`, `oauth.json`) and `$XDG_CACHE_HOME/envio.aps` (SQLite cache,
logs, attachments). `run-desktop` points these at `build/desktop/dev-data`
and `dev-cache`, so a branch build never touches real accounts. Run with
`ENVIO_REAL_DATA=1` to use your XDG folders (default `~/.local/share` and
`~/.cache`) on purpose; that syncs real accounts, sends the Outbox and runs
this branch's migrations on them.

### Known gaps

Read from the code, not yet checked by hand.

- File picker (attach) and receiving shares: no Content Hub, so attaching
  does nothing but log a line.
- `mailto:` links: `cargo make run-desktop -- mailto:a@b.c` (or
  `build/desktop/envio mailto:...`) is read at startup only. A second launch
  is a second instance on the same data; Envio is not the system mail handler.
- Microsoft addresses are refused until the Graph backend lands (#381); its
  desktop sign-in later needs #318 (`enviooauth://` is not handled here).
- Notifications and the launcher badge need Lomiri's Postal; without it
  there are none (one log line).
- Passwords are a `0600` file in the data folder, as on the phone; no
  system keyring.
- On-screen keyboard: none reported, so layouts are unchanged.
