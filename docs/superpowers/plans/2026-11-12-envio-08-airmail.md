# Envio Plan 8a: the Airmail look Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Airmail theme look like its boards on the phone: the font roles, pill controls, the red-and-blue airmail ring around every page and sheet, and stamp avatars instead of the unread dot. Then the smaller design-drift items: chevrons, top bar, pinned footers and link colour.

**Architecture:** QML only, plus one CI lint. Every visual difference between Carbon and Airmail goes through a `Theme` role (font, radius, inset, size), so pages and components never test `theme.airmail` for a literal value. The only places that test `theme.airmail` directly are the components that exist only in one theme (`AirmailFrame`, the stamp shape in `Avatar`). Each task adds or changes roles in `qml/platform/Theme.qml`, then moves the components to them, then adds QML tests for both themes.

**Tech Stack:** Qt 5.15 / QtQuick 2.15 (Ubuntu Touch 24.04), qmltestrunner offscreen (`cargo make qml-test`), fonts Inter, IBM Plex Mono and Lora (bundled, SIL OFL 1.1). No Rust changes, so the Rust crates and `qt/` stay untouched.

**Spec:** `docs/superpowers/specs/2026-11-07-envio-design.md` ("Look"). Look: `docs/10-design.md` (roles, overrides), the boards `docs/design/airmail-*.html` and `docs/design/carbon-*.html` (exact values: read the inline CSS of the board named in each task). Issues: #67, #68, #65, #66 (must), then #69, #70, #71, #74 (could). Out of scope: #72 (backlog), #73 and #76 (behaviour, not look).

## Global Constraints

- Theme roles only: pages and components never hard-code a colour, font family, radius or inset. They read `Theme`. The existing CI lint (`tools/ci/`, run by `cargo make app-docker`) is extended per task to enforce the new roles.
- Carbon must not change: each task's tests check the Carbon value stays what it was, light and dark.
- Every QML `Text` states `textFormat` (lint, from Plan 7).
- Touch targets at least 44 px high (`Theme.touch`); sizes through `Theme.gu(n)`.
- QML never formats a count (lint #3). User-facing text is `qsTr()`; run `cargo make pot` after changing strings (this plan should add none).
- No swipe actions on list rows.
- Files around 300 lines; a new visual piece is its own file under `qml/components/`.
- Hosts, link parts and the fingerprint stay monospace in both themes, through `Theme.fontAddress`; screens never read `fontMono`.
- Each task runs `cargo make qml-test` and `cargo make app-docker`; `cargo make ci` still passes at every commit.
- Each must task ends with a phone screenshot pair (Carbon, Airmail) of a screen it changed, taken with `tmp/u.sh shot`, kept in `docs/phone-runs/airmail/` and named in the task report. Never hand a visual check back to the user.

## Plan series

7. UI for 0.1.0 (PR #77). **8a. The Airmail look** (this plan). 8b. Release 0.1.0: the phone checklist on a clean install, budgets, store listing, tag.

---

### Task 1: Font roles (#67)

**Files:** `qml/platform/Theme.qml`; the files #67 lists (`DateText.qml:27`, `MessageRow.qml:117`, `FoldersDrawer.qml:82,93`, `HeaderBlock.qml:56,66`, `AttachmentList.qml:19,59`, `Suggestions.qml:59`, `AttachmentChips.qml:53`, `Toast.qml:65`, `SearchPage.qml:124`, `AppearancePage.qml:102`, `ReaderPage.qml:178`, and the compose body); `tests/qml/tst_theme.qml`; the lint in `tools/ci/`.

**Produces (Theme):**
- `fontLabel: string`: Carbon `fontMono`, Airmail `fontUi`.
- `fontBody: string`: Carbon `fontUi`, Airmail Lora (fallback `fontUi` if Lora fails to load).
- `bodySize: real` (Carbon 16, Airmail 16) and `bodyLineHeight: real` (Carbon 1.0, Airmail 1.6).
- `eyebrowFont: font`, or the fields `eyebrowFamily`, `eyebrowSize`, `eyebrowWeight`, `eyebrowCaps: bool` and `eyebrowSpacing: real`: Airmail Inter 12 semibold, capitals, letter-spacing 1.2 px; Carbon the current label style (mono, as now).
- `fontMono` stays, read by `Theme.qml` only; `fontAddress` (= `fontMono`) is what screens use for addresses, hosts, link parts and the fingerprint.

**Invariants:** after this task no file under `qml/` except `Theme.qml` reads `fontMono`; screens use `fontAddress` for addresses and `fontLabel` for labels. Reader body, compose body and the quote fold use `fontBody`, `bodySize` and `bodyLineHeight`.

**Tests (tst_theme / tst_reader / tst_compose):**
- In Airmail, `fontLabel` is not `fontMono` and `fontBody` is Lora's family. In Carbon, `fontLabel === fontMono` and `fontBody === fontUi`.
- A tree walk: build MessageRow, HeaderBlock, AttachmentList, Toast and DateText under an Airmail theme and check that no visible `Text` has `font.family === theme.fontMono`. Build TrustDialog and check that its fingerprint `Text` uses `fontAddress`.
- The reader body `Text` takes `theme.fontBody` and its `lineHeight` from `bodyLineHeight`.
- Lint: `fontMono` (dotted or `theme["fontMono"]`) outside `Theme.qml` fails. Show the lint failing on a scratch copy that keeps one old use.

### Task 2: Control radius (#68)

**Files:** `Theme.qml`; `AppButton.qml`, `Toast.qml`, `Chip.qml`, `Field.qml`, `AppDialog.qml`, `Sheet.qml`, compose's Send button; `tst_components.qml`; the lint.

**Produces (Theme):**
- `controlRadius: real`: Airmail full pill (`controlHeight / 2`, 26 at 52 high), Carbon 8.
- `controlHeight: real`: Airmail 52, Carbon the current height (keep ≥ `touch`).
- `controlFont`: Airmail Inter 15 semibold, Carbon as now.
- `fieldRadius`, `chipRadius`, `dialogRadius`, `sheetRadius`: values from the boards (`airmail-prompts.html` and `airmail-compose.html` for the dialog, sheet, chip and field; `carbon-*` for the Carbon values, which must equal what the components use today).

**Invariants:** no `radius:` literal in `qml/components/` or `qml/pages/` except `radius: width / 2` style circles (dots, toggles, Carbon avatars). The Toast is a pill in Airmail.

**Tests:** per theme, AppButton and Toast `radius` equal `theme.controlRadius`, AppButton's height equals `controlHeight`, and Carbon values are unchanged (8 for buttons, the old toast value). Lint: a numeric `radius:` in components or pages fails, except `width / 2` and `height / 2` forms. Show it failing on a scratch copy.

### Task 3: The airmail ring (#65)

**Files:** new `qml/components/AirmailFrame.qml`; `Theme.qml`; `AppShell.qml` (around the page stack); `tst_components.qml`, `tst_shell.qml`.

**Produces:**
- Theme: `frameInset: real` (Airmail 10, Carbon 0), `frameOuterRadius` (30), `frameInnerRadius` (20). The stripe colours are the existing `chevron` (red) and `chevronBlue` roles, with `ground` as the paper.
- `AirmailFrame { required property Theme theme; default property alias content }`: draws the ring only when `theme.airmail`, and lays its content out inside `frameInset`. In Carbon it adds no inset and draws nothing.

**Tricky part, the stripes.** The board's ring is `repeating-linear-gradient(135deg, red 0–14, paper 14–28, blue 28–42, paper 42–56)` in a 10 px band between radius 30 and 20. QML 2.15 has no repeating gradient. Draw it with a `Canvas` (`onPaint`: clip to the rounded ring path, outer rounded rect minus inner, then fill diagonal stripes 14 px wide at 135° in the order red, paper, blue, paper across the bounding box). Repaint on `width`, `height` and theme colour changes. Set `renderTarget: Canvas.FramebufferObject` only if the phone shows tearing. Keep the content in a separate Item above the Canvas, clipped to the inner radius, so pages scroll under nothing.

**Invariants:** in Airmail the ring is drawn once, in `AppShell`, around the page stack; `Sheet` and `AppDialog` draw none (never a ring inside a ring), so a Sheet or dialog opens inside the shell's ring. Content is never clipped by the band: the page's area is `frameInset` smaller on each side. Carbon layout is pixel-identical to before (inset 0, no Canvas visible).

**Tests:** Airmail: the frame's Canvas is visible, and the content item's `x` and `y` equal `frameInset`. Carbon: the Canvas is not visible and the content fills the frame (`x === 0`). The shell's current page width equals the window width minus `2 * frameInset`. A Sheet opened in Airmail draws no frame of its own. Phone shots of the inbox and a sheet in Airmail light and dark.

### Task 4: Stamp avatars, no unread dot in Airmail (#66)

**Files:** `qml/components/Avatar.qml` (and a new `Stamp.qml` if Avatar grows past ~120 lines); `MessageRow.qml`; the search rows, the reader header (`HeaderBlock.qml`), the accounts rows and the outbox rows; `Theme.qml`; `tst_components.qml`, `tst_inbox.qml`, `tst_search.qml`, `tst_outbox.qml`.

**Produces:**
- Theme: `stampWidth` (40) and `stampHeight` (48) for Airmail; `avatarSize` for the Carbon circle (keep the current size); `showUnreadDot: bool` (Carbon true, Airmail false); `unreadWeight` (Airmail `Font.DemiBold`, i.e. 600; Carbon as now).
- `Avatar` in Airmail: a perforated stamp. From the board, the edge is a mask of 3 px circular holes every 8 px around the border (`radial-gradient(circle at 4px 4px, transparent 3px, #000 3.3px) -4px -4px/8px 8px`), with the tint `tintA` (red) or `tintB` (blue) and Lora initials in `accent` or `heading`. Draw it with a `Canvas`: fill the rect, then `globalCompositeOperation = "destination-out"` circles of radius 3 centred on the edge every 8 px. In Carbon it stays the circle.
- The tint choice is stable per sender: `blue` comes from a hash of the address the caller already has, not from row position. Keep the hash in JS in `Avatar` callers or a tiny `qml/platform/stampTint.js` (pure, tested); no new Rust.

**Invariants:** Airmail rows show the stamp and no dot, and unread rows have the sender at `unreadWeight`. Carbon rows are unchanged (dot shown, current avatar or none, as the Carbon board has it). Initials come from the caller, as now.

**Tests:** for MessageRow in each theme: the dot is visible only in Carbon, the stamp only in Airmail, and an unread row's sender has `font.weight === theme.unreadWeight`. `stampTint.js`: the same address always gives the same tint, and two known addresses give red and blue. Avatar in Airmail has the size `stampWidth × stampHeight`. Phone shots of the inbox, reader header and outbox in Airmail.

### Task 5: Disclosure chevrons (#69)

Read #69. Add a `Chevron` component (Theme glyph size `iconSmall`, colour `secondary`) and use it on settings rows, folder groups and select rows, in both themes as the boards show. Tests: the rows that #69 names show a visible chevron and the rows that act at once (toggles) don't.

### Task 6: Top bar and compose bar (#70)

Read #70. Theme roles `barHeight` and `barTitleFont` from the boards; the compose bar gets a close glyph on the left and a Send glyph button on the right (the Send button keeps the Task 2 pill in Airmail if the board shows one). Tests: the bar height equals `theme.barHeight` in both themes, and compose's close and Send buttons exist with targets ≥ `touch`.

### Task 7: Pinned footers (#71)

Read #71. A `PageFooter` component that pins the primary action to the bottom of the wizard, options and sign-in pages, above the on-screen keyboard (it must not regress #61: the field being typed in stays visible). Tests: the footer's button stays at the page bottom when content is short and when it scrolls, and the focused field is not covered by the footer when the keyboard's height is simulated.

### Task 8: Link colour (#74)

Read #74. A Theme role `link` (from the boards: Airmail `heading` blue, Carbon `accent`, or the board's value), passed to the rich-text body as the anchor colour. This is the HTML-to-Qt-rich-text path, so if the colour must go into the generated markup, add it through the existing view context in `envio-mime` and say so in the report (that makes the task trunk). Prefer a QML-only fix (`linkColor` on the `Text`) if Qt honours it for `<a>` in the rich text. Tests: the reader body's `linkColor` equals `theme.link` in both themes.

## Done when

- #65–#68 closed by this plan's PR with phone shots, and #69, #70, #71, #74 closed or re-milestoned with a reason.
- `docs/phone-checklist.md` "Dark mode" row re-run in Airmail light and dark against the boards.
