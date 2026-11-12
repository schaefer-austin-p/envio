# Envio Plan 8b: daily use on a real account Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix what using envio on a real mail account turned up: newsletters and receipts that take over 10 s to open and render as a one-character column (#88), no way to reach mail older than the 30-day window (#89), a wizard that looks frozen while it works (#87), and the smaller list, search, trust-prompt and link-sheet faults (#73, #63, #62, #76).

**Architecture:** Same layering as before: logic in Rust, QML only shows it. #88 is fixed in `envio-mime` (table and cell widths that Qt's rich-text layout understands), proved by a synthetic corpus file in both the Rust snapshot tests and the QML layout test. #89 adds a pure planner (`plan/older.rs`) and a driver step in `envio-sync`, a `LoadOlder` command and `Step::Older` in `envio-core`, an `OlderRow` state in `envio-ui`, and a footer row in QML; the sync window becomes a setting in the settings file (no migration). The rest are small core text changes (placeholders, service names, the link host) plus QML fixes.

**Tech Stack:** Rust (envio-mime ammonia + html5ever, envio-sync, envio-core, envio-ui), cxx-qt in `qt/`, Qt 5.15 / QtQuick 2.15 on Ubuntu Touch 24.04, qmltestrunner offscreen, Stalwart in Docker for `cargo make e2e-it`.

**Spec:** docs/superpowers/specs/2026-11-07-envio-design.md ("Sync", "Reading", "Accounts", "Testing"). Issues: #88, #89, #87 (must), #73, #63, #62, #76.

## Global Constraints

From `CLAUDE.md` and the spec, verbatim where possible:

- Logic in Rust, Qt and QML only for UI and platform APIs. The core is Qt-free; `unsafe` only in `qt/`.
- Dependencies point one way: ui → core → sync → net, store, mime. net, store and mime never depend on each other.
- One concept per module, files around 300 lines, logic kept out of I/O code (pure planners, thin drivers).
- Migrations are numbered and never edited after merge. (This plan adds none; if an implementer finds it needs a column, it is a NEW numbered migration.)
- Unsent work (Outbox, drafts, pending actions) lives outside the SQLite cache and is saved within about 1 s.
- No swipe actions on list rows (Ubuntu Touch edge gestures). "Load older" is a plain tap row.
- User-facing text is translated: `qsTr()` in QML, gettext in Rust, domain `envio.aps`. Run `cargo make pot` after changing strings. User-facing error text is chosen and translated in Rust.
- Single-crate integration tests go in `crates/<crate>/tests/`; anything that needs two crates goes in `envio-e2e`.
- Spec: one transaction per sync step; bodies are fetched on open; logs mask addresses and subjects (no personal data in any timing log this plan adds).
- Every QML `Text` states `textFormat`; colours, fonts and sizes come from `Theme` roles (Plan 8a lint); QML never formats a count.
- Private mail never enters the repo: the two real slow messages in the controller's `tmp/dev88/` are not committed and not quoted; corpus files are synthetic look-alikes.
- Each task runs the proving commands it names; `cargo make ci` passes at every commit.

## Plan series

7. UI for 0.1.0. 8a. The Airmail look (PR #86). **8b. Daily use on a real account** (this plan). 8c. Release 0.1.0: the phone checklist on a clean install, budgets, store listing, tag.

---

### Task 1: Receipts and newsletters open fast and lay out (#88)

**Known (controller measured, don't redo):** Rust is not the bottleneck: envio-mime parse + render takes about 1.5 ms (33 KB, 38 tables) and 2.3 ms (153 KB Stripe receipt) in release. On the phone the receipt renders as a one-character-wide column ("Re/cei/pt"), the app reaches VmRSS 1.4 GB / VmHWM 2.0 GB after opening it. The source sets widths in CSS (`width: 32px`, `width: 16px`, `width: 480px`, `width: 100%`) on `table`/`td`, with few `width=` attributes; `style::filter_style` keeps CSS `width` only on `img`, so the sanitised output keeps no widths except `width="100%"`, and Qt's table layout gives the text columns almost nothing. Hypothesis: the >10 s is Qt laying out and painting a huge, very tall document made by collapsed columns.

**Files:**
- Create `crates/envio-mime/tests/corpus/receipt-like.eml`: synthetic, Stripe-receipt-shaped (outer `width: 100%` table, centred `width: 480px` card, `width: 32px` and `width: 16px` spacer cells either side of text cells, nested 3–4 tables deep, an amount table, footer links, two small dummy PDF parts). Invented names and amounts only. Also `newsletter-like.eml`: LinkedIn-like, 40+ tables, tracking links, many remote images, CSS widths only.
- Create `crates/envio-mime/src/widths.rs` (CSS width → attribute; under 150 lines).
- Modify `crates/envio-mime/src/style.rs` (keep `width` in the style for `table`, `td`, `th`, `col` so the width pass can read it), `crates/envio-mime/src/images.rs` (the `table | td | th` arm calls `widths::carry` before `limit_width`; add `col`), `crates/envio-mime/src/sanitize.rs` (allow `width` on `col`, if not already), `lib.rs` (`mod widths`).
- Modify `crates/envio-mime/tests/corpus.rs` (two snapshot tests), new `crates/envio-mime/tests/widths.rs`.
- Modify `tests/qml/tst_rich_text.qml` (height and time bounds for the two files), `tests/qml/tst_reader.qml` (loading state).

**Produces:**
- `pub(crate) fn carry(tag: &mut Tag, max: u32)` in `widths.rs`: reads `width` from the tag's `style`; if it is `Npx` (N integer > 0) or `N%` (0 < N <= 100) and the tag has no `width` attribute, sets `width="N"` / `width="N%"`. A pixel width above `max` (when `max > 0`) becomes `width="100%"` (not dropped: a 600 px card on a 360 px screen should fill it). Removes `width` from the style it moved (Qt ignores it there). Any other unit (`em`, `auto`, `calc`) is left alone and not carried.
- Order inside the `table | td | th | col` arm: `carry`, then `limit_width` (existing; now only ever sees pixel widths `<= max` from `carry`, but still guards `width=` attributes from the source).
- `col` added to the arm and to the allowed tags if missing (`<col style="width:32px">` is common in receipts).

**Invariants:**
- Spacer cells keep their small pixel widths (they are what keeps text columns apart).
- Nothing gets wider than `max_image_width`; a percentage never exceeds 100.
- An explicit `width=` attribute wins over CSS (the source author set both; keep the attribute).
- No change for messages without CSS widths: every existing corpus snapshot stays byte-identical except where a CSS `width` on a table/cell existed (review each changed snapshot in the diff).

**Steps:**
- [ ] 1. Add the two corpus files and snapshot tests `receipt_like` / `newsletter_like`; run `cargo test -p envio-mime --test corpus` and look at the rendered output to confirm it reproduces the fault (no widths on the spacer cells).
- [ ] 2. Measure in the CI image before the fix: extend `tst_rich_text.qml` with `test_receipt_like_layout` that loads `corpus-receipt-like.html` into a `Text` (RichText, Wrap) at width 360 and `console.info`s the layout ms and `implicitHeight`. Run `cargo make qml-test`; record both numbers in the task report. Expected: implicitHeight many times the screen-height of the desktop render (one-character column).
- [ ] 3. Write `crates/envio-mime/tests/widths.rs` (below, failing), then `widths.rs` and the wiring.
- [ ] 4. Re-run step 2's measurement; record after-numbers. If layout time is still above 1 s at 360 px, read Qt's remaining hot spot from the timing of smaller cut-down files (drop tables one level at a time) and say so in the report; don't guess a second fix.
- [ ] 5. `cargo make pot` is not needed (no strings). Run the proving commands.

**Tests (`crates/envio-mime/tests/widths.rs`):**
- `css_pixel_width_on_a_cell_becomes_the_width_attribute`: `<td style="width:32px">` → output `td` has `width="32"`, style has no `width`.
- `css_percent_width_on_a_table_becomes_the_width_attribute`: `width: 100%` → `width="100%"`.
- `pixel_width_wider_than_the_screen_becomes_full_width`: `max_image_width` 360, `width: 480px` → `width="100%"`.
- `width_attribute_wins_over_css`: `<td width="20" style="width:40px">` → `width="20"`.
- `other_units_are_not_carried`: `width: 10em`, `width: auto`, `width: 150%` → no `width` attribute.
- `col_keeps_its_width`: `<col style="width:16px">` → `<col width="16">`.
- `receipt_like_keeps_its_spacer_and_card_widths`: render `corpus/receipt-like.eml` at 360; every spacer `td` has a pixel width <= 32, no `width` attribute exceeds 360 px.
- `rendering_scales_linearly`: render `newsletter-like` body repeated ×1 and ×2 (best of 3 each); time ×2 < 3 × time ×1, and ×1 under 2 s absolute (generous: `cargo make ci` runs instrumented).

**Tests (QML):**
- `tst_rich_text.qml::test_receipt_like_layout` and `test_newsletter_like_layout`: `timed(name)` (< 1000 ms at width 360) and `implicitHeight < 20 * 640` (the bound is chosen from the step-4 measurement with at least 3× headroom; write the number and its source in a comment).
- `tst_reader.qml::test_loading_shows_until_the_view_arrives`: MockCore with no view and empty `unavailable` → the "Loading…" item is visible and `bodyText` is not; after setting a view, the reverse. (The item exists; Task 5 restyles it.)

**Proving:** `cargo make ci`, `cargo make qml-test`, `cargo make app-docker`.

**Phone (controller):** opens the real receipt and the Link notice; time to open and VmRSS/VmHWM before and after (Task 8).

**Done when:** the synthetic receipt lays out at 360 px in under 1 s with a bounded height in the CI image, and the controller confirms on the phone that both real messages open in about 1 s with columns, not a one-character strip.

---

### Task 2: Sync window setting and older messages in envio-sync (#89)

**Files:**
- Modify `crates/envio-sync/src/plan/sync.rs` (window from settings, `kept_floor`), create `crates/envio-sync/src/plan/older.rs`, create `crates/envio-sync/src/driver/older.rs`, modify `driver/folders.rs`, `server.rs` (trait), `crates/envio-sync/tests/support/fake.rs`.
- Modify `crates/envio-net/src/imap/ops.rs` or create `imap/window.rs` if ops.rs passes ~330 lines; `imap/query.rs`.
- Modify `crates/envio-store/src/userdata.rs` (`Settings.sync_window_days`).
- Modify `crates/envio-core/src/worker/store.rs` (`sweep_bodies` uses the setting).
- Create `crates/envio-e2e/tests/older_stalwart.rs`.

**Produces:**
- `envio_store::Settings { …, #[serde(default = "thirty")] sync_window_days: u32 }`. Values 30, 90, 365, 0 (= All). Any other number read from the file snaps to 30 (lenient: an odd value must not move the whole settings file to `.damaged`). `pub fn sync_window(&self) -> Option<u32>` (`None` = All).
- `plan::sync::window_start(now_ms: i64, days: Option<u32>) -> Option<i64>` (`None` → no SINCE: search ALL).
- `FolderView { …, kept_floor: Option<u32> }`. Keep rule in `plan_folder`: a cached UID outside the window is kept (not in `remove`) iff `kept_floor = Some(f)` and `f <= uid < min(window)`. Kept rows are not in `check` and not in `flags` (not re-synced for flags).
- net: `ImapSession::search_all() -> Result<Vec<u32>>` (`UID SEARCH ALL`) and `search_uids_below(uid: u32) -> Result<Vec<u32>>` (`UID SEARCH UID 1:<uid-1>`; returns empty without a round trip when `uid <= 1`). `MailServer` gains both.
- `plan::older::plan_older(below: &[u32], cached: &BTreeSet<u32>) -> OlderPlan { fetch: Vec<u32> /* <= BATCH, newest first */, exhausted: bool }`. `exhausted` iff after this batch no server UID below the oldest fetched remains uncached.
- `driver::older::load_older(server, local, folder: FolderId) -> Result<Older, SyncError>`, `Older { fetched: Vec<MessageId>, floor: u32, exhausted: bool }`. `below` = smallest cached UID in the folder, or `uid_next` if none. Refuses (returns `SyncError::Reset`, like `search_server`) on a UIDVALIDITY change. One transaction per batch via the existing `store_summaries`.
- `sync_folder(server, local, id, now_ms, window: Option<u32>, floor: Option<u32>)` (two new parameters; `Synced` unchanged).

**Invariants:**
- In-window server deletions are still removed even with a floor.
- Older rows deleted on the server stay until the floor goes (app restart, window change or folder reset); known and accepted.
- With window All, `remove` is only server deletions, and the body sweep prunes nothing by date.
- Known limit, written in the plan and the issue: an old-dated message with a high UID (moved in later) is not reached by Load older; server search still finds it.

**Tests:**
- `crates/envio-sync/src/plan/older.rs` unit: `older_yields_uids_below_the_oldest_cached_in_batches` (120 below → 50, 50, 20 + exhausted), `older_skips_uids_already_cached`, `older_is_exhausted_when_nothing_is_below`.
- `plan/sync.rs` unit: `rows_above_the_floor_survive_the_window`, `rows_below_the_floor_are_removed`, `in_window_deletions_are_removed_with_a_floor`, `kept_rows_are_not_flag_checked`, `window_all_searches_everything`.
- `envio-store` tests: `sync_window_defaults_to_thirty`, `odd_sync_window_snaps_to_thirty`, `settings_without_the_field_load` (old file).
- `envio-sync/tests/` driver (fake server): `load_older_fetches_newest_first_in_one_transaction_per_batch`, `load_older_refuses_on_uidvalidity_change`.
- `envio-net` (Stalwart, `cargo make e2e-it`): `search_uids_below_returns_lower_uids`, `search_all_returns_every_uid`.
- `envio-e2e/tests/older_stalwart.rs`: seed 120 messages spread over 89 days with `append_at`. `first_sync_caches_every_message_in_the_window` (counts the ones dated within 30 days: all of them, more than 10; this is the "separate bug" check from #89); `load_older_fetches_50_then_the_rest_then_is_exhausted` (about 80 outside the window: 50, then the rest with `exhausted`); `a_sync_with_the_floor_keeps_everything`; `a_sync_without_the_floor_goes_back_to_the_window`; `a_90_day_window_caches_everything`.

**Proving:** `cargo make ci`, `cargo make e2e-it`.

**Done when:** the planner, driver and e2e tests pass; if `first_sync_caches_every_message_in_the_window` fails, that is a separate bug: fix it in this task if the cause is in the planner, otherwise file it as a new issue (moscow label + milestone) and say so in the report.

---

### Task 3: LoadOlder in the core and the list state (#89)

**Files:**
- Create `crates/envio-core/src/worker/older.rs`; modify `worker/mod.rs` (`floors: BTreeMap<FolderId, u32>`, `Wants.older: BTreeSet<ListRef>`), `worker/plan.rs` (`Step::Older(FolderId)` after `Body`, before `Replay`), `worker/run/mod.rs` and `run/sync.rs` (pass window and floor to `sync_folder`; drop the floor when `Synced.reset`), `api/command.rs`, `api/event.rs`, `dispatch.rs`, `text/read.rs`, `worker/store.rs` (settings change).
- Modify `crates/envio-ui/src/messages.rs`, `signals.rs`, `convert.rs`; create `crates/envio-ui/src/older.rs` if `messages.rs` would pass ~300 lines.

**Produces:**
- `Command::LoadOlder { list: ListRef }`. For `ListRef::Inboxes` it loads one batch for each account's inbox (one `Step::Older` each), events aggregated into one.
- `Event::OlderLoaded { list: ListRef, fetched: usize, exhausted: bool }`, `Event::OlderFailed { list: ListRef, text: String }`; then the usual `MessagesChanged` so the list refreshes.
- `text::read::older_unavailable() -> String` ("Couldn't load older messages. Try again when you are online.").
- Floor: set to `Older.floor` after each batch; dropped on app start (in memory only), when `sync_window_days` changes (also `wants.full` for every account), and on folder reset.
- `envio_ui::OlderRow { Hidden, Ready, Loading, Done }`; `Ui::load_older()` sends the command and sets `Loading`; `Signal::OlderRow(OlderRow)`. `Ready` when the list's `end` is reached and the list is a folder or Inboxes (not search); `Done` after `exhausted`; failure → `Ready` plus the existing error toast signal with `text`. Opening another list resets to `Hidden`.

**Tests:**
- `envio-core` worker tests (fake server, existing harness): `load_older_fetches_a_batch_and_reports_it`, `load_older_on_inboxes_asks_every_account`, `a_sync_after_load_older_keeps_the_older_rows`, `changing_the_window_drops_the_floor_and_resyncs`, `load_older_offline_reports_older_unavailable`.
- `envio-ui`: `older_row_is_ready_at_the_end_of_a_folder`, `older_row_is_hidden_for_search`, `load_older_shows_loading_then_done_when_exhausted`, `older_failure_returns_to_ready`.

**Proving:** `cargo make ci`, `cargo make pot`.

---

### Task 4: Load older row and sync window setting in the app (#89)

**Files:** `qt/src/core_object.rs` (property `olderRow: i32`, invokable `loadOlder()`, property `syncDays: i32`, invokable `setSyncWindow(days: i32)`), `qt/src/events.rs`, `qt/src/convert.rs` (`SettingsEdit` gains `sync_window_days`), `qt/src/calls.rs`; create `qml/pages/inbox/LoadOlderRow.qml`; modify `qml/pages/inbox/MessageList.qml` (ListView `footer`), `qml/pages/settings/SyncPage.qml`; `tests/qml/MockCore.qml`, `tests/qml/tst_inbox.qml` (or the list test file), `tests/qml/tst_settings.qml`.

If `core_object.rs` passes ~300 lines, move the settings properties into a new `qt/src/settings_props.rs` (or the existing split the file uses).

**Produces:**
- `LoadOlderRow.qml { theme, state: int }`: `Ready` → a full-width tap row "Load older messages" (touch height `Theme.touch`, no swipe); `Loading` → the Task 5 `BusyLine` ("Loading older messages…"), not tappable; `Done` → plain secondary text "No older messages"; `Hidden` → height 0.
- SyncPage: the per-account "Last 30 days" text (`keepFor-<key>`) becomes one global choice of four `RadioRow`s, objectNames `window-30`, `window-90`, `window-365`, `window-all`, labels "Last 30 days", "Last 90 days", "Last year", "All mail". Decision: global, not per account (one setting in the file; per account can come later).

**Tests (QML):**
- `test_older_row_ready_taps_load_older` (MockCore counts `loadOlder` calls), `test_older_row_loading_is_not_tappable`, `test_older_row_done_shows_text`, `test_older_row_hidden_has_no_height`, in Carbon and Airmail.
- `test_sync_window_choice_calls_set_sync_window` (each radio → days 30/90/365/0), `test_sync_window_shows_the_current_choice`.

**Proving:** `cargo make qml-test`, `cargo make app-docker`, `cargo make pot`.

---

### Task 5: Busy indicator in the wizard and elsewhere (#87)

**Files:** create `qml/components/BusyLine.qml`; modify `qml/components/PageFooter.qml`, `qml/pages/accounts/EmailStep.qml`, `ServersStep.qml`, `SignInStep.qml`, `qml/pages/reader/ReaderPage.qml` (Loading…), `qml/pages/search/SearchPage.qml` (`searchBusy`), `qml/pages/inbox/LoadOlderRow.qml`; `tests/qml/tst_wizard.qml`, `tests/qml/tst_components.qml` (or a new `tst_busy_line.qml`).

**Produces:**
- `BusyLine.qml { property var theme; property bool running; property string text }`: a spinner coloured from `Theme` (accent role; secondary for the text), and a `Text` (`textFormat: Text.PlainText`). `visible: running`.
- `PageFooter` gains `busyText: string`; while `busy`, the BusyLine replaces the primary button (same height, so the layout doesn't jump).
- Status text per step: EmailStep "Looking up settings…", ServersStep "Connecting…", SignInStep "Signing in…".
- Reader "Loading…", search "Searching the servers…" and LoadOlderRow use `BusyLine`; existing objectNames kept (`searchBusy`).

**Tests:**
- `tst_wizard.qml`: for each of the three steps, `test_<step>_shows_busy_exactly_while_busy`: busy false → BusyLine not visible, button visible; busy true → BusyLine visible with its text, button hidden; back to false → reversed. In Carbon and Airmail, light and dark.
- `tst_busy_line.qml`: `test_colours_come_from_the_theme` (spinner and text colours equal the theme roles in both themes).

**Proving:** `cargo make qml-test`, `cargo make app-docker`, `cargo make pot`.

---

### Task 6: Rows show names, placeholders, search field focused (#73, #63)

**Files:** create `crates/envio-core/src/worker/rows.rs` (move `rows()` out of `worker/read.rs`); modify `worker/read.rs`, `worker/search.rs`, `text/read.rs`, `crates/envio-core/src/api/view.rs` (`MessageRow.from_address: String`), `crates/envio-ui/src/rows.rs` (`MessageRowUi.from_address`), `qt/` list model role `fromAddress`; `qml/pages/inbox/MessageRow.qml`, `qml/components/stampTint.js` (take an address, not a from line), `qml/pages/search/SearchPage.qml`; `tests/qml/MockCore.qml` rows, `tests/qml/tst_inbox.qml`, `tests/qml/tst_search.qml`.

**Produces:**
- `rows()` sets `from` to the display name from `envio_mime::Address::parse(&envelope.from)`, else the bare address, else `text::read::no_sender()` ("(no sender)"); `from_address` the bare address (empty if none). Subject empty or whitespace → `text::read::no_subject()` ("(no subject)").
- `MessageRow.qml` tints the stamp from `model.fromAddress`.
- SearchPage on activation (`StackView.onActivated`): `field.input.forceActiveFocus(); Qt.inputMethod.show()`.

**Tests:**
- envio-core: `row_shows_the_display_name`, `row_without_a_name_shows_the_address`, `row_without_a_sender_shows_the_placeholder`, `row_without_a_subject_shows_the_placeholder`, `search_rows_use_the_same_names` (search path).
- QML: `test_stamp_tint_uses_from_address`, `test_search_field_has_focus_when_opened`.

**Proving:** `cargo make ci`, `cargo make qml-test`, `cargo make app-docker`, `cargo make pot`.

---

### Task 7: Trust prompt names the service; link sheet bolds the real host (#62, #76)

**Files (#62):** `crates/envio-core/src/text/setup.rs` (`trust(service, host, reason)`; new `trust_title(service, host)`), `crates/envio-core/src/setup_thread.rs`, `api/event.rs` (`Event::TrustPrompt { …, title }`), `crates/envio-ui/src/signals.rs`, `convert.rs`, `qt/src/core_object.rs` (`trust_prompt` signal gains `title`), `qt/src/events.rs`, `qml/pages/accounts/TrustDialog.qml`, `WizardPage.qml` (`onTrustPrompt(…, title)`), `tests/qml/MockCore.qml`, `tests/qml/tst_wizard.qml`.

**Produces (#62):** title "Can't verify the incoming mail server (IMAP)" / "…outgoing mail server (SMTP)"; body names the host and the service, reusing the wording of `text/status.rs::certificate`. QML shows `title` as given (no `qsTr` around core text).

**Files (#76):** `crates/envio-ui/src/link.rs`, `crates/envio-core/src/text/link.rs`, `qt/src/json.rs` (link keys), `qml/components/LinkSheet.qml`, tests.

**Produces (#76):** `LinkView { before_host, userinfo, host /* ASCII form, xn-- for an IDN */, unicode_host: Option<String> /* only when it differs */, after_host, warning, openable }`. The sheet bolds `host` and shows `unicode_host` as a secondary line with `text::link::looks_like(unicode)` ("Displays as %s, which may imitate another site"). Decision: bold the ASCII form (what the browser will really reach), rather than bolding neither.

**Tests:**
- envio-core: `trust_text_names_imap`, `trust_text_names_smtp`, `two_prompts_for_one_host_differ`.
- envio-ui: `idn_host_is_shown_in_ascii`, `idn_unicode_form_is_secondary`, `ascii_host_has_no_unicode_line`.
- QML: `test_trust_dialog_shows_the_core_title`, `test_link_sheet_bolds_the_ascii_host`.

**Proving:** `cargo make ci`, `cargo make qml-test`, `cargo make app-docker`, `cargo make pot`.

---

### Task 8: Phone check (controller)

The controller installs the build (`cargo make click-install`) and takes every screenshot with `tmp/u.sh shot` / `ute2e shot`, both themes where the screen changed, kept in `docs/phone-runs/daily-use/`. Never handed back to the user.

- [ ] #88: the real Stripe receipt and the Link notice (not committed): time from tap to text, VmRSS and VmHWM after opening (`/proc/<pid>/status`), screenshot showing columns (crop so no private content is kept; or shoot the synthetic `receipt-like` sent to the test account instead).
- [ ] #89: Inbox end with "Load older messages", during loading, after one batch, "No older messages"; Settings → Sync with the four choices; after "Last 90 days", the list reaches 90 days back.
- [ ] #87: each wizard step while busy, Carbon and Airmail.
- [ ] #73/#63: inbox and search rows with names; a no-sender/no-subject message; search opened with the keyboard up.
- [ ] #62: both trust prompts against the local Stalwart self-signed cert (IMAP then SMTP).
- [ ] #76: link sheet for `https://xn--pypal-4ve.com/`.
- [ ] Update `docs/phone-checklist.md` rows for each of the above.

---

## Done when

Issues #88, #89, #87, #73, #63, #62 and #76 are closed with the phone screenshots named in each closing comment, `docs/phone-checklist.md` has the new rows, and CI is green on master.

## Self-review

- Spec coverage: each issue maps to a task (#88 T1; #89 T2–T4; #87 T5; #73 and #63 T6; #62 and #76 T7); the #89 "separate bug" check is the first e2e test in T2; the loading state for #88 is tested in T1 and restyled in T5.
- No placeholders: every new type, function and test is named; the one number decided at run time (T1's `implicitHeight` bound) has its rule (3× headroom over the measured value, recorded in a comment).
- Type consistency: `sync_window_days: u32` (store) ↔ `Option<u32>` (`sync_window()`, `window_start`, `sync_folder`) ↔ `syncDays: i32` (qt; 0 = All). `OlderRow` 4 states ↔ `olderRow: i32` ↔ `LoadOlderRow.state`. `from_address` ↔ `fromAddress`. `LinkView.unicode_host` ↔ json key `unicodeHost`. `Event::TrustPrompt.title` ↔ qt signal `title` ↔ `onTrustPrompt(…, title)`.

## Pre-flight scan

Files touched by more than one task; run the tasks in order, never in parallel.

| Overlap | Tasks | Why it matters |
|---|---|---|
| `sync_folder` / `load_older` signatures | T2 → T3 | T3 calls what T2 defines |
| envio-ui signals, commands; qt properties | T3 → T4 | T4 binds to T3's `OlderRow` |
| `LoadOlderRow.qml` | T4 → T5 | T5 swaps its spinner for `BusyLine` |
| `qml/pages/reader/ReaderPage.qml`, `tst_reader.qml` | T1, T5 | T1 tests "Loading…", T5 restyles it; keep the objectName |
| `crates/envio-core/src/worker/read.rs` | T6 (T3 if it touches list paging) | T6 moves `rows()` to `worker/rows.rs` |
| `envio-ui/src/convert.rs`, `signals.rs`; `envio-core/src/text/read.rs`; `api/event.rs` | T3, T6, T7 | new variants and strings in the same files |
| `qt/src/core_object.rs`, `qt/src/events.rs` | T4, T6, T7 | line budget: split settings properties if over ~300 |
| `qml/pages/accounts/WizardPage.qml`, `tst_wizard.qml` | T5, T7 | busy line and the trust prompt title |
| `tests/qml/MockCore.qml` | T4, T5, T6, T7 | each adds members |
| `po/envio.aps.pot` | T3–T7 | run `cargo make pot` at the end of each task, regenerate on conflict |
| `crates/envio-mime/src/images.rs` (295 lines) | T1 | the width logic goes in new `widths.rs`, only a call is added here |
