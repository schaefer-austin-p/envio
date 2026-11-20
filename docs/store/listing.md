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
- Notifications for new mail while Envio is running. For mail that arrives
  while Envio is in the background, exempt it from app suspension (UT Tweak
  Tool); Settings > Notifications explains how.
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
  removed. "Always for this sender" is offered on mail signed (DKIM)
  by the sender's domain and applies only while Envio can verify that
  signature, so a forged From address does not get trusted.
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
- Push notifications: to show new-mail notifications and the launcher
  badge.
- Content exchange: to attach files from other apps.
- Content exchange source: to hand attachments to other apps to open, save
  or share them.
  Files move through Content Hub in both directions; Envio cannot read other
  folders.

## Screenshots

Upload in this order from `docs/store/screenshots/`. Taken on 2026-11-20 on a
Volla Phone (Ubuntu Touch 24.04) with the 0.3.0 build, 1080 x 2292, status
bar cropped. All mail is synthetic: the account is grace at envio.test on a
throwaway test server. Shot 6 was taken with the made-up address envio.demo@fastmail.com (not shown on screen),
whose domain only went to the server lookup, and the account was not added.

| # | Shot | File |
| - | ---- | ---- |
| 1 | Unified inbox, Carbon light | `1-inbox-carbon-light.png` |
| 2 | Reader, Airmail light | `2-reader-airmail-light.png` |
| 3 | Reply in compose, Airmail light | `3-compose-airmail-light.png` |
| 4 | Inbox, Carbon dark | `4-inbox-carbon-dark.png` |
| 5 | Inbox, Airmail dark | `5-inbox-airmail-dark.png` |
| 6 | Setup wizard with the servers it found, Airmail light | `6-wizard-airmail-light.png` |
