# Design language: Carbon and Airmail

The look of the app, decided while brainstorming on 2026-11-07 (mockups:
`docs/design/`, spec: `docs/superpowers/specs/2026-11-07-envio-design.md`).
Carbon is the default theme. Airmail is the alternative, picked in Settings.
Both follow the system's light or dark mode.

## Principles

- Ubuntu Touch first: one column, thumb-reachable primary actions at the
  bottom, nothing that fights the system's edge swipes.
- Mail is the content. One accent colour, used for unread, links and the
  few destructive actions.
- Honest status. Envio says when it is offline, when it only syncs while
  open, and when a message is waiting in the Outbox.
- Private by default: remote images blocked, the link's real address shown,
  no read receipts.

## Colour roles

One place in QML, `qml/platform/Theme.qml`, picks a role by theme and by the
system's light/dark flag. The values are ours, not Suru's palette. Where the
Rust core produces coloured text (rich-text conversion of HTML mail), it
repeats these values, since the core is Qt-free.

### Carbon

| Role | Light | Dark | Used for |
| --- | --- | --- | --- |
| ground | `#F8F6F1` | `#15181D` | window, pages, sheets |
| surface | `#FFFFFF` | `#1F242B` | cards, dialogs, fields |
| text | `#1E2A38` | `#E8E5DE` | running text, titles |
| secondary | `#5A5D62` | `#A2A5AA` | previews, captions, labels |
| hairline | `#DAD7CE` | `#2E343C` | rules, row separators |
| field border | `#C9C6BD` | `#3A4049` | text fields, outlined buttons |
| accent | `#B8342C` | `#E36A60` | unread dot, links, destructive |
| primary | `#1E2A38` on `#F8F6F1` | `#E8E5DE` on `#15181D` | primary buttons, selected tab |
| tint A | `#F4E3E1` | `#3A2526` | warning banners |
| tint B | `#E3E6EA` | `#26303B` | info banners |
| selected | `#ECE9E1` | `#232932` | selected rows |
| hover | `#F3F1EA` | `#1B2027` | the list row under the mouse (desktop, two-pane) |
| scrim | `rgba(30,42,56,.45)` | `rgba(0,0,0,.55)` | behind sheets and dialogs |
| chevron, chevron blue | `#5A5D62` | `#A2A5AA` | disclosure chevrons (same as secondary; both roles) |

### Airmail

| Role | Light | Dark | Used for |
| --- | --- | --- | --- |
| ground | `#FBF8F1` | `#141C27` | window, pages, sheets |
| surface | `#FFFFFF` | `#1C2633` | cards, dialogs, fields |
| text | `#22303F` | `#ECE5D6` | running text |
| heading | `#1F3A5F` | `#A9C1E3` | titles, sender names |
| secondary | `#5D6A78` | `#A3AEBB` | previews, captions, labels |
| hairline | `#DDD5C4` | `#2C3747` | rules, row separators |
| field border | `#CFC6B1` | `#3A475A` | text fields, outlined buttons |
| accent | `#B23A33` | `#E58C84` | unread, links, destructive |
| primary | `#1F3A5F` on `#FBF8F1` | `#A9C1E3` on `#141C27` | primary buttons, selected tab |
| chevron (red) / chevron blue | `#B23A33` / `#1F3A5F` | `#C4473F` / `#5B7FB3` | the airmail border |
| tint A | `#F3E2DF` | `#3A2427` | warning banners |
| tint B | `#E0E7F0` | `#1F2D42` | info banners |
| selected | `#F1EADB` | `#1F2A38` | selected rows |
| hover | `#F7F2E8` | `#18222E` | the list row under the mouse (desktop, two-pane) |
| scrim | `rgba(34,48,63,.45)` | `rgba(0,0,0,.55)` | behind sheets and dialogs |

Contrast is checked against WCAG AA by `tests/qml/tst_theme.qml`: text,
heading, secondary and accent on ground, and primary text on primary, reach
4.5:1; the chevron roles on ground reach 3:1 (graphics). A value that fails is
adjusted here first. `Theme.qml` has the roles plus `name` ("carbon" or
"airmail"), `darkMode` ("system", "light" or "dark"), the injectable
`systemDark`, and the fonts `fontUi`, `fontMono` and `fontTitle`.

The paper card: HTML mail whose sanitised markup sets a background or a dark
text colour (the core decides, `ownColours`) is drawn on the light theme's
surface in both themes, with the light theme's text and link (`Theme.paper`,
`paperText`, `paperLink`), because its colours assume a white page. Plain-text
mail, raw mail and HTML that sets no colour keep the theme's card, so dark mode
stays dark for them. In light mode nothing changes.

## Type

| | Carbon | Airmail |
| --- | --- | --- |
| Body and reading | Inter | Inter (lists), Lora (reading pane) |
| Labels, headers, buttons | IBM Plex Mono | Lora (headers), Inter (buttons) |

On the phone the fonts ship in the click (all OFL). Ubuntu is the fallback.

## Shapes

- Carbon: 8 px corners, hairline rows, flat surfaces.
- Airmail: 26 px corners on the phone frame and sheets, a red-and-blue
  chevron border around the page.
- Both: bottom sheets with a grab handle; one full-width primary button at
  the bottom of a page.

## Views

| View | Boards | Notes |
| --- | --- | --- |
| Inbox and accounts | `*-inbox` | Unified inbox, per-account tabs (All, then each account), unread dot, "New message" at the bottom. |
| Add account | `*-wizard` | Email first, autodetect, then password; manual server settings behind "Set up manually". |
| Account label | `carbon-account-label` | Accounts action sheet with Rename, the rename sheet (label field, "Leave empty to show the default.", Cancel and Save), inbox tabs and drawer heading showing the label. Carbon only (#217). |
| Sign-in and trust | `*-signin` | Google app password steps; Microsoft in the browser (built out in `carbon-ms-oauth`); self-signed certificate with SHA-256 fingerprint, Cancel as the primary button. |
| Microsoft sign-in | `carbon-ms-oauth` | Outlook.com and Microsoft 365 through the system browser (#219), light (dark on four representative frames): the wizard step with "Sign in with Microsoft", waiting for the browser (Cancel), cancelled, failed, the "You signed in as X. Use this account?" question, the "Sign in again" account state (status line, action sheet, page), a held Outbox message, and the no-client-ID state that keeps #221's "not supported yet" message. Carbon only. No new colour role: banners use tint A and tint B. |
| Compose | `*-compose` | New, reply (quoted), from a `mailto:` link; share-in board is 0.3.0. Drafts save on their own. |
| Folders, search, selection | `*-folders` | Folders menu with Outbox; search with "Search on server"; long-press to select, actions in the top bar. |
| Reopen a draft | `carbon-draft-edit` | Reopening a saved draft (#228): Drafts list with the "Not uploaded" mark (#266, tint A), the reader of a Drafts-role message with one "Edit" button (no Reply, Reply all, Forward; Delete stays), and Compose filled from the draft, saving back over the same draft. Carbon only. No new colour role. |
| Reading | `*-reading` | HTML mail with remote images blocked; attachment sheet (Open, Save, Share through Content Hub); offline icon on the account tab and Undo toast. The meeting invite card (0.2.0) sits above the body, in all its states, light and dark. |
| Contacts | `*-contacts` | Superseded in part, see below. |
| Prompts and settings | `*-prompts` | Sync reads "While Envio is open" and "Push relay — not set up". Settings → Notifications (0.2.0) sits at the foot, light. |
| Two panes | `carbon-two-pane` | Tablet and desktop (#192): list left, reader or compose right (wide from 90 gu), multi-select, empty reader, optional drawer column, the breakpoint, hover, right-click menu, shortcut legend, dark rows, Airmail frame (hover value for all four palettes). The open row is the selected fill plus a 3 px accent bar (not multi-select). New: reader Close, empty state, hover role, context menu, shortcuts dialog. |
| Dark | `*-dark` | Inbox, reader and sync settings in each dark theme. Settings → Notifications (0.2.0) sits at the foot, dark. |
| App icon | `*-icons` | Chosen: Airmail 2, the postage stamp with envelope, for both themes. |

## Interaction rules

- **No swipe actions on rows.** Ubuntu Touch uses edge swipes for the
  launcher (left) and app switcher (right). Actions come from long-press
  selection and the reader's buttons.
- **Undo instead of confirm.** Archive, delete and move show an Undo toast
  for about 5 s before they reach the server.
- **Offline is an icon, not a dialog.** An offline account shows a cloud-off
  icon on its tab and its drawer row. The Outbox banner says how many
  messages wait.
- **Errors sit with their item.** A refused send shows on its Outbox entry
  with Edit and Retry.

## Changes to the first boards

From the canvas notes; these win over what the boards show.

- Contacts is not an address book. It is an autocomplete source (people you
  wrote to, later the system address book) plus "Add sender to Contacts".
- Print is dropped (no printing on Ubuntu Touch). Export as PDF stays, later.
- Sync "Push, when available" reads "While Envio is open" until a push relay
  exists.
- No swipe actions on list rows (see above).
- Share into Envio is 0.3.0; `mailto:` links are 0.1.0.
