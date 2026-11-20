# Changelog

All notable changes to Envio are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- New-mail notifications while Envio runs: a system notification and a
  launcher badge when mail reaches an Inbox (#144). Envio still cannot wake
  a suspended phone; that is planned.
- Drafts go to the server. A draft is uploaded to the account's Drafts
  folder and replaces its older copy, and it is removed when you send or
  delete it (#52, #59, #284). A draft the server refuses (mailbox full,
  folder read-only, too large) shows "Not uploaded" with the reason, and
  Envio retries with growing waits (#266).
- Saved drafts show in the Drafts list and reopen. Edit on a draft in
  Drafts opens it for writing again, also when the draft only exists on the
  server (#228, #324). Delete on a draft has Undo. An account whose server
  has no Drafts folder gets a Drafts entry in the drawer that keeps its
  drafts on the phone (#303).
- Share into Envio: a file shared from another app opens compose with it
  attached (#201). Envio is offered in "Open with" for files of any type
  (#290).
- Rename an account (#217).
- A wide two-pane layout on tablets and desktop windows, with the list on
  the left and the reader or compose on the right. Resizing the window
  keeps the open message or draft (#192). Hover highlights and a
  right-click menu on message rows (#192).
- Keyboard shortcuts and a shortcuts dialog (#192).
- Icons on menu and sheet items (#356).
- For developers: `cargo make run-desktop` runs Envio in a desktop window,
  described in the README (#188).

### Changed

- Microsoft addresses (Outlook.com, Hotmail, Live, MSN and their country
  domains, and `*.onmicrosoft.com`) are refused when you add an account. A
  message says that support for them comes in a later version (#381); it is planned for 0.4.0. An
  existing Microsoft account is shown as not supported and can be removed.
- Newsletters look closer to the sender's design: headings, inner
  alignment, font shorthand and sans-serif faces are kept, empty spacer
  cells no longer draw as dark bars, and hidden preheader text stays
  hidden (#341).
- Sheets are capped in height and scroll when long (#83).
- Setup says the server closed the connection, not that it is
  unreachable, when a server drops the session after connecting (#313).
- Security hardening: files Envio writes are private to the app, and
  autoconfig that points at a host outside the address's domain asks you
  first (#158). A message is limited to 200 distinct remote images (#215)
  and inline images are capped in pixels and checked for their real type
  (#213). Headers Envio adds itself cannot be forged behind leading
  whitespace (#214). Lettre is updated to 0.11.24, which checks SMTP reply
  classes (#236).
- The keyboard handling (pages and sheets rising above the on-screen
  keyboard, the focused field scrolled into view) moved to the shared
  [bindle](https://code.austinschaefer.com/schaefera/bindle) library,
  vendored at `qml/bindle`. No visible change.
- Hold to select, Select all and mailto: links now come from the shared
  bindle library (`qml/bindle`, a submodule). Select all in a field's
  edit menu opens the menu again with Cut and Copy ready, and a selection
  the keyboard made while typing no longer comes back on a press and
  hold.

### Fixed

- "Always load images from this sender" and the known-sender cue now apply
  only when Envio itself verifies the message's DKIM signature for the From
  domain, by looking up the sender's key in DNS while the message is open
  (#285, #156). The signing domain must share the From domain's
  organisation, and the message must have exactly one From address. A
  forged From address no longer loads images or shows as known. The mail
  server's own `Authentication-Results` header is not trusted. "Always for
  this sender" is hidden on mail that cannot be verified.
- Attaching a file with the picker works again: the picked files are handed
  over once Content Hub has finished (#286).
- Edit on a draft never loses a label or flag set on the server: such a
  draft is copied, not replaced (#324).
- Discarding a draft or confirming a dialog no longer crashes the app on
  the phone (#329).
- Tapping the open folder's tab again no longer closes the open message
  (#331).
- Load older no longer skips mail that arrived out of date order, and no
  longer comes back with nothing new under your scroll position when a
  batch held only newer mail: it pages by what the cache lacks and fetches
  on to the next batch, up to 500 messages at a time (#94, #254). A removed
  account no longer gets a notice from it (#95).
- Mail styles containing `var(` or `env(` are now dropped on every
  property, not only in the `background` shorthand (#276). Relative font
  sizes in HTML mail are kept in a readable range, including nested ones
  (#191, #243, #259).
- Call-to-action buttons in mail keep their fill: a link or cell styled
  with the `background` shorthand now shows its background colour (#178).
  Qt rich text cannot draw borders or rounded corners, so those stay plain.
- New mail on an account that is not in the foreground could wait up to
  29 minutes to show up when it arrived at the moment Envio began
  listening for mail; it now shows within 5 minutes (#48). The connection
  and folder selection no longer block other actions (#239).
- Deleting a draft in Drafts removes its local copy, and a deleted draft
  stays gone after a crash or restart (#284, #304). An old copy of a
  deleted message no longer comes back (#159).
- Edit works on a failed Outbox message that has attachments (#47).
- Add to calendar is no longer offered on a cancellation or a reply, and
  an event Envio converts from a cancelled invite keeps
  `STATUS:CANCELLED`, so Calendar no longer imports it as a live meeting
  (#212). An exported invite keeps its description (#211).
- The copy menu in a message reopens after a selection handle drag, and
  the selection menu stays clear of the handles (#296, #291).
- The Airmail theme's frame has square bottom corners in a desktop window
  (#343), and the wide layout's list stops at the pane (#338).
- An error from the server on a flag or search command is shown instead
  of being ignored (#190, #231).
- Tapping a link in a message opens the link sheet again on the phone.
  Since text selection arrived in 0.2.0, a finger tap on a link did
  nothing; only a mouse click worked.
- Links whose address a mailer broke over lines (a tab or newline inside,
  spaces at the ends) open, as a browser would read them, instead of
  losing their address. Sign-in and verification links often come this
  way.
- Links written without `https://` (`//example.com/…` or
  `www.example.com/…`) open over https instead of doing nothing, as
  `www.` addresses in plain-text mail already did.

## [0.2.0] - 2026-11-15

Release milestone:
https://code.austinschaefer.com/schaefera/envio/milestone/15

### Added

- Meeting invites. A message with a calendar invite shows a card with the
  title, time in your zone, place and organizer, with Accept, Maybe and
  Decline (#200). The answer goes to the organizer through the Outbox and
  can be undone while it waits there. The card shows the organizer's
  address, which is where the answer goes.
- Add to calendar on the invite card hands the event to the Calendar app.
  Calendar shows its import preview before anything is saved. Invites
  that name a time zone without defining it are converted so Calendar
  can read them (#206).
- Reaching the end of the list loads older mail (#130).
- Select and copy text in a message body.
- A stopped account is shown in the inbox tab and the drawer, and a
  message waiting in the Outbox says why (#141).
- Development builds show their source, build and install time in About.

### Fixed

- Very large messages open in parts instead of all at once (#155). Text
  over 2 MiB in such a message is cut short with a notice, and its
  attachments are fetched when you tap them.
- Inline images embedded in a message (`cid:`) are shown.
- Many fixes to how HTML mail looks: multi-column layout tables are
  stacked, percentage and unsized image widths fit the screen, remote
  images draw once they load, backgrounds stay behind their text, and
  px, pt and em font sizes are kept in a readable range.
- Enter on an address chip in compose keeps the cursor and keyboard (#145).
- A sideways swipe on a folder row no longer opens it (#146).
- The drawer heading shows the address once when the name is the address
  (#139).
- Flag sync on large folders sends bounded commands.

### Known limitations

- An invite whose time zone Envio has to convert loses its repeat rule,
  attendee list and description in Calendar: a repeating meeting is added
  once, and an agenda or dial-in details in the description are left out.
- Add to calendar is offered on a cancelled invite too. If Envio has to
  convert it, Calendar sees a normal event, not a cancellation (#212).
- Percentage font sizes in HTML mail are not limited yet (#191).
- No background sync and no notifications yet.

## [0.1.0] - 2026-11-13

Release milestone:
https://code.austinschaefer.com/schaefera/envio/milestone/12

First release. Envio is an IMAP and SMTP mail app for Ubuntu Touch 24.04.

### Added

- Accounts with a password or app password on any IMAP and SMTP server.
  Envio was used on a phone with real mailo and inbox.eu accounts during
  the release run. Other providers, such as iCloud, Fastmail and Google
  (app password), should work but were not tried for this release.
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
  "Always for this sender" trusts the From address, which a sender can
  forge, so only use it for senders you know.
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
- Inline images embedded in a message (`cid:`) are not shown yet.
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
