# Changelog

All notable changes to Envio are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

Release milestone:
https://code.austinschaefer.com/schaefera/envio/milestone/12

## [0.1.0] - unreleased

First release. Envio is an IMAP and SMTP mail app for Ubuntu Touch 24.04.

### Added

- Accounts with a password or app password: mailo, iCloud, Fastmail,
  Google (app password) and any other IMAP and SMTP server.
- An account wizard that asks for the email address first and looks up the
  servers (Thunderbird ISPDB, the domain's autoconfig URL, DNS SRV). Manual
  setup is the fallback.
- A prompt for self-signed certificates that shows the SHA-256 fingerprint.
  If a pinned certificate changes later, Accounts says so and offers
  "Check certificate".
- A unified inbox, a tab for each account, and a folders drawer with the
  Outbox.
- HTML mail shown as Qt rich text. Remote images are blocked by default.
  A link sheet shows the real host before a link opens.
- Compose, reply, reply all and forward.
- An Outbox for messages waiting to be sent, and drafts that are saved on
  their own and kept when you leave the app.
- Attachments in and out through Content Hub: attach, Open, Save and Share.
- `mailto:` links open the compose screen.
- Search on the phone, and "Search the servers too" for older mail.
- Long-press selection with archive, delete, move, mark read and flag, and
  an Undo toast.
- Themes Carbon (default) and Airmail, each in light and dark.
- Recipient suggestions from addresses Envio has seen, and "Add sender to
  Contacts", which stores the address inside Envio only.
- Unread counts per account and in the unified inbox. An offline account
  shows a cloud-off icon.

### Privacy and security

- Mail servers are reached over TLS only. Self-signed certificates are
  pinned after you accept them.
- Remote images are blocked and tracking content is removed.
- No telemetry.
- Passwords are kept in a file readable only by the app (mode 0600). They
  are not in the mail database or the logs. The system secret store is not
  available to a confined app on Ubuntu Touch.

### Known limitations

- No background sync and no notifications. Ubuntu Touch suspends apps that
  are not in the foreground, so Envio syncs only while it is open.
  New-mail notifications are planned for Plan 9 (#144, #54).
- No OAuth. Microsoft accounts need it and do not work yet. Google works
  with an app password.
- No OpenPGP encryption or signing.
- No use of the system address book. Contacts are stored inside Envio.
- Save in the attachment menu was not confirmed end to end on a phone.
- Open bugs in the release milestone:
  - #141: when a server certificate changes, sync stops, but the inbox tab
    and drawer show no sign of it. The Accounts page does show it, and
    queued messages wait in the Outbox with no reason given.
  - #145: in compose, pressing Enter on an address chip drops focus, so the
    next address you type is lost. Tap the field again first.
  - #130: older mail is loaded with a tap on "Load older" (50 at a time),
    not automatically when you scroll to the end.
  - #146: a sideways swipe on a folder row still opens the folder.
  - #139: the drawer heading repeats the address when the account name is
    the address.
