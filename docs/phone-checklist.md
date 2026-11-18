# Phone checklist

Run this on a real Ubuntu Touch phone before each release. Install with
`cargo make click-install`. Take a screenshot as evidence for each item and
record the result in the last section. A failed item becomes an issue.

## Phone setup

Synthetic mail only: the accounts are the throwaway Stalwart's `grace`,
`heidi` and `ivan` at `envio.test`, never a real account.

1. On the host, start the server: `eval "$(crates/envio-net/tests/stalwart/up.sh)"`.
   It prints `STALWART_IMAPS_PORT`, `STALWART_SUBMISSIONS_PORT` and the other
   published ports.
2. With the phone on adb, forward the ports (the same number on both sides):
   `adb reverse tcp:$STALWART_IMAPS_PORT tcp:$STALWART_IMAPS_PORT` and
   `adb reverse tcp:$STALWART_SUBMISSIONS_PORT tcp:$STALWART_SUBMISSIONS_PORT`.
3. Fill the mailboxes (Python 3, standard library only):
   `python3 tools/phone/seed.py --port $STALWART_IMAPS_PORT --user heidi --password heidi-secret-1 --count 3000 --days 400`
   for the large mailbox, and
   `python3 tools/phone/seed.py --listing --port $STALWART_IMAPS_PORT --user grace --password grace-secret-1`
   for the curated set behind the store screenshots. The same seed gives the
   same subjects every run.
4. In the app's wizard add grace and heidi: host `127.0.0.1`, the IMAPS and
   submission ports from step 1, and accept the trust prompt (the server
   certificate is self-signed).

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
app being killed, and every number is within the Budgets table in the
design spec (idle VmRSS 320 MiB, peak VmHWM 340 MiB, first rows 6 s, cold
start 4 s). Record the time and the memory in the table.

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
| Layout-table mail uses the width (#90) | Pass | The synthetic `receipt-like` and `notice-like` mail: the item names and the notice paragraph take most of the 360 px column (`receipt-airmail`, `notice-airmail`). In dark mode the mail's own dark text colours are unreadable on the dark card (`dark-*`): #108. The notice's button lost its cell padding (a thin bar, `dark-notice-carbon`): #111. |
| Recent mail is prefetched (#102) | Pass | Never-opened messages on grace and heidi opened with their body while the server was stopped or paused (`prefetch-grace-airmail`, `prefetch-heidi-airmail`), so their bodies were downloaded ahead. Open time isn't measurable here (each screenshot takes about 10 s); before this change a first open took 1-2 s on the real accounts. |
| Found on the way | Open | Long certificate fingerprints clip at the right edge on the Accounts page and in Check certificate (`fingerprint-clip-carbon`): #109. The menu's Inbox and Unified inbox counts don't follow messages read in the app: #110. |

Screenshots are in `docs/phone-runs/release-polish/`, synthetic accounts
only; the user's own accounts are painted out. Phone: Ubuntu Touch 24.04,
build 496a6fd, throwaway Stalwart through `adb reverse`, 2026-11-13.

## 0.1.0 release run

| Item | Result | Evidence |
| ---- | ------ | -------- |
| Clean install and first-run screen | Pass | On a clean install envio starts on "Add account" with an empty field and a disabled Continue, in Carbon light (`b01`) and dark (`fr-carbon-dark`). Setting up manually with the email field left empty saves an account with no address or name, so the chip, drawer header and Accounts row are blank: #131 (workaround: type the address first). Opening the drawer the first time logs a `drawerAccount` TypeError: #133. |
| Content Hub in | Pass | The File Manager picker, opened on the synthetic folder only, offers `envio-test.png` (`ch05-folder`) and, after a second pick, the PDF too (`ch08-folder2`). Both attach (`ch06-png-attached`, `ch09-both-attached`). grace sends to heidi; the message arrives with both files, 12 kB and 20 kB (`ch12-recv-open`). The "Choose from" app list and the Pictures folder list also show the owner's apps and folders, so those shots are not kept. |
| Content Hub out | Partial | Open on the received PDF shows the "Open with" app picker, Pass. Share shows the "Share to" picker (File Manager, TELEports, Print, LibreOffice, Messaging and more); nothing was sent, X returns to envio, Pass. Save hands the PDF to a Documents destination whose view is a library of the owner's files; that screen was left without completing it, so Save is not confirmed end to end. No shots kept: all three screens list the owner's apps or files. |
| `mailto:` from the browser | Pass | A page on the host over `adb reverse` with a "Write to heidi" link (`ch23-morph`), tapped in Morph, opens the envio compose screen with To heidi, the subject and body filled and From grace (`ch24-mailto`). |
| Suspend and resume, screen locked | Pass | Screen locked with the power key at 17:18; a message APPENDed from the PC at about 17:19 while locked. After unlocking, envio is the same process (same pid, RSS unchanged at about 298 MB) and the heidi list has "Sent while the phone was locked" at the top (`ch27-resume`). The journal shows only Qt warnings ("Cannot create children for a parent that is in a different thread (QInputInfoManagerMir)", "killTimer/startTimer from another thread"), no crash and no envio error; the source of the QInputInfo use is not looked into yet. |
| Suspend and resume, draft behind another app | Pass | Compose half filled (`ch28-draft-before`), Morph opened through url-dispatcher for 60 s (`ch29-away`), envio started again: the draft is intact (`ch30-draft-back`); `drafts/*.json` was written while away. Sent, it arrives once in heidi, the draft file is removed and the Outbox is empty (`ch31-after-send`). |
| Dark mode, four themes | Pass | Inbox and message in Carbon light and dark and Airmail light and dark (`th-carbon-light-inbox`, `th-carbon-light-message`, `th-carbon-dark-inbox`, `th-carbon-dark-message`, `th-airmail-light-inbox`, `th-airmail-light-message`, `th-airmail-dark-inbox`, `th-airmail-dark-message`); the drawer, compose, Appearance and grace views were checked in all four as well (24 shots, not all kept). #108 paper card: a plain mail on the dark card is readable in both dark themes (`th-carbon-dark-message`, `th-airmail-dark-message`). The `receipt-like` and `notice-like` mail was not kept as a shot in this run. The drawer heading reads "addr · addr" when name and address are the same: #139. |
| #109 Check certificate, IMAP | Pass | After the server certificate was swapped, Accounts says "The certificate of the incoming mail server changed" with the old and new fingerprints whole (`c109-accounts`). Check certificate opens "Can't verify the incoming mail server (IMAP)", both fingerprints and the SHA-256 whole, Cancel as the primary button (`c109-check`); the new fingerprint matches `openssl` on the new certificate. Trust gives Ready and the lists resync to the new server (new UIDVALIDITY). |
| #109 Check certificate, SMTP | Pass, gap | grace's Accounts row says "The certificate of the outgoing mail server changed" (`c109-accounts-smtp`); the dialog is titled "(SMTP)" (`c109-smtp-check`). A queued message sits in the Outbox as "Waiting to be sent" with Delete and no reason (`c109-outbox`): the same for a stopped changed certificate on the tab and drawer, which show no signal at all: #141. After Trust both accounts are Ready (`c109-trusted-both`) and the Outbox empties (`c109-outbox-after`); the message was delivered once (heidi INBOX 1, grace Sent Items 1, checked on the server). |
| #110 unread counts, online and offline | Pass | Reading 3 unread grace messages in the app took the tab from 14 to 11, the drawer Inbox to 11 and the Unified inbox from 297 to 291 (`c110-before`, `c110-list-after`, `c110-drawer-online`). With the server gone the counts stay the same (`c110-drawer-offline`). The Stalwart backstop had ended the server just before, so the offline icons in the "online" shots are real. |
| Edge swipes | Pass, with #142 | Swipes from the left, top and right edges open the launcher, the indicators and the app switcher as on any app; no shot kept (they show the owner's apps and settings). A sideways swipe across a list row opens the message (`h-before`, `h-after-ltr`, `h-after-rtl`): #142. A fix is in progress on branch `row-swipe-142`. |
| Link sheet | Pass | Host in bold: `login.evil-example.net` (`l-sheet1`). The part before the @ is struck through with "Before the @ is decoration. This link opens evil-example.net." (`l-sheet2`). A long host wraps over lines (`l-sheet3`). Nothing opens until Open is tapped. |
| Search on server | Pass; failure message not observed | A 300-day-old message ("The zephyrine lantern") APPENDed to heidi: 0 local results (`s-local`), 1 after "Search the servers too" (`s-found`, 17 Jan). With the server paused it shows "Searching the servers..." and Cancel, still running at 45 s (`s-paused-45`); Cancel stops it (`s-cancelled`). The failure message was not seen: removing `adb reverse` left the live connection answering (0 results, no error), and a paused server gave no timeout within 45 s. |
| Trust dialog | Pass | Wizard, IMAP and SMTP, each named, with the SHA-256 whole and Cancel as the primary button (`trust-imap-wizard`, `trust-smtp-wizard`). Changed-pin form: see #109 above. |
| CA roots line | Pass | `WARN envio_qt::lifecycle: CA roots: source system, 121 certificates` in `envio.log` on this build; matches #31. |
| First sync, heidi (3000 messages, 400 days) | Pass, single run | 208 messages (the 30-day window) were in the database within 4 s of the SMTP trust tap, before the options step shows; sync starts at Connect, not at Done. The count stays 223 after Done. Time to Ready was not taken separately. Older rows without a preview keep an empty preview line until opened (cosmetic, not filed). |
| Memory | Pass, single runs | `VmRSS` / `VmHWM` in kB: idle after first sync 218392 / 231964; after scrolling to the end and one Load older (cache 223 to 273) 203032 / 216944; after opening 20 older messages (5 of 16 fetched were HTML) 211760 / 223096; after 34 opened (28 fetched, 11 HTML) 212892 / 223096; after 10 min in the foreground 212892 / 223096, no growth. Each of these is one run, not a median of three. RSS after the suspend run was about 298 MB. |
| Cold start | Pass | 2.36 / 2.34 / 2.27 s, median 2.34 s (launch request to first frame, phone clock); RSS about 196 MB. |
| Open: prefetched vs fetched on open | Pass, coarse | Prefetched rows open with no fetch. A row fetched on open shows its `.eml` about 0.8-0.9 s after the tap call returns; the tap call itself took 4.25 s, so this is an upper bound only, single run. |
| Open question: secret store | Answered: unavailable | `gnome-keyring-daemon` owns `org.freedesktop.secrets`, but the confined app gets AccessDenied ("An AppArmor policy prevents this sender...") and the journal logs `apparmor="DENIED"` for Introspect and Ping. Passwords stay in the 0600 file. |
| Open question: `.ics` through Content Hub | Answered | `calendar.ubports` declares `{"destination": ["documents","events"]}`, so Calendar is an importer of the events type. Not built; 0.2.0. |
| Open question: CA roots | Answered | 121 certificates from the system store (see the CA roots row). |
| Found on the way | Open | #130 auto-load older mail on scroll (owner request, now one tap per 50), #131 empty email in manual setup, #132 the drawer lists folders as Trash, Drafts, Inbox, Junk, Sent (Inbox not first), #133 `drawerAccount` TypeError, #139 drawer heading "addr · addr", #141 no signal for a changed certificate while stopped, #142 row swipe opens the message. Remove account deletes at once when the account has no unsent work; with Outbox mail, drafts or pending actions it asks "Remove this account?" first. |

Screenshots are in `docs/phone-runs/0.1.0-release/`, synthetic accounts
only (grace, heidi and ivan at envio.test, `envio-test.png` and
`envio-test.pdf`). Shots of the launcher, the app switcher, the
indicators, File Manager, Document Viewer and app pickers are not kept
because they show the owner's phone. Phone: Ubuntu Touch 24.04, build
"envio 0.1.0 (706e3afe9d)", throwaway Stalwart through `adb reverse`,
2026-11-13. The Stalwart server was replaced mid-run (its 2 h backstop
ended) and reseeded, which is why the certificate swap, #109 and #110
shots come after the server change.

## Notifications run (Plan 9a)

Envio exempted with `gsettings` (entry `'envio.aps'`), Carbon light, synthetic
account grace@envio.test only. Mail delivered from the host by IMAP APPEND
into grace's Inbox; another app (Calculator) in front for the background
steps.

| Item | Result | Evidence |
| ---- | ------ | -------- |
| Exemption command | Pass | The list holds package names (`'music.ubports'`, ...), so the entry is `'envio.aps'`, not `'envio.aps_envio'`. Set over adb with `DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/32011/bus` (without it `gsettings set` exits 0 and silently changes nothing; the Terminal app has the bus). Envio behind Calculator stayed `S (sleeping)`, IDLE kept receiving. UT Tweak Tool (installed, asks for the passcode, so not driven): Apps + Scopes, Envio, Details, General, "Prevent app suspension" (from its QML). |
| 1. One message, card with the account name | Pass | "grace@envio.test / 1 new message", Envio's icon (`card-1-message`). |
| 2. Three messages, "3 new messages" | Pass | Appended in one burst: "3 new messages" (`card-3-messages`). Appended 0.5 s apart they sync one by one and give separate "1 new message" cards, and one burst split into "2 new messages" and "1 new message". |
| Bursts collapse into one card | Fail | Cards stack (`card-stacked-cards`): the spec says tag `new-<key>` replaces the earlier card, but the local `Post` leaves them all in the drawer. |
| 3. Text switch on, sender and subject | Pass | "Ada Tester / Notify test G #1" (`card-text-on`). The page in Carbon light: fonts, spacing and the wrapped command block as designed, no scrolling needed (`settings-notifications-carbon`, `settings-notifications-text-on`). |
| 4. Card action brings Envio to the front | Partly | `DispatchURL appid://envio.aps/envio/current-user-version` brings Envio forward (`card-action-dispatch`), on the page it was left on (Settings, Notifications), not the Inbox. The real tap on the card is the owner's. |
| 5. Launcher badge equals Inbox unread | Pass | Badge 31, Inbox unread 31 (IMAP UNSEEN 31, the menu's Inbox count 31) (`launcher-crop`). |
| 6. No exemption: no card while suspended | Pass | Original list restored; Envio behind Calculator went to `T (stopped)`, a message delivered then gave no card and no log line (`suspended-no-card`). |
| 6. No exemption: none on resume while focused | Pass | On resume the worker logged one `notified ... count 1` for the missed message but no card showed in the drawer (`resume-no-card`): Postal drops it while Envio is focused. |
| German card text | Skipped | The phone's `LANG` is en_US and no `de` catalogue exists yet. |

Shots are git-ignored (`.superpowers/sdd/09a/shots/`). Phone: Ubuntu Touch
24.04.5, dev build 0.2.1 (branch plan9a-notify, version bumped locally so
AppArmor regenerated), throwaway Stalwart through `adb reverse`, 2026-11-18.
