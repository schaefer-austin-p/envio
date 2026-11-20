# OpenStore listing, Envio 0.3.0

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

What works in 0.3.0:

- Accounts with a password or an app password. The setup wizard asks for your
  address and looks up the servers. You can also enter them by hand. Rename an
  account at any time.
- A unified inbox, a tab for each account, and a folders drawer. On a tablet or
  a desktop window the list and the reader sit side by side, with keyboard
  shortcuts.
- Notifications for new mail while Envio is running.
- Compose, reply, reply all and forward. Drafts are saved on their own, kept
  on your server's Drafts folder, and reopen from the Drafts list. An Outbox
  holds messages until they are sent.
- Meeting invites with Accept, Maybe and Decline, and Add to calendar.
- Attachments in and out through Content Hub. Share a file into Envio, or pick
  Envio in "Open with".
- `mailto:` links from the browser and other apps.
- Search on the phone, and search on the server for older mail.
- Select several messages with a long press. Archive, delete, move, mark read
  or flag them. Undo is one tap.
- Newsletters and HTML mail look closer to the sender's design.
- Two themes, Carbon and Airmail, each in light and dark.

What it does not do, and why:

- Outlook and Hotmail accounts are not supported yet; support is planned for
  0.4.0. Envio says so when you try to add one.
- No notifications while Envio is closed. Ubuntu Touch suspends apps that
  are not open, so Envio checks for mail only while it runs.
- No encryption (OpenPGP) yet.
- It does not use the system address book. It suggests addresses it has seen.
- Google works with an app password; Google sign-in is not offered.

Privacy:

- Remote images are blocked until you decide otherwise. Tracking content is
  removed. "Always for this sender" is offered only for mail whose sender
  Envio could verify (DKIM), so a forged From address does not get trusted.
- Connections use TLS. A self-signed certificate is shown with its
  fingerprint and pinned only if you accept it.
- No telemetry. Passwords are kept in a file only the app can read, never in
  the mail database or the logs.

Known problems are listed at
https://code.austinschaefer.com/schaefera/envio/issues

## Changelog

0.3.0: new-mail notifications while Envio runs.
Drafts upload to your server and reopen from the Drafts list.
Share into Envio and "Open with". Rename accounts.
Two-pane layout and keyboard shortcuts on tablets and desktop.
Better newsletter layout, menu icons, security hardening, many fixes.
Outlook and Hotmail accounts are not supported yet; support is planned for
0.4.0.

## Permissions

- Networking: to reach your IMAP and SMTP servers.
- Connectivity: to see whether the phone is online, so Envio can show an
  account as offline and retry.
- Content exchange: to attach files from other apps.
- Content exchange source: to hand attachments to other apps to open, save
  or share them.
  Files move through Content Hub in both directions; Envio cannot read other
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
| 5 | Wizard sign-in step with found servers | `wizard-found-servers.png` | Use. Airmail dark, settings found at the provider for the made-up address envio.demo@fastmail.com; the account was not added. |

Shots 1 to 4 use synthetic accounts at envio.test; shot 5 shows a made-up
address whose domain only went to the ISPDB lookup. Check each once more for
status-bar content before upload.
