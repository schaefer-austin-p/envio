# Design conformance

How we check that the built screens match the design, and the first
audit's results.

## The process

Run it before each release and after any change to `qml/platform/Theme.qml`
or a shared control.

1. **Sources.** `docs/10-design.md` wins over the boards in `docs/design/`.
   Render each board at 1360x1040; board pixels are phone-frame pixels
   (360 wide).
2. **Shots.** On the phone, take one shot per screen with ute2edriver
   (`ute2e shot`, 1080x2400) in each theme and scheme: Carbon light and
   dark, Airmail light and dark. Screens a shot cannot reach (prompts,
   empty states) are audited from QML.
3. **Measure, don't eyeball.** For each screen, list every element the
   board shows and compare type face, size, weight, colour role, radius,
   padding and icon size against the QML (`gu()` is 8 px). Skip
   differences under 2 px.
4. **Table.** One row per deviation:
   `| Theme | Element | Board | Built (file:line) | Severity | Suggested fix |`.
   Severity: High when the theme's character is missing, Medium for a
   visible size or layout error, Low for a rule or tint.
5. **Root causes.** Group rows that share one cause (a missing Theme role,
   a shared control) and file one issue per cause, labelled
   `design-drift`, with a moscow label and a milestone. Rows with no shared
   cause get their own issue only when High or Medium.
6. **Check the fix.** A fix issue closes with before and after shots of the
   screens its rows name.

## Audit of 2026-11-11 (Plan 7 build)

Sources: `docs/design/*.html`, `docs/10-design.md`, `qml/`, phone shots in
Carbon light only. Issue #64 (IconButton icon fills the button) was fixed
in Plan 7 and is not re-reported. Root causes: R1 #65, R2 #66, R3 and R4
#67, R5 #68, R6 #69, R7 #70, R8 #71, R9 #72. The phone run added #74 (link
colour in mail bodies).

## Shared root causes (each should become one issue)

| # | Root cause | Where it shows | Severity |
| --- | --- | --- | --- |
| R1 | **Airmail border does not exist.** `Theme.chevron` and `Theme.chevronBlue` are defined (`Theme.qml:75-76`) but nothing in `qml/` reads them. The board frame is a 10 px padded ring of `repeating-linear-gradient(135deg, red 14, paper 14-28, blue 28-42, paper 42-56)`, outer radius 30, inner radius 20, around every page, sheet and dialog (the dialogs sit inside it). This is the user's first complaint. | Every Airmail screen | High |
| R2 | **No Airmail stamp avatar.** `Avatar.qml` is never instantiated (grep: zero users). Airmail rows (inbox, search, reader header, accounts, outbox) show a 40x48 perforated stamp with Lora initials (red-tint or blue-tint), and Airmail has no unread dot (unread = bold sender, 600). Built shows the accent dot in both themes and no avatar. `Avatar.qml:15` is a 4 px rounded rect, not a stamp, and is 44x44. | Inbox rows, reader header, accounts list, stamp colour picker | High |
| R3 | **`fontMono` is hard-coded as the "label" face.** Airmail's type table is Inter and Lora only; mono must never appear. Mono is hard-coded in `DateText.qml:27`, `MessageRow.qml:117`, `FoldersDrawer.qml:82,93`, `HeaderBlock.qml:56,66`, `AttachmentList.qml:19,59`, `Suggestions.qml:59`, `AttachmentChips.qml:53`, `Toast.qml:65`, `SearchPage.qml:124`, `AppearancePage.qml:102`, `TrustDialog.qml:38` (fingerprint; mono is right there). Only `AppButton`, `Chip`, `MessageRow:56` and `TopBar` branch on `airmail`. Needs a `Theme.fontLabel` (Carbon mono, Airmail Inter) and an `eyebrow` style (Airmail: Inter 12 semibold, caps, letter-spaced). | Every Airmail screen | High |
| R4 | **No Lora reading and writing face.** `ReaderPage.qml:178` is Inter 16 in both themes. The Airmail boards set reading body, compose body and the quoted fold in Lora (about 16 px, line height 1.7). There is no `Theme.fontBody`. | Airmail reader, compose | High |
| R5 | **Airmail button and toast shape missing.** Board Airmail primary, secondary, Send and toast are full pills (radius about 26, 52 high) in Inter 15 semibold. `AppButton.qml:50` gives 12 px; `Toast.qml:31` gives 8 px for both themes; `Chip` and Field radii do not change. Needs one `Theme.controlRadius` role (Carbon 8, Airmail pill). | All Airmail buttons, toast, Send | High |
| R6 | **No disclosure chevrons anywhere.** The "forward" and "down" glyphs exist in `Icon.qml:32-33`, but nothing uses them. Boards show a chevron on settings rows, folder-group rows ("Work, 8 folders"), selects (From, Sync, Keep mail), "Server settings" and provider rows. Also the role split (Airmail red/blue) is unused. | Settings, folders drawer, compose From, wizard | Medium |
| R7 | **Top bar is not the board's.** Board bar is a 48 px button row with 12 px top padding (about 60 px), title Carbon 20 mono 600 / Airmail 26 Lora 600, 24 px glyphs, and no bottom hairline. Built: `TopBar.qml:17` touch + 8 = 52, title 18 and 24 (`:49`), hairline at `:24-29`, `IconButton` 44 px (#64 aside). Compose needs a close X, an in-bar Send with a paper-plane glyph and no overflow button. | Every page | Medium |
| R8 | **Page footers are in-flow, not pinned.** Board puts the primary action in a bottom bar (10 top, 16 bottom padding, 52 button, Carbon hairline above, Airmail none). Built wizard steps, options and the sync page put `AppButton`s in the flow (p01, p02, p09, p12). The inbox footer and reply bar are pinned, but always draw a hairline (`InboxPage.qml:200`, `ReplyBar.qml:14`). | Wizard, options, sign-in | Medium |
| R9 | **Fixed (#72): Row and list separators use `hairline` where the board uses a lighter inner rule** (Carbon `#E4E1D8`, Airmail `#EAE3D3`; `hairline` is `#DAD7CE` / `#DDD5C4`). `ListRow.qml:21-28`. `rowRule` role, opt-in per row (`lightRule`, set by `MessageRow`). | Every list | Low |

## Inbox, rows and account tabs

| Theme | Element | Board | Built (file:line) | Severity | Suggested fix |
| --- | --- | --- | --- | --- | --- |
| Airmail | Page border | 10 px chevron ring, 30/20 radii | none (R1) `Main.qml`, `AppShell.qml` | High | R1: one `AirmailFrame` around the stack and overlays |
| Airmail | Row avatar | 40x48 perforated stamp, 14 px gap, no dot | no avatar, accent dot at `MessageRow.qml:21-28` | High | R2: stamp in `MessageRow`, hide the dot in Airmail |
| Airmail | Unread marker | sender 600, subject 500, no dot | dot shown; subject DemiBold 600 (`MessageRow.qml:98`) | Medium | Airmail: weight 500 on subject |
| Airmail | Sender type | Inter 16 | Inter 14 (`MessageRow.qml:57`) | Medium | 16 in Airmail, 14 Carbon |
| Airmail | Date | Inter 13 secondary | Plex Mono 12 (`DateText.qml:27-28`) | Medium | R3 `fontLabel`, 13 |
| Carbon | Unread dot | 8 px dot, text column begins at 16+8+12 = 36 | dot 8 px, text begins at 16+22+12 = 50 (`MessageRow.qml:42`; 22 reserved for the selection box) | Medium | Reserve 8 px; widen only while selecting |
| Carbon | Folder tag line | mono 11, +0.5 spacing, top margin 6 | matches (`MessageRow.qml:109-119`); built also shows it in the unified view only | Low | none |
| Airmail | Account tag on rows | not shown | `showAccount` mono tag shown in unified view (`:111`) | Low | Hide in Airmail or give it Inter (R3) |
| Both | Row separator | Carbon `#E4E1D8`, Airmail `#EAE3D3` | fixed: `rowRule` via `lightRule` (`ListRow.qml`, `MessageRow.qml`) | Low | R9 (#72) |
| Carbon | Account tabs | chips 44 high, 6 px radius, mono 13, gap 6, bar padding 8/16/12 | matches (`Chip.qml`, `AccountTabs.qml:136-151`); label appends an unread count "Name  3" (`AccountTabs.qml:172`) | Low | Board shows names only; drop the count or style it |
| Airmail | Account tabs | text tabs, 3 px underline in heading blue, Inter 15 (600 selected, 500 other), padding 4/12/0 | underline 3 px, Inter 15 (`Chip.qml:36-44`); underline sits above the row hairline in the board, built has a separate gap row | Low | none |
| Both | Top bar | see R7 | `TopBar.qml:17,49` | Medium | R7 |
| Carbon | Title | Plex Mono 20/600 | 18 (`TopBar.qml:49`) | Medium | 20 |
| Airmail | Title | Lora 26/600 in heading blue | 24 | Medium | 26 |
| Airmail | "New message" button | label "Write a message", pill, Inter 15/600 | label "New message", 12 px radius (`InboxPage.qml:208`, `AppButton.qml:50`) | High | R5 and the Airmail string |
| Carbon | Footer | hairline, padding 10/16/16, button 52 radius 8 | hairline, gu(3) padding, 52 high, radius 8 (matches) | Low | none |
| Airmail | Footer | no rule above | rule drawn (`InboxPage.qml:200`) | Low | skip the rule in Airmail |
| Both | Offline banner | tintA, red cloud-off icon, "Offline." bold, 14 px | no banner: a cloud-off icon on the account's tab and drawer row (#122) | Low | none (owner's choice) |
| Airmail | Undo toast | pill, heading-blue-dark fill, Inter | 8 px radius, mono action (`Toast.qml:31,65`) | Medium | R5, R3 |

## Folders drawer, search, selection

| Theme | Element | Board | Built (file:line) | Severity | Suggested fix |
| --- | --- | --- | --- | --- | --- |
| Airmail | Page border | ring (R1) | none | High | R1 |
| Airmail | Section labels ("ALL ACCOUNTS", account header) | Inter 12 semibold, caps, spacing 1 | Plex Mono 11 (`FoldersDrawer.qml:82-83`) | High | R3 |
| Both | Counts | accent, Carbon mono 13/600, Airmail Inter 13/600 | mono in both (`FoldersDrawer.qml:93`) | Medium | R3 |
| Both | Row glyphs | 24 px outline glyphs (inbox, drafts, sent, archive, spam, trash, folder), 12 px gap to label | `FolderLabel.qml` text only, no glyph; `Icon.qml` has no inbox, sent, drafts, spam or folder paths | High | Add 7 glyphs; add icon column to `FolderLabel` |
| Both | "Work, 8 folders" group row | label, count, chevron | no chevron (R6) | Medium | R6 |
| Both | Bottom bar | "Accounts" and "Settings" with glyphs, hairline above | `AppButton`s at `FoldersDrawer.qml:220,227`, outlined buttons | Medium | Replace with glyph-text actions |
| Both | Search chips | 44 high; Carbon 13 mono, Airmail Inter pills (fully round) with 1 px border | square-ish chips (`Chip.qml:36-44` has no Airmail pill background) | Medium | Airmail pill outline background |
| Both | Search field | pill (Airmail) or 8 px (Carbon) with search glyph inside, 44 high | `SearchPage.qml` plain Field, no inline glyph | Medium | Add inline glyph; radius from R5 |
| Both | Search results | match highlight in tintA, "ON THIS PHONE . 3" label | not audited visually; label mono at `SearchPage.qml:124` | Low | R3 |
| Both | "Search the servers too" | outline button with glyph, hint under | present (`SearchPage.qml:204`) | Low | none |
| Carbon | Selection box | 22 px square, 4 px radius, 2 px field border; checked = primary fill | matches (`MessageRow.qml:24-30`) | Low | none |
| Airmail | Selection box | 22 px circle, checked = heading blue | radius 4 (`MessageRow.qml:26`) | Medium | `radius: Theme.airmail ? 11 : 4` |
| Both | Selection bar title | "3 selected" one line, 20/26 px title | `SelectionBar.qml:21` through `TopBar` 18/24; board wraps at the same width, so skip | Low | R7 |
| Both | Empty hint | "Hold a message to start selecting" at bottom | not found in QML | Low | add hint |

## Reading view

| Theme | Element | Board | Built (file:line) | Severity | Suggested fix |
| --- | --- | --- | --- | --- | --- |
| Airmail | Page border | ring | none | High | R1 |
| Airmail | Header block | stamp avatar, name Inter 16/600, "me . Personal . 09:12" 13 secondary, hairline under | mono "From / To / Date" rows (`HeaderBlock.qml:49-66`) | High | Airmail variant of `HeaderBlock` with R2 stamp |
| Airmail | "PAR AVION" tag | red outlined rotated tag in top bar | missing | Medium | add to Airmail reader bar |
| Carbon | Subject | Inter 22/600 (reading board; the prompts board shows mono) | `fontTitle` = Plex Mono 22/600 (`HeaderBlock.qml:30-33`) | Medium | Reading board wins unless spec says otherwise; decide and align boards |
| Airmail | Subject | Lora 26/600 heading blue | Lora 22 (`HeaderBlock.qml:32`) | Medium | 26 |
| Airmail | Body text | Lora about 16/27, text role | Inter 16 (`ReaderPage.qml:178-179`) | High | R4 |
| Carbon | Body | Inter 15-16 | Inter 16 | Low | none |
| Both | Images-blocked banner buttons | 44 high, 6 px radius (Airmail pill), 14 padding, 13 px mono / Inter 600 | `AppButton` 52 high, 8 px radius, 15 px (`ImagesBanner.qml:26-36`) | Medium | Add `compact` AppButton |
| Both | Images-blocked copy | "Always from this sender" | "Always for this sender" (`ImagesBanner.qml:36`) | Low | align text |
| Both | Images-blocked banner icon | lock glyph, `selected` fill | no lock glyph in `Icon.qml` | Low | add lock glyph |
| Both | Reply bar | Carbon: Reply (primary) + Forward (outlined); Airmail adds a 52 px round trash button | `ReplyBar.qml` two buttons only; hairline always drawn | Medium | Airmail trash button, drop rule in Airmail |
| Both | Top-bar actions | archive, delete, more (24 px) | present | Low | R7, #64 |
| Both | Attachment list | eyebrow "2 ATTACHMENTS . 106 KB", rows with file glyph, name 15/500, size, more glyph | mono eyebrow, no total size, size is mono 12 (`AttachmentList.qml:19-20,59-60`) | Medium | R3, add total |
| Both | Attachment sheet | grab handle, title+meta, rows with glyph, title 16 and 13 secondary, Cancel button | `Sheet.qml` matches radius (16 / 26) and handle; rows lack the glyph column | Medium | glyphs open, download, share |

## Compose

| Theme | Element | Board | Built (file:line) | Severity | Suggested fix |
| --- | --- | --- | --- | --- | --- |
| Airmail | Page border | ring | none | High | R1 |
| Both | Top bar | close X left, title ("New message" / "Reply"), attach glyph, filled Send with paper-plane glyph (44 high, radius 8 / pill) | back arrow, title "Write", attach, more, grey Send without glyph (`ComposePage.qml:137-165`; p21) | High | Close glyph, title by mode, Send glyph; move "more" into a sheet |
| Both | Header fields | rows: label (Carbon mono 12, Airmail Inter 13) at left, value at 14-16, hairline between, no frames | From row uses `ListRow`; Subject is a framed `Field` with label above (`ComposePage.qml:239`; p21) | High | Done by #138: `SubjectRow.qml`, label column and no frame |
| Both | To row | chips + live input, "Cc Bcc" text at the right | separate "Cc and Bcc" outlined chip under the row (p21) | Medium | Done by #138: `trailingText` on `RecipientField.qml` |
| Airmail | Body | Lora about 16, unframed | framed Field (`ComposePage.qml:239`) | High | R4, unframed multi-line area |
| Both | Recipient chips | 6 px (Carbon) / pill (Airmail) tintB chip, name + x | `RecipientField.qml:76-105` chip radius not branched | Medium | R5 |
| Both | Suggestion popup | card, 8 px radius, shadow, name 15/600 plus mono address 12 | `Suggestions.qml:49-71` | Low | R3 |
| Both | Status footer | "Draft saved 09:41" with check; "Plain text . Signature: Personal" | not found (grep "saved" in compose) | Medium | Add status line |
| Both | Quote fold | dashed outlined chip, "Show quoted text, 9 lines", Airmail Lora | `QuoteFold.qml:35` Inter 14, check dashed border | Low | R4 |
| Both | Attachment chip | card with glyph, name 15, size mono 13 | `AttachmentChips.qml:41-53` | Low | R3 |

## Sign-in, wizard and options

| Theme | Element | Board | Built (file:line) | Severity | Suggested fix |
| --- | --- | --- | --- | --- | --- |
| Airmail | Page border | ring | none | High | R1 |
| Both | Step indicator | "STEP 2 OF 3" label and three 4 px segments, two filled | none (`WizardPage.qml`, `Wizard` steps; p01, p14) | Medium | Add progress component |
| Both | Primary action | pinned footer (R8): Continue, Done with check, Cancel outline | in-flow below content (`EmailStep.qml:56,77`, `OptionsStep.qml:92`; p01) | Medium | R8 |
| Both | Provider list | Google / Microsoft / iCloud / Fastmail / Other rows with chevron | email-first autodetect per 10-design.md ("Add account"), so not a deviation | n/a | none |
| Both | Google app-password card | bordered card, bold lead, "Open Google account settings" link glyph | `SignInStep.qml:28-54` plain note; no external-link glyph | Low | add glyph |
| Both | Server settings | "Server settings v" disclosure inside sign-in | separate step with title "Server settings" | Low | accepted; skip |
| Airmail | Stamp colour picker (Options) | Red / Navy / Green 40x48 stamps | missing (`OptionsStep.qml`) | Medium | add with R2, needs a core setting |
| Both | Options selects | "Sync" and "Keep mail" as bordered selects with a down glyph | text rows "Sync / While Envio is open" and "Keep mail for / Last 30 days" with no affordance (p14) | Medium | R6 selects |
| Both | Field style | 48 high (Carbon) / pill-ish 12 radius (Airmail), 1 px field border, label 13 above | 44 high, radius 8 (`Field.qml:36-40`) | Medium | 48; Airmail radius per R5 |
| Both | Trust dialog | red alert glyph, title 20 mono with host on its own line, fingerprint in a tinted mono box at 12, "Issued by . expires" line, stacked buttons | no glyph, title 18 with host inline, plain text, no box, no issuer line (`TrustDialog.qml:28-39`, `AppDialog.qml:43`; p10) | Medium | Box with `selected` fill; add glyph |
| Carbon | Wizard title | Plex Mono 20 | 18 | Low | R7 |

## Microsoft sign-in (#219)

Board: `docs/design/carbon-ms-oauth.html`, frames 1 to 8 (light; dark on the
four frames the board shows in dark). Built in T8: `SignInStep.qml` (the
`microsoft` variant), `BrowserStep.qml`, `NoticeCard.qml`,
`SignInAgainPage.qml`, the Outbox row's held-send text. Checked so far:
layout and behaviour by `cargo make qml-test` against the fake core, and the
end-to-end path by `crates/envio-e2e/tests/oauth_e2e.rs` (no Microsoft
involved). **Not yet audited:** the phone shots with the real flow, which
need the real flow (#219 T10; the client ID is set, the phone test is
open). Until then no deviation row on these frames has been measured on a
phone. T10 runs the audit process above on these frames and adds the rows
here.

## Contacts and settings (Appearance, Sync, Images, About)

| Theme | Element | Board | Built (file:line) | Severity | Suggested fix |
| --- | --- | --- | --- | --- | --- |
| Both | Contacts board | superseded (10-design.md: autocomplete source plus "Add sender to Contacts") | no contacts screen | n/a | none |
| Both | Settings list rows | label 16/500, caption 13 secondary, trailing chevron | no chevron (R6; `SettingsPage.qml:61`) | Medium | R6 |
| Both | Appearance theme cards | 2 rows, each a mini inbox (dot + sender + rule; Airmail shows a chevron border strip), name, caption ("Default. Plain and quiet."), checked = 2 px primary border and a check | abstract bars, name only, border 3 px, 6 px radius, no caption, no check (`AppearancePage.qml:55-90`) | Medium | Match board card |
| Both | Section eyebrow | Carbon mono 11 caps, Airmail Inter 12 caps | mono both (`AppearancePage.qml:102`) | Medium | R3 |
| Both | Toggle | 52 x 30 track, 24 knob | 52 x 30, knob 24 (`Toggle.qml:23-35`), matches | Low | none |
| Both | Radio rows (Sync) | 22 radio, title 16, caption 13, "Set up a relay" link | `RadioRow.qml:35-65` matches; link not checked | Low | none |
| Both | Sync screen copy | "While Envio is open", "Push relay: not set up" | matches (p13 shows the options page) | Low | none |

## Prompts, dialogs and sheets

| Theme | Element | Board | Built (file:line) | Severity | Suggested fix |
| --- | --- | --- | --- | --- | --- |
| Airmail | Frame | ring around phone, sheets inside ring, 26 px sheet corners | `Sheet.qml:25` 26 but no ring (R1) | High | R1 |
| Both | Sheet title | Carbon mono 18, Airmail Lora 20 | `fontTitle` 18 both (`Sheet.qml:52`) | Low | Airmail 20 |
| Both | Dialog corners | Carbon 8-12, Airmail 26 | gu(1.5) = 12 both (`AppDialog.qml:28`) | Medium | R5 radius role |
| Both | Forward prompt | radio list, "Send from" select with chevron, Cancel + Continue side by side | not screened; selects need R6 | Low | R6 |
| Both | Share or export sheet | rows with glyph, "Include attachments" toggle row, Cancel | not screened | Low | audit with phone |
| Both | Print row | dropped (10-design.md), board still shows it | absent (correct) | n/a | none |
| Both | Toast | see R5 and R3 | `Toast.qml:31,65` | Medium | R5 |

## Dark (Carbon and Airmail)

| Theme | Element | Board | Built (file:line) | Severity | Suggested fix |
| --- | --- | --- | --- | --- | --- |
| Airmail | Frame stripes | red `#C4473F`, blue `#5B7FB3` on the dark page (roles exist) | roles unused (R1) | High | R1 reads `chevron` and `chevronBlue` |
| Airmail | Stamp avatar | dark tint fills (`tintA` and `tintB` dark), initials in `accent` dark and `heading` dark | stamp missing (R2) | Medium | R2 |
| Airmail | Primary button on dark | `#A9C1E3` fill, `#141C27` text, pill | `primary` and `primaryText` roles correct; shape (R5) | Medium | R5 |
| Both | Colour roles | palette values | `Theme.qml:44-65` match `10-design.md` | Low | none |
| Carbon | Dark inbox and reader | as light with dark palette | no deviation beyond the light-mode rows | Low | none |
