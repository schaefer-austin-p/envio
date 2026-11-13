# Envio Plan 8d: Release 0.1.0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the six open milestone-12 issues (#108, #109, #110, #114, #115, #116), then release 0.1.0. The release covers a phone run on a clean install with synthetic accounts only, the budgets set from that run, an answer or a record for each of the spec's open questions, the OpenStore listing, the changelog, the launch audit and the `v0.1.0` tag.

**Architecture:** Part A is two PRs built in parallel from two worktrees, one per CI runner. **A-app** (`plan8d-app`: A1, A2, A3) changes what the user sees. #108 gets a Rust "the mail sets its own colours" flag in `envio-mime` and a paper card in the reader. #109 changes the wrap mode of two Text items. #110 makes envio-ui list the folders again when their messages change. **A-tests** (`plan8d-tests`: A4, A5, A6) makes three tests reliable. It also fixes the worker spin that #116 exposed, a loop in `pick_for` with no way out. No file is in both PRs. Part B starts once both are merged. B1, B4 (text) and B5 can be drafted alongside Part A. The phone run, the budgets, the screenshots, the audit and the tag follow in order.

**Tech Stack:** Rust (envio-mime, envio-core, envio-ui), cxx-qt in `qt/`, Qt 5.15 / QtQuick 2.15 on Ubuntu Touch 24.04, qmltestrunner offscreen, Stalwart in Docker. On the phone: clickable, `adb`, `ute2e shot` (~/dev/ute2edriver). The release: Forgejo Actions (`click.yml` builds the tagged click) and OpenStore.

**Spec:** docs/superpowers/specs/2026-11-07-envio-design.md ("Goals and non-goals", "Scope 0.1.0", "Testing and CI": the phone checklist, budgets "set from the first measurements on a phone", "Open questions to check on a phone"). Checklist: `docs/phone-checklist.md`. Issues: #108, #109, #110 (A-app); #114, #115, #116 (A-tests).

## Global Constraints

Verbatim from the spec:

- "Apps are suspended when not in the foreground, so there is no background sync. Sync runs only while envio is open"
- "AppArmor confinement: files only in envio's own folders; other files come and go through Content Hub. The system address book and calendar need reserved policy groups, so 0.1.0 uses neither."
- "System edge swipes (left: launcher, right: app switcher) rule out swipe actions on list rows."
- "`mailto:` links reach envio through url-dispatcher."
- "Logic lives in Rust; Qt and QML are used only for UI and platform APIs."
- "Dependencies point one way: core -> sync -> net, store, mime. net, store and mime do not depend on each other. The core is Qt-free."
- "one concept per module, files around 300 lines, logic kept out of I/O code, numbered migrations, state saved within about 1 s. Lectern's memory budgets do not apply; envio's are set from the first measurements on a phone."
- "Everything that is not on the server yet (Outbox, drafts, the action journal's pending actions) is kept outside the database so a rebuild cannot lose it"
- "**Passwords** in a separate file in envio's private data folder (mode 0600), never in the mail database, logs or crash reports."
- "**TLS only.** ... A changed pin stops sync and shows old and new fingerprints; it is never accepted on its own."
- "**Privacy:** remote images blocked by default (allow per message or per sender); known tracking pixels removed even when images are allowed; a link shows its real address before opening; no read receipts in 0.1.0; no telemetry; ... logs mask addresses and subjects."
- "User-facing error text is chosen and translated (gettext) in Rust; QML only shows it."
- "a written phone checklist per release (Content Hub, `mailto:`, suspend and resume, dark mode, memory and sync time, which set the budgets)."

From `CLAUDE.md` (also verbatim) and earlier plans:

- "Logic in Rust, Qt and QML only for UI and platform APIs. The core is Qt-free; `unsafe` only in `qt/`."
- "Migrations are numbered and never edited after merge." (This plan adds none.)
- "User-facing text is translated: `qsTr()` in QML, gettext in Rust, domain `envio.aps`. Run `cargo make pot` after changing strings."
- "Single-crate integration tests go in `crates/<crate>/tests/`; anything that needs two crates goes in `envio-e2e`."
- Every QML `Text` states `textFormat`. Colours, fonts and sizes come from `Theme` roles.
- Privacy for this release: only the synthetic Stalwart accounts `grace@envio.test` and `heidi@envio.test` (and `ivan` for the wizard) appear in any screenshot, log excerpt, listing image or issue. Mail in them is synthetic, with invented names and amounts and domains such as `example.org` or `envio.test`. The owner's real accounts never appear in this plan's evidence.
- Each task runs the proving commands it names, and `cargo make ci` passes at every commit.
- A-app tasks never touch `crates/envio-core/tests/`, `crates/envio-e2e/`, or `crates/envio-core/src/worker/turn.rs`. A-tests tasks never touch `qml/`, `qt/`, `crates/envio-mime/`, `crates/envio-ui/`.

## Plan series

8a. The Airmail look. 8b. Daily use on a real account. 8c. 0.1.0 polish and release CI. **8d. Release 0.1.0** (this plan).

## Parallel lanes (two CI runners)

| Lane | Branch / worktree | Tasks | Starts |
|---|---|---|---|
| 1 | `plan8d-app` | A1 → A2 → A3 | after the plan PR merges |
| 2 | `plan8d-tests` | A4 → A5 → A6 | same time as lane 1 |
| 1 (after A merges) | `plan8d-release` | B1 (script can start during A) → B2 → B3 → B4 → B5 → B6 | when both A PRs are merged |
| owner | — | B7 tag, B8 OpenStore upload | after B6 says go and the owner says go |

B4's listing text and B5's changelog are documentation. A controller can draft them during Part A, but they are committed on `plan8d-release` after the phone run, so they describe the build that ships.

---

## Part A, lane 1: app (one PR, branch `plan8d-app`)

### Task A1: HTML mail that sets its own colours is shown on a paper card (#108)

**Root cause (read in the code):** the sanitiser keeps author colours (`style.rs` `STYLE_PROPS` keeps `color` and `background-color`; `sanitize.rs` allows `font color`, `bgcolor` on `table`/`tr`/`td`). `ReaderPage.qml` (about line 181) always draws the body on `theme.surface`, and its `TextEdit` default `color` is `theme.text`. In dark mode the surface is `#1F242B` (Carbon) or `#1C2633` (Airmail). Text the mail colours itself (`#1a1f36`-style navy in `receipt-like.eml` and `notice-like.eml`) keeps that colour on the dark card, so it is dark on dark. Text without an author colour takes `theme.text` and stays readable. That matches the screenshots.

**Decision:** approach (a) from the issue. Mail whose sanitised HTML sets any colour is drawn on a light "paper" card in both themes, with dark default text and the light theme's link colour. Gmail, K-9 and FairEmail do the same for mail that sets its own colours. Plain-text mail, raw mail and HTML that sets no colour keep the theme's card, so dark mode stays dark for them. Approach (b), dropping author text colours, fails on the receipt: its header cell is white (`bgcolor="#ffffff"`), so text that loses its dark colour would go light on white. (Owner may overrule: see Open decisions.)

**Files:** create `crates/envio-mime/src/colours.rs` (under 120 lines); modify `crates/envio-mime/src/sanitize.rs` (`Rendered` gains `own_colours`), `lib.rs`; create `crates/envio-mime/tests/colours.rs`; modify `crates/envio-core/src/api/view.rs`, `crates/envio-core/src/worker/read.rs` (line ~112), `qt/src/json.rs` (`message_view_json`, line ~185), `qml/platform/Theme.qml` (three roles), `qml/pages/reader/ReaderPage.qml`, `tests/qml/MockCore.qml` (if the view shape lives there), `tests/qml/tst_reader.qml`; `docs/10-design.md` (one paragraph: the paper card).

**Produces:**
- `colours::sets_colours(html: &str) -> bool`: true when any *tag* in the sanitised HTML has a `style` with a `color` or `background-color` declaration, a `color` attribute (`font`), or a `bgcolor` attribute. It scans tags only (reuse `linear.rs`'s `parse_tag` or the same tokenizer), never text. Text that says "color: red" does not count.
- `Rendered { html, blocked_images, removed_trackers, own_colours: bool }`. `sanitize_html` sets it from `colours::sets_colours` on its final output. The too-deep plain fallback and `render`'s plain and raw paths leave it `false`.
- `MessageView.own_colours: bool`, from `rendered.own_colours`. A raw message is always `false`. qt JSON key `"ownColours"`.
- `Theme.qml`: `readonly property color paper: roles[family].light.surface`, `paperText: roles[family].light.text`, `paperLink: airmail ? roles.airmail.light.heading : roles.carbon.light.accent`, where `family` is the current family's key. In light mode they equal `surface`/`text`/`link`, so light mode does not change.
- `ReaderPage.qml`: `readonly property bool paper: view !== null && view.ownColours`. The card's `color` is `paper ? theme.paper : theme.surface`. The `TextEdit`'s `color` is `paper ? theme.paperText : theme.text`. `bodyHtml` passes `paper ? theme.paperLink : theme.link` to `RichText.withLinkColor`. In dark mode the paper card keeps the card radius and the page ground around it. It gets no extra border.

**Invariants:** no QML decides from the HTML itself. The flag comes from Rust. The attack and hidden-content corpora pass unchanged (the flag reads only the output). `rendering_scales_linearly` still passes, because the scan is linear and runs once over the output. Light mode looks exactly as before: the paper roles equal the light surface, text and link.

**Tests (fail without the fix):**
- `crates/envio-mime/tests/colours.rs`: `receipt_like_sets_colours`, `notice_like_sets_colours` (corpus files), `a_font_color_attribute_counts`, `a_bgcolor_counts`, `a_style_background_color_counts`, `html_without_colours_does_not`, `text_that_mentions_color_is_not_a_colour` (`<p>color: red</p>` → false), `plain_text_mail_does_not` (`render` of a text body), `too_deep_mail_does_not`.
- envio-core (`read.rs` tests): `the_view_says_when_the_mail_sets_colours` (HTML with `<td style="color:#1a1f36">` → `own_colours`; a text/plain message → not).
- qt: `json.rs` test `message_view_json_carries_own_colours`.
- QML `tst_reader.qml`: `test_mail_with_own_colours_uses_the_paper_card_data` / `test_mail_with_own_colours_uses_the_paper_card(data)` for Carbon dark and Airmail dark. The card's `color` equals `theme.paper`, `bodyText.color` equals `theme.paperText`, and `bodyHtml` holds the `paperLink` colour. Also `test_plain_mail_keeps_the_theme_card_in_dark` (`ownColours: false` → `theme.surface`, `theme.text`) and `test_light_mode_is_unchanged` (Carbon light with `ownColours: true` → the card colour equals `theme.surface`).

**Proving:** `cargo make ci`, `cargo make qml-test`, `cargo make app-docker`. Phone (B2): `receipt-like` and `notice-like` in Carbon dark and Airmail dark, plus one plain-text mail in dark.

---

### Task A2: Fingerprints wrap at any character (#109)

**Root cause:** both clipped lines are core text shown in a `Text` with `wrapMode: Text.WordWrap`. One is the Accounts page status (`AccountsPage.qml` ~line 82, `objectName: "status"`). The other is `AppDialog.qml`'s message (`wrapMode: Text.WordWrap` at lines 53 and 64), which the Check certificate dialog fills with `pinText`. The text is `text::status::certificate`, `"…\nOld fingerprint: {old}\nNew fingerprint: {new}"`. `Fingerprint::display()` writes `AA:BB:…`, 95 characters with no break opportunity: Unicode line breaking does not break after `:` between letters and digits. `WordWrap` never breaks inside a word, so the line runs past the width and is clipped. The "New fingerprint" line only wrapped by luck. The dialog's own fingerprint `Text` (`TrustDialog.qml:38`) already uses `WrapAnywhere`, which is why the wizard's prompt was fine.

**Decision:** `wrapMode: Text.Wrap` (break at a word boundary when there is one, otherwise anywhere) on those two `Text` items and on `Banner.qml`'s text, which also shows core status lines. Prose keeps word breaks, and only an over-long word breaks inside. No change in Rust: inserting break characters into the fingerprint would change the text the user compares and copies.

**Files:** modify `qml/pages/accounts/AccountsPage.qml`, `qml/components/AppDialog.qml`, `qml/components/Banner.qml`; `tests/qml/tst_accounts.qml`, `tests/qml/tst_components.qml`.

**Tests (fail without the fix):**
- `tst_accounts.qml::test_a_changed_pin_status_wraps_inside_the_fingerprint`: an account row whose `status` is the Rust-shaped changed-pin text with two 95-character fingerprints, at width 360. The `status` Text has `contentWidth <= width` and `lineCount >= 4`, and every line of the text is shown (no `truncated`). With `WordWrap`, `contentWidth > width`.
- `tst_components.qml::test_dialog_message_wraps_an_unbroken_word` (an AppDialog with that message at width 300: `contentWidth <= width`) and `test_banner_wraps_an_unbroken_word`.
- `tst_components.qml::test_dialog_message_keeps_word_breaks` (ordinary prose breaks at spaces: the first line ends with a whole word).

**Proving:** `cargo make qml-test`, `cargo make app-docker`. Phone (B2): Check certificate after a changed pin, both themes, every fingerprint character visible.

---

### Task A3: Menu folder counts follow what the user reads (#110)

**Root cause:** the drawer's per-folder numbers (`FoldersDrawer.qml` line ~209, `folder.model.unread`) come from the `Folders` model. envio-ui fills that model from `Event::Folders`, which the core sends only in answer to `Command::ListFolders`. The `unread_counts` query runs then and only then (`worker/read.rs` ~204). The UI asks for it in two places. One is `FoldersDrawer.expand()`, which `onVisibleChanged` calls only while `expanded === ""`, so only on the first opening. The other is `Ui::apply` on `Event::FoldersChanged` (a folder added or removed). Opening or marking a message emits `MessagesChanged` and `AccountUnread`. Neither refreshes the folder list. So "Inbox 4" stays until a folder is added or the app restarts. The Unified row is different. It sums the account rows' `unread` (the probe Repeater, lines 33-56), the same Inbox-only count (`inbox_unread`) the tabs show, and it follows `AccountUnread`. "Unified 1" after grace read all four was heidi's one unread, which is correct. The earlier "Unified 3 vs 4 + 1" could not be explained from the code. The task pins it with a test, and B2 rechecks it on the phone.

**Files:** modify `crates/envio-ui/src/ui.rs` (`apply`), `crates/envio-ui/src/ui/messages.rs` or a new `crates/envio-ui/src/ui/folder_counts.rs` if `messages.rs` would pass ~300 lines; `qml/pages/inbox/FoldersDrawer.qml`; `tests/qml/MockCore.qml`, `tests/qml/tst_inbox.qml` (or `tst_shell.qml`, wherever the drawer is tested).

**Produces:**
- `Ui::apply(Event::MessagesChanged { folders })` adds `Output::Send(Command::ListFolders { account })` when `folders_of` is `Some(account)` and any id in `folders` maps to that account in `folder_roles`, or is not in `folder_roles` at all (a folder the UI hasn't listed yet). It adds at most one `ListFolders` per event, next to the existing `messages_changed` outputs.
- `Ui::apply(Event::AccountUnread { account, .. })` also sends `ListFolders` when `folders_of == Some(account)`. This covers a count the core changed without a `MessagesChanged` for a listed folder.
- Coalescing: `Ui` keeps `folders_pending: bool`. It is set when a `ListFolders` goes out and cleared on `Event::Folders` for that account. While it is set, later triggers set `folders_again` instead, and the `Folders` answer sends one more `ListFolders` if `folders_again` was set. A sync batch therefore costs at most two queries, not one per event. (`messages.rs`'s `refresh_again` uses the same pattern.)
- `FoldersDrawer.qml`: `onVisibleChanged: if (visible) expand(expanded !== "" ? expanded : probe.firstKey)`. Every opening re-lists the open account. It skips the call when the key is empty.

**Invariants:** the counts stay local. The source is still `unread_counts` over cached flags, so they follow reads made offline (the issue's paused-server case). No QML computes a count. No `ListFolders` is sent for an account whose folders are not shown.

**Tests (fail without the fix):**
- envio-ui: `reading_in_a_shown_folder_lists_the_folders_again` (Folders for account a are shown, then `MessagesChanged { [inbox of a] }` → outputs contain `Send(ListFolders { a })`), `changes_in_another_account_do_not_list` (b's folder → no `ListFolders`), `account_unread_for_the_shown_account_lists_again`, `a_burst_of_changes_sends_at_most_two_lists` (five `MessagesChanged` before the `Folders` answer → one now and one after the answer), `nothing_is_listed_when_no_folders_are_shown`.
- QML: `test_drawer_lists_the_open_account_on_every_opening` (open, close, open → MockCore recorded `listFolders` twice) and `test_unified_count_is_the_sum_of_the_accounts` (accounts model updates 4+1 → 0+1: the Unified row shows 5, then 1).

**Proving:** `cargo make ci`, `cargo make qml-test`, `cargo make app-docker`. Phone (B2): read every unread message in grace's Inbox. The drawer's grace Inbox count clears, and Unified equals the sum of the tab counts. Then the same with the server stopped.

---

## Part A, lane 2: test reliability (one PR, branch `plan8d-tests`)

### Task A4: `changing_the_window_drops_the_floor_and_resyncs` waits for the right sync (#114)

**Root cause (read in the code):** the issue says "pruning happens after the worker has processed the answer". It cannot happen mid-step, because the worker handles commands only between steps (`Worker::turn` drains after `run_one`), and the prune is in the same step as the SEARCH (`driver::sync_folder`: search, `plan_folder`, `delete_messages`). `rows_of` is a command, so it is answered after the step it arrives during. The real race is *which* sync the counted `UID SEARCH SINCE` belongs to. `start_core` plans a full startup sync. `LoadOlder` is sent at the same moment, and the planner runs the Older step before folder syncs, so `older_loaded` can return before the startup sync has sent its SEARCH. Then `before` is taken too early, and the first SEARCH the test sees is the startup sync's (30-day window, floor still set, the 10 rows kept). Whether `SaveSettings` reaches the worker before or after that step starts decides the outcome, hence `left: 10`. The sibling `a_sync_after_load_older_keeps_the_older_rows` has the same flaw in the other direction. The SEARCH it waits for may also be the startup sync's, so it can pass without testing the Refresh.

**Fix:** a helper `settled(h, server, list) -> usize`. It waits for the startup sync (`until_lines(server, "UID SEARCH SINCE", 1)`), then calls `rows_of` (queued behind that step, so it is answered after it) and returns the count. Both tests call it after `older_loaded` and assert 10, *then* take `before`. The final assert stays a plain `assert_eq!` after `until_lines(before + 1)`, which is now the resync's SEARCH by construction. No polling: a poll would also hide a prune that came one step late.

**Files:** modify `crates/envio-core/tests/older.rs` only.

**Test that fails without the fix:** make the interleaving certain with a scratch `std::thread::sleep(Duration::from_millis(200))` right after `older_loaded` in both versions. Do not commit it. With it, the old test fails every time (left 10), and the new test passes 20 times in a row. Then, without the sleep and while `cargo build --release` loads the machine: `for i in $(seq 200); do cargo test -p envio-core --test older changing_the_window || break; done` passes. Report both runs in the PR.

**Proving:** `cargo make ci`.

---

### Task A5: The slow-copy draft test has no unguarded wait (#115)

**Root cause:** in `crates/envio-core/tests/drafts.rs` (~line 186), the helper thread sits inside `std::thread::scope`, which joins it before returning. If `core.send(AppActive(false))` never returned, `recv_timeout(10 s)` would give `false`, and then the scope would block on the join for ever, and so would the binary. The 10 s timeout guards nothing. In fact `Core::send(AppActive(false))` is already bounded: `DraftKeeper::flush` waits at most `FLUSH_WAIT` (500 ms, `drafts/keeper.rs:19`) and the command channel is unbounded, so the helper thread adds nothing on today's code. The real hang left in the test is `release()`, which opens the FIFO for writing. That open blocks until a reader exists, so a copy that never opened the pipe would hang the test.

**Fix:**
- Drop the helper thread, the channel and the scope. On the test thread, take `let t = Instant::now(); s.h.send(Command::AppActive(false)); let took = t.elapsed();`. The flushed check becomes `took < Duration::from_secs(5) && s.wait_for_draft(Duration::ZERO, |d| d.body == "typed again")`, with the assertion message "the flush waited for the copy".
- A detached watchdog (`std::thread::spawn`, not scoped) calls `release` after 10 s, so a flush that blocked for ever on the copy still returns, and the `took` check then fails. One release only: a shared `Arc<AtomicBool>` (or `Once`) makes the watchdog and the main path release at most once together.
- `release` opens the FIFO with `O_NONBLOCK` (`std::os::unix::fs::OpenOptionsExt::custom_flags(libc::O_NONBLOCK)`, or `nix` if it is already a dev-dependency; otherwise retry `open` for up to 5 s on `ENXIO`). If no reader appears it panics with "the copy never opened the source" instead of blocking.
- Every wait in the test is now bounded, which the test's doc comment states.

**Files:** modify `crates/envio-core/tests/drafts.rs` only (a `libc` dev-dependency in `crates/envio-core/Cargo.toml` only if no existing dependency gives `O_NONBLOCK`; prefer the retry loop over a new dependency).

**Test that fails without the fix:** two scratch breaks, neither committed:
1. Run the copy on the keeper thread (call `copy_into` inline in `run` instead of posting to `envio-attach`). The new test fails within about 1 s ("the flush waited for the copy"). The old one also fails, through `FLUSH_WAIT`.
2. Additionally make `flush` wait without a bound (`done.recv()` instead of `recv_timeout`). The old test hangs (kill it at 60 s). The new test fails at about 10 s, when the watchdog releases the pipe and `took` is too long.

Report both in the PR.

**Proving:** `cargo make ci`; the test 20 times under load as in Plan 8c A6.

---

### Task A6: A prefetch regression fails fast instead of hanging (#116)

**Root cause (reproduced on 2026-11-13 while writing this plan):** with `plan::prefetch::step()` returning `None`, `prefetch_stalwart` did not fail at its 30 s body wait. It failed earlier, in `CoreRun::wait` (`core_run.rs:105`, "no matching event within 30s"): the worker never answered `ListMessages`. Then the panic's unwind dropped `CoreRun`. `Core::drop` sends `Shutdown` and `join`s the worker thread with no bound, so the test hung (killed at 120 s). The worker was stuck before the panic, spinning in `Worker::pick_for` (`worker/turn.rs`). `Timing.has_wants` is `!live.wants.is_empty() || prefetch.is_some()`, where `prefetch` comes from `prefetch_due()`. `steps()` gets the prefetch step from a *second* copy of the rule, `plan::prefetch::step(&env)`. When the two disagree, `next()` says `Run`, `steps()` is empty, the "only downloads were wanted" fallback resets `wants`, and the `loop` goes round again. `prefetch_due` is still `Some`, so it never awaits and never returns. On master today the two copies check the same conditions (foreground, not paused, disk not full), so this is latent. But any change to either copy turns it into a frozen worker on the phone, not just a hung test.

**Fix (two parts):**
1. Worker (`crates/envio-core/src/worker/turn.rs`, `crates/envio-core/src/worker/prefetch.rs`): one rule. `pick_for` builds `env` before `timing`, from `prefetch_folder()` (the candidate Inbox, no state checks). Then it sets `has_wants: !live.wants.is_empty() || plan::prefetch::step(&env).is_some()`. `prefetch_due`'s duplicated foreground, paused and disk-full check goes, and the function is removed if `prefetch_folder` covers its callers. As a backstop, the empty-plan fallback may run once per `pick_for` call. A second empty plan in the same call logs `log::error!("planner wanted to run but had no step")` and returns `None`, so the loop always ends.
2. Harness (`crates/envio-e2e/tests/support/core_run.rs`): bounded teardown. `CoreRun.core` becomes `ManuallyDrop<Core>`, so call sites keep `run.core.send(..)` through `Deref`, and `restart_with` takes it out with `ManuallyDrop::take`. `impl Drop for CoreRun` moves the `Core` to a helper thread that drops it and signals on a channel. It waits at most 10 s. On timeout it prints `"core did not stop within 10 s: the worker is stuck"` to stderr and leaks the thread. If the test is not already panicking (`std::thread::panicking()`), it panics with that message. A stuck worker turns a hang into a red test within about 40 s.

**Invariants:** `has_wants` and `steps()` use the same prefetch rule, and there is no other copy. `pick_for` returns on every path within a bounded number of passes: at most one `begin_idle` continue and one empty-plan reset. Production `Core::drop` is unchanged: the app must still flush and join. (A bounded join on the phone is a separate decision, not 0.1.0.)

**Files:** modify `crates/envio-core/src/worker/turn.rs`, `crates/envio-core/src/worker/prefetch.rs`, `crates/envio-core/src/worker/plan/prefetch.rs` (tests only); `crates/envio-e2e/tests/support/core_run.rs`; create `crates/envio-e2e/tests/teardown.rs`.

**Tests (fail without the fix):**
- `plan/prefetch.rs` unit: `wants_and_steps_agree_on_prefetch`. For every combination of `foreground`, `paused`, `disk_full` and `prefetch: Some/None`, `prefetch::step(&env).is_some()` holds exactly when `steps(&Wants::default(), &folders, &env)` contains `Step::Prefetch`.
- `turn.rs` unit (with the `testing.rs` harness, like `recent_inbox_bodies_are_prefetched_until_skipped`): `pick_for_returns_in_the_background_with_candidates_left`. A background worker with prefetch candidates returns `None` from `pick_for` within 1 s. Run it on a thread with `recv_timeout` so a spin fails instead of hanging. Also `pick_for_returns_when_paused_with_candidates_left`, the same for a paused account.
- `crates/envio-e2e/tests/teardown.rs` (no Stalwart): `a_stuck_core_does_not_hang_teardown`. A `CoreRun` sends `Command::Stall { ms: 120_000 }` (the test-hooks stall the core tests use), then drops the run inside `catch_unwind` after a forced panic. The drop returns within 15 s. Without part 2 it takes 120 s and the test's own timer fails it.
- Scratch, not committed: with `step()` returning `None` again, `cargo make e2e-it` goes red on `prefetch_stalwart` within 60 s (the issue's acceptance) with part 1 and part 2. With part 2 alone, it is red within about 40 s with the "worker is stuck" message. Record both times in the PR and in #116.

**Proving:** `cargo make ci`, `cargo make e2e-it`.

---

## Part B: release (branch `plan8d-release`, after both Part A PRs merge)

### Task B1: Synthetic mail for the phone run (can start during Part A)

**Files:** create `tools/phone/seed.py` (Python 3 standard library only: `imaplib`, `email`, `ssl`, `random` with a fixed seed), `tools/phone/README` section in `docs/phone-checklist.md` (no new README file).

**Produces:**
- `seed.py --host 127.0.0.1 --port <imaps> --user heidi --password heidi-secret-1 --count 3000 --days 400 [--html-share 0.4] [--attachments 20]`. It APPENDs `count` synthetic messages to INBOX, dates spread over `days` (newest first, so the 30-day window holds about 225). The senders are invented names at `example.org`, `example.net` and `envio.test`. Subjects and bodies come from a fixed word list, threads have In-Reply-To chains, and about 40 % are HTML, some with author colours. `--attachments` gives N messages a small generated PDF and PNG. A fixed random seed makes the mailbox the same on every run. Certificate check off (the throwaway server is self-signed; this is a test tool).
- `seed.py --listing --user grace ...`: a curated set of about 15 messages for the store screenshots. Examples are a trip itinerary from "Lena Ortiz", a project thread from "Sam Okafor", a newsletter (`notice-like` style), a receipt (`receipt-like`) and a message with an attachment. All names, companies and amounts are invented. No real brand, domain or person.
- The checklist gets a "Phone setup" section. It covers `crates/envio-net/tests/stalwart/up.sh`, `adb reverse` of the IMAPS and submission ports, adding grace and heidi in the wizard (host `127.0.0.1`, trust prompt) and running `seed.py`.

**Invariants:** nothing real. A reviewer can read the word list and the name list in the script.

**Tests:** `python3 -m py_compile tools/phone/seed.py`. A dry run against the local Stalwart from `up.sh` (`--count 50`), then `UID SEARCH ALL` gives 50 and a second run with the same seed gives the same subjects.

---

### Task B2: Phone run on a clean install (controller)

The controller runs it. Every screenshot is taken with `ute2e shot` (never handed back to the owner), kept in `docs/phone-runs/0.1.0-release/` and named in the checklist table. Build: `master` after Part A, `cargo make click-install` (or the `click.yml` artifact of that master commit, to test what CI builds).

**Clean install:**
- [ ] **Owner go-ahead first** (Open decisions): back up the phone's envio data to the host (`adb pull` of `~/.local/share/envio.aps`, `~/.cache/envio.aps`, `~/.config/envio.aps` into `~/.cache/envio-phone-backup/<date>/`, mode 0700, outside the repo, never committed). Then `clickable uninstall` (or `pkcon remove envio.aps`) and remove the three folders, and confirm with `ls` that they are gone.
- [ ] Install, start, see the empty first-run screen (Carbon light and dark).

**Accounts:** `grace` (listing set from B1) and `heidi` (3000 messages from B1), both through the wizard against the throwaway Stalwart over `adb reverse`. Synthetic only.

**Checklist items (each gets a row in a new "0.1.0 release run" table in `docs/phone-checklist.md`):**
- [ ] Content Hub in: attach a photo (Gallery) and a PDF (Document Viewer). Send grace → heidi. Both arrive and open. (Partial in the pre-release run: completes it.)
- [ ] Content Hub out: Open, Save, Share on the received PDF.
- [ ] `mailto:` from the browser: a page served from the host over `adb reverse` with `mailto:heidi@envio.test?subject=…&body=…`, tapped in Morph. (The pre-release run used url-dispatcher directly.)
- [ ] Suspend and resume: a half-written draft, a minute behind another app, then back, and again with the screen locked (not run before). Mail APPENDed meanwhile shows within a few seconds, nothing lost or doubled.
- [ ] Dark mode: every screen in Carbon light and dark and Airmail light and dark, compared with `docs/design/`. Include the #108 paper card (`receipt-like`, `notice-like`, one plain mail in dark), #109 (Check certificate after swapping the server certificate: both fingerprints whole) and #110 (read all of grace's unread, then check the drawer counts online and with Stalwart stopped).
- [ ] Edge swipes, link sheet, search on server, trust dialog, CA roots line in the log: quick re-run on this build (one shot each).
- [ ] **Memory and sync time** (sets the budgets, B3), on heidi with 3000 messages:
  - first sync: seconds from Continue on the last wizard step to the first rows shown, and to status Ready (app log timestamps).
  - `VmRSS`/`VmHWM` from `/proc/$(pidof envio)/status` (via `adb shell`) at: idle after the first sync; after scrolling the unified list to the bottom of the window and tapping Load older once; after opening 20 messages (10 HTML); after 10 min in the foreground (IDLE).
  - first open of a prefetched message vs one fetched on open: the `timedopen.sh` method from Plan 8c, if it still works, else note "not measurable" as 8c did.
  - cold start to the inbox drawn: three runs, median.
  - each number run three times, median recorded, phone model and build in the table.
- [ ] Open questions (B3 records them): the secret store probe, the Content Hub `.ics` probe, the CA roots line.

Any failed item becomes a Forgejo issue (`moscow/*` label, milestone "Plan 8: Release 0.1.0" if it blocks the release, else the next milestone) and is triaged before B6: fix in a follow-up PR on this lane, or record as a known limitation in B5 with the owner's agreement.

---

### Task B3: Budgets and open questions in the spec (controller)

**Files:** modify `docs/superpowers/specs/2026-11-07-envio-design.md` (replace "Open questions to check on a phone" with "Answered on the phone (0.1.0)" plus whatever stays open, and add a "Budgets" section after "Testing and CI"); `docs/phone-checklist.md` ("Memory and sync time" pass criterion now names the budgets).

**Budgets rule:** each budget is the B2 median × 1.5, rounded up (memory to the next 10 MB, time to the next whole second). The budgets cover RSS idle after the first sync, peak (`VmHWM`) over the run, first sync of 3000 messages to the first rows and to Ready, and cold start. The pass criterion for later releases: within budget on the same phone, or the release notes say why. The multiplier is an open decision for the owner; 1.5 is the default.

**Open questions, each answered or recorded:**
- *Can a confined app reach a system secret store?* Probe on the phone: `busctl --user list | grep -i secrets` (is a Secret Service running at all?), then `aa-exec -p envio.aps_envio_0.1.0 -- gdbus call --session --dest org.freedesktop.secrets --object-path /org/freedesktop/secrets --method org.freedesktop.DBus.Peer.Ping`, and read `journalctl` for an AppArmor `DENIED` line. Record the result and the evidence. Expected: no service, or denied without a reserved policy group. Then passwords stay in the 0600 file, as the spec already says.
- *Does the Calendar app accept an `.ics` file through Content Hub?* A 0.2.0 question. Check the Content Hub peer registrations on the phone (`/usr/share/content-hub/peers/` and the calendar click's `content-hub` hook JSON) for an importer of the events type, and record what is there. Do not build anything. If it is not answered, it stays open under 0.2.0's "Meeting invites" item.
- *Are the system CA certificates readable under confinement?* Already answered: yes. The pre-release run logged "CA roots: source system, 121 certificates" (#31). The B2 log line is re-checked and both are cited.
- *Real memory use with a few thousand messages?* Answered by B2's measurements, which set the budgets above.

**Proving:** the spec diff, reviewed in the release PR.

---

### Task B4: OpenStore listing text and screenshots (controller)

**Files:** create `docs/store/listing.md` (the text as it will be pasted, with each OpenStore field labelled) and `docs/store/screenshots/*.png` (5 images); modify `manifest.json` `description` only if the owner agrees (see Open decisions).

**Listing fields:**
- Name: Envio. Package: `envio.aps`. Category: Communication. License: GPL-3.0 (from `LICENSE`). Source: `https://code.austinschaefer.com/schaefera/envio`. Support: that repo's issues. Maintainer: from `manifest.json`.
- Tagline (one line): what it is ("Email for Ubuntu Touch: IMAP and SMTP, private by default").
- Description (plain, short paragraphs). What works in 0.1.0 (accounts with password or app password, the autodetecting wizard, unified inbox, compose, reply, forward, Outbox, drafts kept, attachments through Content Hub, `mailto:`, local and server search, Undo, Carbon and Airmail themes). What it doesn't do and why: no background sync or notifications (apps are suspended), no OAuth (Microsoft accounts need it; Google works with an app password), no encryption yet. Privacy: remote images blocked, trackers removed, no telemetry, passwords never in the mail database or logs. The wording follows the spec's "honest about what a confined, suspended app can do".
- Changelog field: the 0.1.0 entry of `CHANGELOG.md` (B5), shortened.
- Permissions note: networking, connectivity and content exchange, and why each is needed.

**Screenshots (synthetic only, from the B2 clean install with grace's listing set):** 1. unified inbox, Carbon light; 2. reader on a curated HTML mail, Airmail light; 3. compose with an attachment chip; 4. inbox in Carbon dark; 5. the wizard's sign-in step showing the found servers. Taken with `ute2e shot` at the phone's native size. Before committing, check each one by eye: no real address, name, host or notification in the status bar (use `envio.test` hosts only, Do Not Disturb on).

**Proving:** the owner reads `listing.md` and the images in the release PR (taste list item in B6).

---

### Task B5: Version 0.1.0 and the changelog

**Files:** create `CHANGELOG.md` at the repo root; modify nothing else unless a version disagrees.

**Version:** `manifest.json` `"version": "0.1.0"`, `Cargo.toml` `[workspace.package] version = "0.1.0"` and `qt/Cargo.toml` `version = "0.1.0"` already agree on master. The task checks they still do after Part A. It also checks that Settings → About (`qt/src/build_info.rs`, read from `manifest.json`) shows `0.1.0` and the commit on the B2 build, with a screenshot. If one disagrees, fix it here.

**`CHANGELOG.md`:** Keep a Changelog format, one section `## [0.1.0] - <tag date>`.
- **Added**: the spec's "Scope 0.1.0" list as shipped, checked against the merged plans 1-8d, not copied blindly.
- **Privacy and security**: TLS only, pinned self-signed certificates, remote images blocked, trackers removed, no telemetry, passwords in a 0600 file.
- **Known limitations**: no background sync or notifications, no OAuth, no OpenPGP, no system address book, plus any B2 failure the owner accepted as a limitation, each with its issue number.
- No list of internal refactors. Link to the Forgejo milestone.

**Proving:** `cargo make ci` (docs only); the B6 spec audit checks every changelog line against the code.

---

### Task B6: Launch audit (controller, `auditing-before-launch`)

Run the `auditing-before-launch` skill on master after B1-B5 merge. There is no earlier tag, so the range is the whole history (`git rev-list --max-parents=0 HEAD`..`origin/master`): every PR since the repo started.

- [ ] `review-calibrate --list --rev origin/master <repo>` lists the weak-review PRs (score `-` or below 70). Their trunk files get read in full.
- [ ] The four parallel audits (correctness, security, performance, spec) run in fresh agents with the Blast radius from `CLAUDE.md` and the weak-PR list. The spec audit also checks `CHANGELOG.md` and `docs/store/listing.md`: every promise there exists and has a test or a phone row.
- [ ] Taste list for the owner: listing text and screenshots, changelog wording, the paper card in dark mode (#108 decision), the budgets' multiplier, default theme and sync window.
- [ ] Report in the skill's exact format (`Verdict: go | no-go`, blockers, non-blocking findings, weak-review PRs, taste list, rollback plan). Rollback for a store release means unpublishing the revision on OpenStore. Users keep the installed click, and the next revision supersedes it.
- [ ] CI is green on the `origin/master` commit to be tagged (`ci.yml` and `click.yml` on that push), or the verdict is no-go.

A blocker goes back to a fix PR (`shipping-a-pr`, trunk depth for trunk files) and the audit re-runs on the new range end.

---

### Task B7: Tag `v0.1.0` (**owner's explicit go-ahead required**)

Only after B6 reports `Verdict: go` *and* the owner says to tag. The audit's go is not the owner's go.

- [ ] `git fetch origin`. The tag goes on the exact `origin/master` commit B6 audited (check `git rev-parse origin/master` equals the audited sha).
- [ ] Annotated tag `v0.1.0` with the changelog's 0.1.0 section as the message, pushed to `origin` (Forgejo). Never force, never retag.
- [ ] `click.yml` runs on the tag. Its artifact is downloaded, and `click info` shows `envio.aps`, `0.1.0`, `arm64`. `clickable review` output in the job has no errors.
- [ ] A Forgejo release on the tag with the changelog section and the `.click` attached (`create_release`, only with the owner's go-ahead, same as the tag).

---

### Task B8: OpenStore upload (**owner's explicit go-ahead required**)

Only after B7 and when the owner says to publish. The owner may want to upload it personally (account, terms, review).

- [ ] Upload the `v0.1.0` click artifact (the one `click.yml` built from the tag, not a local build) with `docs/store/listing.md`'s fields and the five screenshots.
- [ ] After OpenStore's review: install from the OpenStore app on the phone over the B2 build. It starts and keeps grace and heidi. Take a screenshot of the store page.
- [ ] Close milestone 12. Restore the owner's backed-up phone data only if the owner asks (B2's backup).

---

## Done when

The plan PR is merged. `plan8d-app` and `plan8d-tests` are merged with #108, #109, #110, #114, #115 and #116 closed, each closing comment naming its proof (phone screenshot for #108-#110; the scratch-break results for #114-#116). `docs/phone-checklist.md` has the 0.1.0 release run table. The spec has the budgets and the answered open questions. `CHANGELOG.md` and `docs/store/` are merged. B6 reports go. With the owner's go-ahead, `v0.1.0` is tagged and its click is built, and the OpenStore upload is done or explicitly left to the owner.

## Self-review

- Coverage: #108 A1, #109 A2, #110 A3, #114 A4, #115 A5, #116 A6. Phone run B2 covers Content Hub, `mailto:`, suspend and resume, dark mode, memory and sync time. Open questions B3, budgets B3, listing B4, version and changelog B5, audit B6, tag B7, upload B8.
- Type consistency: `Rendered.own_colours` → `MessageView.own_colours` → JSON `ownColours` → `ReaderPage.paper` → `Theme.paper`/`paperText`/`paperLink`. `Ui.folders_pending`/`folders_again` are used only around `ListFolders`/`Event::Folders`. `CoreRun.core: ManuallyDrop<Core>`, with `restart_with` adapted.
- Each fix has a test that fails without it. A1-A3 have direct unit and QML tests. For A4-A6, a test-only fix can only be shown failing by putting the bug back on purpose, so each names a scratch break and its expected result.
- Numbers decided at run time have a rule: budgets (median × 1.5, rounded up), A5's watchdog (10 s), A6's teardown bound (10 s).

## Pre-flight scan

| Overlap | Tasks | Why it matters |
|---|---|---|
| `crates/envio-core/src/worker/read.rs` | A1 | `own_colours` in the view; A6 does not touch it |
| `crates/envio-core/src/worker/turn.rs`, `prefetch.rs` | A6 | only lane 2 |
| `tests/qml/MockCore.qml` | A1, A3 | each adds members (`ownColours` in views; `listFolders` recording); same lane, in order |
| `qml/platform/Theme.qml` (trunk) | A1 | three read-only roles, no value changes |
| `qml/components/AppDialog.qml`, `Banner.qml` | A2 | used by every dialog and banner: `Text.Wrap` keeps word breaks, tested |
| `docs/phone-checklist.md` | B1, B2, B3 | same lane, in order |
| `docs/superpowers/specs/…design.md` | B3 | the open questions section is replaced; the rest is unchanged |

## Open decisions

1. **#108 approach:** paper card for mail that sets its own colours (default in this plan) vs stripping author colours. Owner may overrule before A1 starts.
2. **Clean install wipes the phone's envio data**, including the owner's real accounts. B2 backs it up to the host first and needs the owner's go-ahead.
3. **Budget multiplier:** 1.5 × the measured median (default).
4. **`manifest.json` description:** today "UT-First email client". B4 proposes the listing tagline. Change it only with the owner's agreement, since the manifest is trunk.
5. **Tag (B7), Forgejo release and OpenStore upload (B8):** each needs the owner's explicit go-ahead. The owner may prefer to upload personally.
