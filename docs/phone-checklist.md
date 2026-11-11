# Phone checklist

Run this on a real Ubuntu Touch phone before each release. Install with
`cargo make click-install`. Take a screenshot as evidence for each item and
record the result in the last section. A failed item becomes an issue.

## Content Hub, in

Do: in a new message, tap Attach and pick a photo, then a PDF.
Pass: both show in the compose window with name and size. After the message
is sent, it arrives with both files, and they open.

## Content Hub, out

Do: open a message with an attachment. Use Open, Save and Share in turn.
Pass: Open starts the right app. Save writes the file and says where. Share
shows the share sheet. A failure shows a notice and nothing crashes.

## mailto:

Do: in the browser, open a page with a `mailto:` link that has a subject and
a body.
Pass: envio opens compose with the address, subject and body filled in.

## Suspend and resume

Do: write half a message, send envio to the background for a minute, then
come back. Repeat with the screen locked.
Pass: the draft is still there. The inbox refreshes within a few seconds. No
mail is lost or doubled.

## Dark mode

Do: view every screen in Carbon light, Carbon dark and Airmail light.
Compare with `docs/design/`.
Pass: no unreadable text, no light patches in dark, colours match the boards.

## Memory and sync time

Do: add an account with a few thousand messages. Time the first sync. Read
`ps` or the app log for memory after the sync and after scrolling the list.
Pass: the first sync shows the newest mail at once and finishes without the
app being killed. Record the time and the memory in the table.

## CA roots load confined (#31)

Do: start the app with the AppArmor profile on, then read the app log.
Pass: the roots source is `System` with a count above 0. If it says
`Bundled`, note it in #31 and keep the fallback.

## The trust dialog

Do: add an account whose server uses a certificate the phone does not trust.
Pass: the dialog names the host and shows the fingerprint. Cancel adds
nothing. Accept adds the account and the pin is kept.

## Check certificate for a changed pin

Do: with an account that has a pin, change the server's certificate. On the
Accounts page, tap Check certificate.
Pass: the page shows the old and the new fingerprint and the account stays
stopped until you accept. Without a change it says the certificate matches.

## The SRV confirm

Do: add an address whose settings come from a DNS SRV record.
Pass: the wizard asks you to confirm the server names before it sends the
password. No confirm, no sign-in.

## Search on server

Do: search a word that only old mail has. Tap Search on server. Tap it again
and Cancel while it runs. Search with the network off.
Pass: it shows busy while it runs. Cancel stops it. Found mail is listed. A
failure shows a message, not an empty list.

## The link sheet shows the real host

Do: open a message with a link whose text shows one host and whose address
goes to another, and one with `user@` before the host. Tap each.
Pass: the sheet shows the real host, flags the decoration before the `@`,
and wraps a long host. Nothing opens until you confirm.

## The corpus renders (#29)

Do: open the corpus's newsletter and the 256-level nested file. Time each
from tap to text.
Pass: both show without a crash. The newsletter shows in under 3 seconds.
The 256-level file shows its text or a plain fallback. Remote images stay
blocked.

## No swipe conflicts with edge gestures

Do: in the inbox and in a message, swipe in from each screen edge.
Pass: edge swipes open the launcher, the app switcher and the indicators.
No row reacts to a swipe.

## 0.1.0 pre-release run

| Item | Result | Evidence |
| ---- | ------ | -------- |
