# OpenStore listing, Envio 0.1.0

Each section is one OpenStore field, ready to paste.

## Name

Envio

## Package

envio.aps

## Category

Communication

## License

GPL-3.0

## Source

https://code.austinschaefer.com/schaefera/envio

## Support

https://code.austinschaefer.com/schaefera/envio/issues

## Maintainer

Austin Schaefer (as set in `manifest.json`)

## Tagline

Email for Ubuntu Touch: IMAP and SMTP, private by default.

## Description

Envio is a mail app made for Ubuntu Touch. It talks to your mail server over
IMAP and SMTP and keeps its own copy of your mail on the phone.

What works in 0.1.0:

- Accounts with a password or an app password. The setup wizard asks for your
  address and looks up the servers. You can also enter them by hand.
- A unified inbox, a tab for each account, and a folders drawer.
- Compose, reply, reply all and forward. Drafts are saved on their own. An
  Outbox holds messages until they are sent.
- Attachments in and out through Content Hub.
- `mailto:` links from the browser and other apps.
- Search on the phone, and search on the server for older mail.
- Select several messages with a long press. Archive, delete, move, mark read
  or flag them. Undo is one tap.
- Two themes, Carbon and Airmail, each in light and dark.

What it does not do, and why:

- No background sync and no notifications. Ubuntu Touch suspends apps that
  are not open, so Envio checks for mail only while you use it. New-mail
  notifications are planned.
- No OAuth. Microsoft accounts need it, so they do not work yet. Google works
  with an app password.
- No encryption (OpenPGP) yet.
- It does not use the system address book. It suggests addresses it has seen.

Privacy:

- Remote images are blocked until you decide otherwise. Tracking content is
  removed.
- Connections use TLS. A self-signed certificate is shown with its
  fingerprint and pinned only if you accept it.
- No telemetry. Passwords are kept in a file only the app can read, never in
  the mail database or the logs.

This is a first release. Known problems are listed at
https://code.austinschaefer.com/schaefera/envio/milestone/12

## Changelog

0.1.0: first release.
Accounts with a password or app password and an autodetecting wizard.
Unified inbox, compose, reply, forward, Outbox and drafts.
Attachments through Content Hub, `mailto:`, local and server search, Undo.
Themes Carbon and Airmail, light and dark.
Not in this release: background sync and notifications, OAuth, OpenPGP.

## Permissions

- Networking: to reach your IMAP and SMTP servers.
- Connectivity: to see whether the phone is online, so Envio can show an
  account as offline and retry.
- Content exchange: to attach files from other apps and to open, save or
  share attachments. Files move through Content Hub; Envio cannot read other
  folders.

## Screenshots

Source folder: `docs/phone-runs/0.1.0-release/`. Copy the chosen files to
`docs/store/screenshots/`.

| # | Shot | File | Status |
| - | ---- | ---- | ------ |
| 1 | Unified inbox, Carbon light | `th-carbon-light-inbox.png` | Use. Shows the "All" tab with grace and heidi at envio.test; the tab row is cut off at the edge. |
| 2 | Reader, Airmail light | `th-airmail-light-message.png` | Use, but it is a plain-text mail, not curated HTML. Retake if an HTML mail is wanted. |
| 3 | Compose with attachment chips | `ch09-both-attached.png` | Use. Carbon light, two attachments (png and pdf). |
| 4 | Inbox, Carbon dark | `th-carbon-dark-inbox.png` | Use. |
| 5 | Wizard sign-in step with found servers | none | TO TAKE. `trust-imap-wizard.png` shows the certificate dialog over manual settings, not the found servers. |

All files use synthetic accounts at envio.test. Check each once more for
status-bar content before upload.
