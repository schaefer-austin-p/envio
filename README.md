# Envio

A native email app for Ubuntu Touch first. Spec, design and plans are in
`docs/`; the working rules are in `CLAUDE.md`.

## Run on the desktop

Envio runs on an x86_64 Linux desktop with its own Qt 5.15, without click or
a container:

    git submodule update --init
    cargo make run-desktop

The task configures CMake in `build/desktop`, builds with `cargo` through
cxx-qt, and starts `build/desktop/envio`. Prerequisites: a Rust toolchain,
`cargo-make`, `cmake` 3.24+, a C++17 compiler, and Qt 5.15 base,
declarative, quickcontrols2 and dbus development files (Manjaro:
`qt5-base qt5-declarative qt5-quickcontrols2`; QML modules
`QtQuick.Controls 2` must be installed). The first CMake run downloads
cxx-qt-cmake. `cargo make app-docker` is the container route (build and
checks only, no window).

The window is the phone's 360x640 and is resizable (no two-pane layout yet).

Data is in the app's XDG folders, named after the app id `envio.aps`:
`$XDG_DATA_HOME/envio.aps` (default `~/.local/share/envio.aps`: Outbox,
drafts, `passwords.json`, `oauth.json`) and `$XDG_CACHE_HOME/envio.aps`
(default `~/.cache/envio.aps`: the SQLite cache, attachments). Point both
variables elsewhere for a throwaway run.

### Known gaps

Read from the code, not yet checked by hand.

- File picker (attach) and receiving shares: no Content Hub, so attaching
  does nothing but log a line.
- `mailto:` links work from the command line (`envio mailto:...`), but
  nothing registers Envio as the system mail handler.
- Microsoft sign-in (#318): the browser opens, but `enviooauth://callback`
  is not handled on desktop, so the sign-in cannot finish.
- Notifications and the launcher badge need Lomiri's Postal; without it
  there are none (one log line).
- Passwords are a `0600` file in the data folder, as on the phone; no
  system keyring.
- On-screen keyboard: none reported, so layouts are unchanged.
