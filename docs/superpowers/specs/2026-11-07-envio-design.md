# Envio design

Date: 2026-11-07. Status: approved in brainstorming, awaiting spec review.

Envio is a native email app for Ubuntu Touch first (24.04, Lomiri, clickable,
AppArmor-confined), with the logic in Rust and a thin Qt/QML shell. Look and
mockups: `docs/10-design.md` and `docs/design/`.

## Goals and non-goals

Goals for 0.1.0: a daily-usable IMAP/SMTP client on a phone that is honest
about what a confined, suspended app can do, never loses unsent writing, and is
private by default.

Non-goals for 0.1.0: background notifications, OAuth, a calendar, an address
book of its own, encryption (OpenPGP).

## Platform constraints

- Apps are suspended when not in the foreground, so there is no background
  sync. Sync runs only while envio is open; a self-hosted push relay using the
  UBports push service is later.
- AppArmor confinement: files only in envio's own folders; other files come
  and go through Content Hub. The system address book and calendar need
  reserved policy groups, so 0.1.0 uses neither.
- System edge swipes (left: launcher, right: app switcher) rule out swipe
  actions on list rows.
- `mailto:` links reach envio through url-dispatcher.

## Scope

### 0.1.0

- IMAP + SMTP accounts with a password or app password: mailo, iCloud,
  Fastmail, Google (app password), any other server.
- Account wizard: email first, autodetect (Thunderbird ISPDB, the domain's
  autoconfig URL, DNS SRV), manual fallback, self-signed certificate prompt.
- Unified inbox, per-account tabs, folders menu with Outbox.
- HTML mail sanitized to Qt rich text; remote images blocked by default.
- Compose, reply, reply all, forward; Outbox; drafts saved on their own.
- Attachments in and out through Content Hub (Open, Save, Share; attach).
- `mailto:` handling.
- Local search plus "Search on server".
- Long-press selection; archive, delete, move, mark read, flag; Undo toast.
- Themes Carbon (default) and Airmail, each light and dark.
- Recipient autocomplete from addresses envio has seen; "Add sender to
  Contacts" stores them locally.

### 0.2.0

- Share into envio (a file shared from another app opens compose with it
  attached).
- Meeting invites (iMIP): invite card in the reader with Accept, Maybe,
  Decline and an optional note; reply sent as `METHOD:REPLY` through the
  Outbox; updates and cancellations shown on the card. "Add to calendar" only
  once the platform route is checked on a phone (Content Hub `.ics` handoff
  or reserved calendar access).

### Later

Microsoft OAuth (browser plus url-dispatcher), Google OAuth (needs a
verification audit), push relay, "Open full view" in a locked-down
QtWebEngine, system address book, export as PDF, signatures and aliases,
two-pane layout for tablets and desktop, OpenPGP, JMAP.

## Architecture

Approach A: envio's own sync engine on Rust crates. Logic lives in Rust; Qt
and QML are used only for UI and platform APIs.

```
qml/                 views; qml/platform/ut/ is the only place for Lomiri APIs
qt/                  cxx-qt glue: QObjects that forward commands and events
crates/envio-core    commands in, events out; owns a worker thread with a tokio runtime
crates/envio-sync    pure step planner + driver that runs the steps
crates/envio-net     IMAP (async-imap), SMTP (lettre), autoconfig, rustls with TOFU pins
crates/envio-store   SQLite (rusqlite), numbered migrations, FTS5, action journal
crates/envio-mime    parse (mail-parser), build, reply/forward, HTML -> Qt rich text
crates/envio-e2e     cross-crate tests only
xtask/               cargo xtask (CI helpers such as `affected`)
```

Dependencies point one way: core -> sync -> net, store, mime. net, store and
mime do not depend on each other. The core is Qt-free. The existing
`envio-core` crate moves under `crates/`, and the glue moves from
`qmetaobject` to cxx-qt.

Rules carried over from lectern: one concept per module, files around 300
lines, logic kept out of I/O code, numbered migrations, state saved within
about 1 s. Lectern's memory budgets do not apply; envio's are set from the
first measurements on a phone.

Room for JMAP: the planner talks to a `MailServer` trait (list folders,
changes since, fetch, store flags, move, append), so a JMAP backend is a
second implementation, not a rewrite.

## Data flow and storage

- **Metadata in SQLite:** per folder UIDVALIDITY, UIDNEXT, HIGHESTMODSEQ; per
  message envelope, flags, preview, thread references, download state.
- **Bodies on disk** as raw `.eml` files in envio's cache folder.
- **Retention:** the last 30 days are kept in sync; bodies are fetched on
  open, and prefetched on Wi-Fi.
- **Sync** is a list of resumable steps, one database transaction per step.
  CONDSTORE/QRESYNC are used when the server offers them; IDLE on the Inbox
  while envio is in the foreground. On resume from suspension every
  connection is assumed dead.
- **Action journal:** user actions apply locally at once, wait about 5 s
  (the Undo window), then go to the server. Failures follow "Error handling".
- **Outbox:** a message is frozen as an `.eml` file written atomically, sent
  over SMTP, then APPENDed to Sent (skipped on Gmail, which files it itself).
  It leaves the Outbox only once the server accepted it. After a crash
  mid-send it stays in the Outbox marked "May have been sent"; envio
  searches Sent for its Message-ID and moves it if found, otherwise asks:
  Send again, or Move to Sent. Envio never resends on its own.
- **Drafts** are files outside the database, saved within about 1 s of a
  change, and uploaded to the Drafts folder when compose is left.
- **Search:** FTS5 over local mail; "Search on server" runs IMAP SEARCH and
  merges the results.

The database is a rebuildable cache of the server. Everything that is not
on the server yet (Outbox, drafts, the action journal's pending actions) is
kept outside the database so a rebuild cannot lose it: Outbox and drafts as
files, pending actions in a small append-only journal file that envio-store
keeps in the app data folder, outside the cache, next to the Outbox and
drafts.

## Passwords, certificates, privacy

- **Passwords** in a separate file in envio's private data folder (mode
  0600), never in the mail database, logs or crash reports. Confinement keeps
  other apps out; device encryption covers a lost phone. In memory they are
  held in `secrecy` types and wiped after use. If a system secret store
  turns out to be reachable from a confined app, envio moves to it.
- **TLS only.** Implicit TLS (993, 465) preferred; STARTTLS required where
  offered, never a plaintext fallback. Certificates checked against the
  system roots. A self-signed certificate is accepted only through the
  fingerprint prompt and pinned per account. A changed pin stops sync and
  shows old and new fingerprints; it is never accepted on its own.
- **Privacy:** remote images blocked by default (allow per message or per
  sender); known tracking pixels removed even when images are allowed; a
  link shows its real address before opening; no read receipts in 0.1.0; no
  telemetry; a generic SMTP EHLO name instead of the device's hostname;
  autodetect sends only the domain to the ISPDB, and the wizard says so;
  logs mask addresses and subjects.

## Error handling

The core sorts each failure into one kind with a fixed response:

| Kind | Examples | Response |
| --- | --- | --- |
| Temporary | network, timeout, server busy, suspension | Retry with backoff (1 s to 5 min, with jitter); "Offline, retrying" bar; no dialogs. |
| Sign-in | wrong or revoked password | Pause the account; "Sign-in failed, update password"; no retry loop. |
| Certificate | pin changed | Stop, as above. |
| Refused by server | quota, size, rejected recipient | Error on that item (Outbox entry with Edit and Retry). |
| Strange message | broken MIME, bad charset | Show that message as raw text; sync continues. |
| Local | disk full; damaged database | Disk full: stop downloads, keep reading and writing. Damaged database: rebuild from the server. |

Also:

- UIDVALIDITY change: drop that folder's cache and sync it again; pending
  actions find their messages by Message-ID.
- An action that fails on the server is dropped silently when the end state
  is what the user wanted (already deleted), otherwise reverted locally with
  a notice.
- A panic in the worker is caught, the worker restarts, and a crash log is
  written locally, never uploaded.
- User-facing error text is chosen and translated (gettext) in Rust; QML
  only shows it.

## Testing and CI

- **envio-mime:** a corpus of real `.eml` files (Gmail, Outlook, Apple Mail,
  Thunderbird, mailing lists, broken encodings, invites) with snapshot
  tests; a sanitizer attack corpus (scripts, CSS tricks, tracking pixels).
- **envio-sync:** table tests for the planner (server state + local state ->
  steps); property tests that unsent work is never deleted and flags
  converge.
- **envio-store:** in-memory SQLite; each migration tested from every older
  schema.
- **envio-net:** against Stalwart in Docker, with a fault-injecting TCP proxy
  that cuts connections mid-step (the next run must resume with no
  duplicates and nothing lost); self-signed and changed-certificate cases.
- **envio-e2e:** the core against Stalwart: add account, sync, send, find it
  in Sent, archive and undo.
- **UI:** a few qmltestrunner smoke tests; a written phone checklist per
  release (Content Hub, `mailto:`, suspend and resume, dark mode, memory and
  sync time, which set the budgets).

**Modular CI** (`.forgejo/workflows/*.yml`, `runs-on: docker`, Stalwart as a
job service):

- `cargo xtask affected` lists crates changed since the merge base with
  master plus their reverse dependencies (`git diff` + `cargo metadata`).
  Only those run their unit tests and `tests/` integration tests.
- Always: `cargo fmt --check`, clippy on the whole workspace, `envio-e2e`,
  and the clickable build.
- Everything runs when `Cargo.lock`, the workspace `Cargo.toml` or
  `.forgejo/` change, and on pushes to master and on tags.

## Design docs

The canvas is exported to `docs/design/` (20 plain HTML boards, Carbon and
Airmail) and the design language written to `docs/10-design.md`, both in
this change. The 0.2.0 invite card board is still to add.

## Open questions to check on a phone

- Can a confined app reach a system secret store?
- Does the Calendar app accept an `.ics` file through Content Hub?
- Are the system CA certificates readable under confinement (expected yes)?
- Real memory use with a few thousand messages, to set budgets.
