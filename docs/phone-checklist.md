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
| Content Hub, in | Partial pass | Document Viewer pick of a text file adds it with name and size (`r28`); the draft keeps a copy under `~/.local/share/envio.aps/drafts/files/`. Photo, PDF and send-and-arrive not run. |
| Content Hub, out | Pass | Open starts Document Viewer (`r16`); Save goes through File Manager, which says "saved 1 file into ~/Downloads", file checked (`r21`); Share shows the picker (`r22`). Forward carries both attachments (`r24`). |
| mailto: | Pass | Sent through `lomiri-url-dispatcher`, not the browser UI: compose opens with the address chip, subject and body (`r32`). |
| Suspend and resume | Partial pass | Half-written draft kept after a minute behind System Settings (`s03`); mail appended meanwhile shows within about 7 s (`s04`). Screen-lock variant not run. |
| Dark mode (#15) | Pass | Airmail light/dark and Carbon light/dark for inbox, reader and compose: readable, no light patches (`t-*`). |
| Memory and sync time | Partial | Only 67 test messages, so no first-sync timing for a large box. RSS 296 MB after 38 min of use. |
| CA roots load confined (#31) | Pass | Log: "CA roots: source system, 121 certificates". |
| The trust dialog | Pass | Shown at account add; on a changed pin it names the host, shows old and new fingerprints, Cancel is primary (`c05`). |
| Check certificate for a changed pin | Pass | New server certificate: the account says the certificate changed with both fingerprints (`c04`); Cancel keeps it stopped; Trust gives Ready (`c07`), and the inbox follows the new server (`c08`). |
| The SRV confirm | Not run | Needs a DNS SRV record the phone resolves; covered by envio-net tests. |
| Rich text and links (#29) | Pass | Deep-nesting message renders and opens as fast as a plain one (`r12`); newsletter images blocked, quote levels shown (`r06`, `r07`); spoofed userinfo link names the real host (`r08`); punycode link warns (`r09`, bold host is #76). |
| Search on the server | Pass | Old message: 0 local results, 1 after "Search the servers too" (`q04`); with the server paused it shows progress and Cancel, and Cancel stops it (`q06`, `q07`). |
| No swipe conflicts with edge gestures | Pass | Left edge opens the launcher, app unchanged after (`e05`). A horizontal drag on a row opens the message like a tap (`e02`); no row action. |
| Unread count | Fail, then pass | On df69b78 the account and unified count stayed at the startup value after new or read mail (#75). On 0cb013c: opening an unread message takes the tab from 66 to 65 (`u02`), new mail brings it back to 66 (`u03`), and the drawer's Unified and Inbox agree (`u04`). |

| Upgrade from an earlier build | Fail, then pass | On 2feec5e a phone that had run the earlier migration 0003 failed with "no such column: m.has_attachments", retried every second and flashed "Offline." (`v01`). On 7d8e6be (0003 restored, columns in 0004) the inbox loads, the log has no retries, and messages open (`v02`). |
| Page transitions | Pass | Test build of 7d8e6be with the enter animation slowed to 40 s, linear (not committed). Opening a message: the inbox is gone at once while the reader fades in (`v03`). Back: the reader is gone while the inbox fades in (`v04`). |
| The link sheet opens a link | Pass | On 7d8e6be: the sheet shows `https://example.com/ok` (`v05`); Open starts Morph Browser on that page under AppArmor (`v06`). |

Screenshots are in `docs/phone-runs/0.1.0/`. Phone: Ubuntu Touch 24.04,
build df69b78 (later rows: the build named in the row), throwaway Stalwart
through `adb reverse`.

## Daily-use run (Plan 8b)

| Item | Result | Evidence |
| ---- | ------ | -------- |
| Heavy HTML opens quickly (#88) | Pass | Two real Stripe-built messages (private, not committed). GUI-thread CPU per open: Link notice 0.19 s (was about 10 s), receipt 0.39 s. RSS after both: 214 and 229 MB, peak 238 MB (was 1.4 GB, peak 2.0 GB). The receipt's text column is still narrow: #90. |
| Load older mail (#89) | Pass | The heidi inbox ends at "Load older messages" (`older-ready-*`); one tap loads back to 400 days, then "No older messages" (`older-done-*`). The loading state was too quick to catch against the local server. |
| Sync window (#89) | Pass | Settings → Sync offers 30 days, 90 days, a year and all mail (`sync-window-*`). After "Last 90 days" the list reaches the 88-day message and the row is back to "Load older messages" (`after-90-days-*`). Search then says "Finds mail older than the 90 days kept here". |
| Wizard busy state (#87) | Pass | "Looking up settings…" with a spinner in place of Continue (`wizard-email-busy-*`). The Servers and Sign-in steps finish too fast against the local server to catch. |
| Rows and search (#73, #63) | Pass | Rows show the sender's name, and "(no sender)" and "(no subject)" for a bare message (`rows-placeholders-airmail`). Search opens with the field focused and the keyboard up (`search-open-airmail`). |
| Trust prompt names the service (#62) | Pass | Wizard: "Can't verify the incoming mail server (IMAP)", then "…outgoing mail server (SMTP)" (`trust-imap-*`, `trust-smtp-airmail`). A changed pin on the Accounts page shows the same title with the old and new fingerprints (`pin-airmail`). |
| Link sheet shows the real host (#76) | Pass | `https://xn--pypal-4ve.com/` with the host in bold, then "Displays as paypal.com, which may imitate another site" (`link-idn-*`). |

Screenshots are in `docs/phone-runs/daily-use/`. The user's own account
addresses are painted out or cropped. Phone: Ubuntu Touch 24.04, build
3fe8913, throwaway Stalwart through `adb reverse`.

## Release-polish run (Plan 8c)

| Item | Result | Evidence |
| ---- | ------ | -------- |
| Spoofed display name (#92) | Pass | `"billing@paypal.com" <x@evil.example>`: the row and the reader header show `x@evil.example` (XE avatar), and "billing@paypal.com" appears nowhere (`spoof-row-airmail`, `spoof-reader-*`, `offline-spoof-row-carbon`). |
| Offline banner per tab (#91) | Pass | With the local Stalwart stopped, the grace tab says "Offline." (`offline-grace-airmail`, `offline-spoof-row-carbon`), the All tab says "2 accounts are offline: grace@envio.test, heidi@envio.test" (`offline-unified-airmail`), and the real account's tab, still online, shows no banner. Dropping `adb reverse` alone kept the open connections; stopping the server was needed. |
| Guessed servers can be checked and edited (#93) | Pass | ivan@envio.test (no autoconfig): Sign in says "No settings found. These are a guess, please check them" and lists `imap.envio.test:993, TLS` and `smtp.envio.test:465, TLS` with Edit servers (`wizard-guess-carbon`); Edit servers opens the server form filled in (`wizard-servers-carbon`). Carbon only; the Airmail screens share the same QML. |
| Server lookup (#97) | Pass, rough | Continue to Sign in took at most about 14 s including the ~10 s screenshot overhead; not measurable more finely with the screenshot tool. |
| Layout-table mail uses the width (#90) | Pass | The synthetic `receipt-like` and `notice-like` mail: the item names and the notice paragraph take most of the 360 px column (`receipt-airmail`, `notice-airmail`). In dark mode the mail's own dark text colours are unreadable on the dark card (`dark-*`): #108. |
| Recent mail is prefetched (#102) | Pass | Never-opened messages on grace and heidi opened with their body while the server was stopped or paused (`prefetch-grace-airmail`, `prefetch-heidi-airmail`), so their bodies were downloaded ahead. Open time isn't measurable here (each screenshot takes about 10 s); before this change a first open took 1-2 s on the real accounts. |
| Found on the way | Open | Long certificate fingerprints clip at the right edge on the Accounts page and in Check certificate (`fingerprint-clip-carbon`): #109. The menu's Inbox and Unified inbox counts don't follow messages read in the app: #110. |

Screenshots are in `docs/phone-runs/release-polish/`, synthetic accounts
only; the user's own accounts are painted out. Phone: Ubuntu Touch 24.04,
build 496a6fd, throwaway Stalwart through `adb reverse`, 2026-11-13.
