# Envio Plan 8c: 0.1.0 polish and release CI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close what the Plan 8b phone run and the 0.1.0 audit left open before the release: a server lookup that reads the hosts file four times (#97), a list row that shows a fake address as the sender's name (#92), an offline banner on every tab (#91), guessed servers that can't be checked or edited (#93), narrow text columns in Stripe-style mail (#90), two flaky timing tests (#99, #80); and on the CI side the arm64 click build (#7), security-only dependency updates (#25) and the empty test selection (#16).

**Architecture:** Two PRs, built in parallel from two worktrees. **Part A (app)** changes Rust and QML: #97 is in `envio-net` autoconfig (one resolver per Detect, lookups raced but answered in priority order), #92/#91/#93 put the decision and the text in `envio-core`/`envio-ui` and keep QML to showing it, #90 adds a linearising pass in `envio-mime`. **Part B (CI)** adds workflows and scripts only. No task in one part touches a file of the other part. This plan's own docs-only PR exercises #16.

**Tech Stack:** Rust (envio-net with hickory-resolver 0.26 and futures, envio-mime, envio-core, envio-ui), cxx-qt in `qt/`, Qt 5.15 / QtQuick 2.15 on Ubuntu Touch 24.04, qmltestrunner offscreen, Stalwart in Docker; Forgejo Actions (dind), clickable, Renovate.

**Spec:** docs/superpowers/specs/2026-11-07-envio-design.md ("Scope 0.1.0", "Error handling", "Testing and CI"). Issues: #97, #92, #91, #93, #90, #99, #80 (Part A); #7, #25, #16 (Part B).

## Global Constraints

From `CLAUDE.md` and the spec, verbatim where they bind:

- Logic in Rust, Qt and QML only for UI and platform APIs. The core is Qt-free; `unsafe` only in `qt/`.
- Dependencies point one way: ui → core → sync → net, store, mime. net, store and mime never depend on each other.
- One concept per module, files around 300 lines, logic kept out of I/O code (pure planners, thin drivers).
- Migrations are numbered and never edited after merge. (This plan adds none.)
- No swipe actions on list rows (Ubuntu Touch edge gestures).
- User-facing text is translated: `qsTr()` in QML, gettext in Rust, domain `envio.aps`. Run `cargo make pot` after changing strings. Status and setup text is chosen and translated in Rust; QML shows it as given (no `qsTr` around core text) and never formats a count.
- Single-crate integration tests go in `crates/<crate>/tests/`; anything that needs two crates goes in `envio-e2e`.
- Every QML `Text` states `textFormat`; colours, fonts and sizes come from `Theme` roles.
- Privacy: never commit or quote private real mail. Corpus files are synthetic look-alikes with invented names, amounts and domains (`example.org`, `envio.test`). Logs mask addresses (`mask::address`).
- Part A tasks never touch `.forgejo/`, `tools/ci/`, `renovate.json`; Part B tasks never touch `crates/`, `qt/`, `qml/`, `tests/`, `po/`.
- Each task runs the proving commands it names; `cargo make ci` passes at every commit.

## Plan series

8a. The Airmail look. 8b. Daily use on a real account. **8c. 0.1.0 polish and release CI** (this plan). 8d. Release 0.1.0: the phone checklist on a clean install, budgets, store listing, tag.

---

## Part A: app (one PR, branch `plan8c-app`)

### Task A1: One resolver per Detect, lookups in parallel (#97)

**Files:** modify `crates/envio-net/src/autoconfig/srv.rs`, `mod.rs`, `fetch.rs` (`FETCH_TIMEOUT`); create `crates/envio-net/src/autoconfig/race.rs` (the pure "which answer wins" rule, under 150 lines); modify `crates/envio-net/tests/autoconfig.rs`. `crates/envio-core/src/setup_thread.rs` is unchanged (it already makes one `Internet` per Detect job).

**Produces:**
- `srv::Resolver` wrapping one `TokioResolver`, built with `use_hosts_file = ResolveHosts::Never` (SRV records never come from hosts); `pub fn new() -> Option<Resolver>` (None when the system config can't be read: SRV then answers empty, as today); `pub async fn lookup(&self, name: &str) -> Vec<SrvRecord>`. A pure `fn options(base: ResolverOpts) -> ResolverOpts` sets the hosts rule so it can be unit-tested (check the exact 0.26 builder API).
- `Internet { roots, limit, resolver: Option<srv::Resolver> }`, built once in `Internet::new`/`with_fetch_limit`; `Lookups::srv` uses it. No other place builds a resolver.
- `FETCH_TIMEOUT` 30 s → 10 s; `srv::SRV_TIMEOUT = 10 s` (was `CONNECT_TIMEOUT`, 20 s). `DETECT_TIMEOUT` stays 60 s.
- `find` starts the three config fetches and the four SRV queries at once (`FuturesUnordered` of boxed futures over `&lookups`, no spawn) and decides after each answer with `race::settle`.
- `race::settle(files: &[Option<Option<String>>; 3], srv: &[Option<Vec<SrvRecord>>; 4], domain, email) -> Settle { Found(Found), Nothing, Wait }`: walk the files in `config_urls` order; an unknown file → `Wait`; a file that parses to a pair (`xml::parse` + `plan::best`) → `Found`; when every file is known and none gave a pair, wait for all four SRV answers, then `plan::best(.., Source::Srv, domain)` → `Found` or `Nothing`.

**Invariants:**
- Priority unchanged: ISPDB beats provider beats well-known beats SRV, whatever answers first. An early ISPDB answer returns at once (no waiting for slower lower-priority lookups).
- SRV confirmation (#33) unchanged: `best` still sets `confirm` for SRV hosts outside the domain, and the worker's `srv_hosts` path is untouched.
- The 60 s budget still ends everything with the guess.

**Tests:**
- `srv.rs` unit: `the_resolver_never_reads_the_hosts_file` (`options(default).use_hosts_file == ResolveHosts::Never`).
- `race.rs` unit: `waits_for_a_higher_priority_file`, `a_lower_file_wins_when_the_higher_ones_are_empty`, `srv_is_used_only_when_every_file_is_empty`, `waits_for_every_srv_answer`, `nothing_when_all_are_empty`.
- `tests/autoconfig.rs` (scripted `Lookups` with per-call delays): `lookups_run_in_parallel` (each call sleeps 300 ms and finds nothing; detect returns the guess in under 1.2 s, sequential would be 2.1 s), `the_ispdb_wins_even_when_the_provider_answers_first` (ISPDB 300 ms, provider at once → `Source::Ispdb`), `an_early_ispdb_answer_does_not_wait_for_slow_lookups` (ISPDB at once, others pending forever, detect within 500 ms), `files_beat_srv_even_when_srv_answers_first`. Existing tests, including `srv_servers_outside_the_domain_ask_for_confirmation` and `a_search_that_runs_out_of_time_gives_the_guess`, pass unchanged.

**Proving:** `cargo make ci`.

**Phone (A7):** detect for `ivan@envio.test` with the phone's 36 MB hosts file: `envio-setup` CPU time and VmRSS/VmHWM before and after, and time from Continue to the Sign-in step.

---

### Task A2: The row shows the real address when the name looks like one (#92)

**Files:** modify `crates/envio-core/src/worker/rows.rs`, `crates/envio-core/src/worker/read.rs` (both header paths use the same sender rule), `qml/pages/inbox/MessageRow.qml` (stamp expression), `tests/qml/tst_inbox.qml`, `tests/qml/tst_reader.qml`.

**Produces:**
- `pub(super) fn sender_label(name: Option<&str>, email: &str) -> String` in `rows.rs`: the email when `name` is None, empty, or looks like an address (after `strip_bidi_controls`, it contains `@`, `＠` U+FF20 or `﹫` U+FE6B); otherwise the name. Used by `rows::sender` and by `read.rs` for `MessageHeader.from` (stored and parsed paths), so the list, the reader title and both stamps agree.
- Unparseable but non-empty From line: `MessageRow.from` = `strip_bidi_controls(line.trim())` (what the reader's stored header shows, not "(no sender)"), and `MessageRow.from_address` = the stamp key, a Rust mirror of `stampTint.addressOf` (inside the last `<…>`, else the whole line; trimmed, lowercased). `from_address` on the row is only read by the stamp; document the field as "the address, or the key the stamp is tinted by". `MessageHeader.from_address` stays empty for such a line (it drives `sender_known` and image loading).
- Empty From: unchanged ("(no sender)", empty address, no initials, `isBlue("")`, as the reader).
- `MessageRow.qml` keeps its expression (`initials: fromAddress === "" ? "" : StampTint.initials(from)`, `blue: StampTint.isBlue(fromAddress)`); `HeaderBlock.qml` (lines 52-56) uses `initials(view.from)` and `isBlue(addressOf(fromAddress ? fromAddress : from))`. They agree only through the Rust values, so the guard is a Rust test: `rows.rs` gets `fn stamp_key(address: &str, from: &str) -> String` (the `addressOf` mirror applied to `address`, else `from`), and `stamp_key_of_the_row_matches_the_reader` checks, for a named sender, a bare address, a name that is an address, an unparseable line and an empty From, that the row's `from_address` equals `stamp_key(header.from_address, header.from)` and that the row's initials source (`from`, or nothing when `from_address` is empty) matches the header's. Change either QML expression only if that test shows a case the Rust values cannot cover; then a QML test renders both components with the same inputs.

**Decision:** only "looks like an address" is treated as misleading. A brand name without `@` (`"PayPal" <x@evil.example>`) still shows as the name; the reader shows the address under it.

**Tests:**
- envio-core (`rows.rs`, next to `row_shows_the_display_name`): `a_name_that_is_an_address_shows_the_real_address` (`"billing@paypal.com" <x@evil.example>` → `x@evil.example`), `a_fullwidth_at_sign_counts_as_an_address`, `a_name_equal_to_the_address_shows_the_address`, `an_unparseable_sender_shows_the_raw_line_and_a_stamp_key`, `the_reader_header_uses_the_same_label` (`read.rs` test).
- QML: `tst_inbox.qml::test_row_and_header_stamps_agree` (a row and a HeaderBlock fed the same Rust-shaped values for a named sender and an unparseable one: same initials, same `blue`), in Airmail.

**Proving:** `cargo make ci`, `cargo make qml-test`, `cargo make app-docker`.

---

### Task A3: Offline banner per tab, naming the accounts (#91)

**Files:** modify `crates/envio-core/src/text/status.rs`; create `crates/envio-ui/src/ui/offline.rs`, modify `crates/envio-ui/src/ui.rs` (`mod offline`, re-export); modify `qt/src/core_object.rs` (invokable declaration only) and create `qt/src/status_calls.rs` for its body (`calls.rs` is 359 lines, `core_object.rs` 311); modify `qml/pages/inbox/InboxPage.qml`, `tests/qml/MockCore.qml`, `tests/qml/tst_inbox.qml`; `po/envio.aps.pot`.

**Produces:**
- `text::status::offline_here(waiting: u64) -> String`: "Offline." or "Offline. {waiting}" (the existing `count::waiting` text, joined by a translated template `"{offline} {waiting}"` with a TRANSLATORS note).
- `text::status::offline_accounts(emails: &[String], waiting: u64) -> String`: one → "{email} is offline."; more → `tr_n("{n} account is offline: {emails}", "{n} accounts are offline: {emails}", n)` with the emails joined by ", "; same waiting suffix. Empty slice → "". (A `CountKind` is not needed: QML never asks for this count; the plural goes through `tr_n` as `count.rs` does.)
- `envio_ui::BannerTab { Unified, Account(String), Folder(i64) }` and `Ui::offline_banner(&self, tab: &BannerTab, waiting: u32) -> String`: Unified → `offline_accounts` over every account whose `state == "offline"`, in account order; Account(key) → `offline_here` if that account is offline, else ""; Folder(id) → the folder's account via `folder_roles`, then as Account; an unknown folder → "".
- qt invokable `offlineText(tab: QString, folder: i32, waiting: i32) -> QString`: tab "" → Unified, "folder" → Folder(folder), else Account(tab).
- InboxPage: the offline Banner's `text` is `{ offlineProbe.revision; return root.core.offlineText(root.tab, root.folderId, root.waitingCount) }`. The probe (InboxPage.qml:128) has only `offline` today: add `property int revision: 0` and `revision++` at the end of `recount()`, so the binding re-reads whenever any account's state changes, even when the offline count stays the same, `visible: text !== ""`. The Outbox banner shows when `waitingCount > 0` and the offline banner is hidden. The probe stays only as the change trigger; `offlineCount` goes.

**Tests:**
- envio-core: `one_offline_account_is_named`, `several_offline_accounts_are_counted_and_named`, `waiting_mail_is_added_to_the_offline_text`, `offline_here_is_the_plain_text`.
- envio-ui (`ui/offline.rs`): `an_account_tab_ignores_other_offline_accounts`, `an_account_tab_shows_its_own_offline_state`, `the_unified_tab_names_every_offline_account`, `a_folder_tab_follows_its_account`, `no_banner_when_all_are_online`.
- QML (`tst_inbox.qml`, MockCore's `offlineText` returns a tagged string of its arguments): `test_offline_banner_asks_for_the_current_tab` (unified, account, folder), `test_offline_banner_rereads_when_an_account_changes_state`, `test_outbox_banner_shows_when_no_offline_banner`.

**Proving:** `cargo make ci`, `cargo make pot`, `cargo make qml-test`, `cargo make app-docker`.

---

### Task A4: Sign-in step shows the servers, with "Edit servers" (#93)

**Files:** modify `crates/envio-core/src/text/setup.rs`, `crates/envio-core/src/api/event.rs` (`Event::Detected` gains `servers`), `crates/envio-core/src/worker/setup.rs`; `crates/envio-ui/src/signals.rs`, `convert.rs`; `qt/src/core_object.rs` (`detected` signal gains `servers`), `qt/src/events.rs`; `qml/pages/accounts/SignInStep.qml`, `ServersStep.qml`, `WizardPage.qml`; `tests/qml/MockCore.qml`, `tests/qml/tst_wizard.qml`; `po/envio.aps.pot`.

**Produces:**
- `text::setup::servers(imap: &ServerAddr, smtp: &ServerAddr) -> String`: two lines, "Incoming (IMAP): {host}:{port}, {security}" and "Outgoing (SMTP): {host}:{port}, {security}"; `{security}` is "TLS" or "STARTTLS" (not translated). Sent for every source, not only the guess.
- `Event::Detected { email, setup, text, servers }` → `Signal::Detected { …, servers }` → qt `detected(email, text, needsConfirm, setupJson, servers)`.
- `SignInStep`: `property string serversText`, a `Text` (`objectName: "serversText"`, PlainText) under the found text, and an `AppButton` "Edit servers" (`objectName: "editServers"`, not primary, disabled while busy) emitting `editServersRequested()`.
- `ServersStep.prefill(setup)`: sets username, hosts, ports and the two STARTTLS toggles from the setup JSON object; `prefill(null)` resets to the defaults (empty hosts, 993, 587, TLS). WizardPage: `onEditServersRequested: { root.forget(); servers.prefill(root.setup); root.step = "servers" }`; "Set up manually" calls `servers.prefill(null)` first, so it still starts empty. Hosts typed or kept on the Servers step count as confirmed (existing manual path).

**Tests:**
- envio-core: `servers_text_names_both_hosts_ports_and_security`; worker setup test `detected_carries_the_servers_text` (existing detect harness with the test-hooks fake `Found`).
- envio-ui: `detected_signal_carries_servers`.
- QML (`tst_wizard.qml`): `test_sign_in_shows_the_servers_text`, `test_edit_servers_opens_the_servers_step_prefilled` (hosts, ports, STARTTLS from a guessed setup), `test_manual_setup_starts_empty_after_edit_servers`, `test_edit_servers_is_disabled_while_busy`; Carbon and Airmail.

**Proving:** `cargo make ci`, `cargo make pot`, `cargo make qml-test`, `cargo make app-docker`.

---

### Task A5: Text columns use the width in layout-table mail (#90)

**Known:** Qt 5.15 resolves a `width="100%"` table nested in a cell against the cell's minimum width; beside a fixed cell (`width="76"`, an illustration) the text column shrinks to a few characters. The owner's desktop measurement on a real receipt at 360 px: 1836 px as rendered, 1222 px with `%` dropped on `td`, 1046 px with every `td` width dropped. The width pass is the streaming tag rewrite in `images::fit` + `widths.rs` (no tree), so linearising needs its own pre-scan.

**Files:** create `crates/envio-mime/tests/corpus/notice-like.eml` (synthetic Stripe-style notice: a 76 px illustration cell beside a text cell that holds nested `width="100%"` single-column tables, a long paragraph, a button table; invented text); create `crates/envio-mime/src/linear.rs` (under 250 lines); modify `widths.rs`, `sanitize.rs` (call `linear::linearise` on the output of `images::fit`), `lib.rs`; modify `crates/envio-mime/tests/widths.rs`, `tests/corpus.rs` (snapshot for the new file), new `crates/envio-mime/tests/linear.rs`; `tests/qml/tst_rich_text.qml`.

**Rule (decided after reading the code and the corpus):**
1. Percentage widths on `td`/`th` are dropped: `carry` no longer carries a CSS `%` for cells, and `sane_attribute` gains the tag name (`sane_attribute(tag: &str, value: &str)`) and drops a `%` attribute on `td`/`th`. Its only caller is `limit_width` at `images.rs:296`; change that one call to pass the tag and nothing else in `images.rs`, which must stay at or under 300 lines. Tables keep `%`; cells keep pixel widths (spacers).
2. Single-column layout tables are linearised: a table with no `th` and no `caption` in which every own `tr` has exactly one own cell (any `colspan`). Its own `table` and cells become `div` (keeping `align` and `style`, dropping the rest), its own `tbody`/`thead`/`tfoot`/`tr` tags are dropped. Nested tables are judged on their own. `linear::layout_tables(html) -> Vec<bool>` (one entry per `<table` start, in order) is the pre-scan; `linearise(html) -> String` rewrites using it. The input is the sanitiser's balanced output.
3. Only if the QML check still fails after 1 and 2: drop `width="100%"` on a table whose parent is a cell. If it still fails, stop and report the measured widths; don't guess a fourth rule.

**Invariants:** data tables (any `th`, or any row with two or more cells) keep their rows and columns; text and element order are unchanged; nothing gets wider than `max_image_width`; the attack and hidden-content tests pass unchanged; `rendering_scales_linearly` still passes. Existing `tests/widths.rs` cases that use a single-cell table to test carrying (e.g. `css_percent_width_on_a_table_becomes_the_width_attribute`) get a second cell so they still test width carrying, not linearising. Review every changed snapshot in the diff.

**Steps:**
- [ ] 1. Add `notice-like.eml` and its snapshot; add the QML check (below) and run `cargo make qml-test` to see it fail on the notice; record the measured column widths.
- [ ] 2. Write `tests/linear.rs` and the `widths.rs` test changes (failing), then rule 1, then rule 2.
- [ ] 3. Re-run the QML check and record the after-numbers in the task report and the test comment.

**Tests (`crates/envio-mime/tests/linear.rs` and `widths.rs`):**
- `percent_widths_on_cells_are_dropped` (attribute and CSS), `pixel_widths_on_cells_are_kept`, `percent_width_on_a_table_is_kept`.
- `a_single_column_table_becomes_blocks`, `nested_single_column_tables_in_a_two_column_row_become_blocks` (the outer row stays a table), `a_table_with_th_is_never_linearised`, `a_two_column_table_is_kept`, `linearising_keeps_text_order_and_alignment`, `colspan_cell_alone_in_its_row_counts_as_one`.

**Tests (QML, `tst_rich_text.qml`):**
- `columnWidth(edit, marker)`: in the reader's read-only `TextEdit` at width 360, find `marker` in `edit.getText(0, edit.length)` and take the x-extent of `positionToRectangle` over that paragraph's characters.
- `test_notice_like_text_column_uses_most_of_the_width` and `test_receipt_like_text_column_uses_most_of_the_width`: the marker paragraph's extent ≥ 0.6 × 360 = 216 px (the pass criterion of #90). The notice's height also gets a bound (3× the measured value, number and source in a comment), and `check()`'s 1 s layout bound applies to it.

**Proving:** `cargo make ci`, `cargo make qml-test`, `cargo make app-docker`.

---

### Task A6: Timing tests assert order, not wall time (#99, #80)

**Files:** modify `crates/envio-core/tests/drafts.rs` (`a_slow_attachment_copy_does_not_delay_saves_or_the_flush`), `crates/envio-e2e/tests/ui_stalwart.rs` (`a_draft_survives_a_slow_first_sync`).

**#99:** the copy stays blocked until `release`, so ordering proves the point without a deadline: wait for the "typed while copying" draft on disk with a long bound (10 s) *before* `release`; if the save waited for the copy, it can't land before `release` and the wait fails. Drop `on_time`. The flush: send `AppActive(false)` on a helper thread and wait for it with `recv_timeout(10 s)` before `release`; the draft "typed again" is on disk when it returns. If the harness can't be shared with a thread, keep the existing 2 s flush bound (it has not flaked) and say so. Assertion messages keep naming the fault ("a text save waited for the copy", "the flush waited for the copy").

**#80:** the limit stays at master's 1.8 s (since 98b2e3a, which already answers the flake) and the measured latency is printed on success: `eprintln!("draft on disk {} ms after the change", ms)` (visible with `--nocapture` and in the failure message). The 3 s proxy hold still separates a save that waited for the sync. If the PR's CI run logs more than 1.5 s, note it in #80 instead of widening. The unit tests in `drafts/book.rs` stay the exact check of the 1000 ms due time.

**Tests:** the two tests above, run 20 times locally under load (`for i in $(seq 20); do cargo test -p envio-core --test drafts a_slow_attachment || break; done` while `cargo build --release` runs) and the e2e test 5 times with `cargo make e2e-it`; report the latencies.

**Proving:** `cargo make ci`, `cargo make e2e-it`.

---

### Task A7: Phone check (controller)

The controller installs the build (`cargo make click-install`) and takes every screenshot with `ute2e shot` (never handed back to the user), both themes where the screen changed, kept in `docs/phone-runs/release-polish/`.

- [ ] #97: add `ivan@envio.test` with the uAdBlockNG hosts file in place: CPU time of the setup thread, VmRSS/VmHWM before and after (`/proc/<pid>/status`), seconds from Continue to Sign-in.
- [ ] #93: that Sign-in step showing the guessed servers; after "Edit servers", the Servers step pre-filled.
- [ ] #91: with one account unreachable (no adb reverse): its tab shows "Offline.", the other account tabs show no banner, the unified tab names it.
- [ ] #92: a synthetic message from `"billing@paypal.com" <x@evil.example>` sent to the test account: list row and reader header.
- [ ] #90: the synthetic `notice-like` and `receipt-like` sent to the test account, reader at full width.
- [ ] Update `docs/phone-checklist.md` rows for each.

---

## Part B: CI (one PR, branch `plan8c-ci`)

### Task B1: The arm64 click build in CI (#7)

**Known:** `cargo make click` runs `clickable build --arch arm64` on the host, and clickable starts its own Docker container with a bind mount of the project. CI jobs reach only the runner's dind daemon (no docker CLI, no `DOCKER_HOST`; `tools/ci/stalwart.sh` shows how to reach it), and a bind mount of the job container's path doesn't exist on that daemon.

**Files:** create `.forgejo/workflows/click.yml`, `tools/ci/click.sh`; modify the spec's "Modular CI" bullet ("Always: … and the clickable build") to say when the click job runs.

**Produces:**
- `click.yml`: `on: push: branches [master], tags ["v*"]`, `workflow_dispatch`, and `pull_request: paths: [clickable.yaml, manifest.json, envio.apparmor, CMakeLists.txt, "qt/**", ".forgejo/workflows/**", tools/ci/click.sh]`. One job `click`, `runs-on: docker`, concurrency as `ci.yml`. Steps: checkout (`persist-credentials: false`), `tools/ci/click.sh`, upload `build-click/*.click` as an artifact (`actions/upload-artifact@v3`, Forgejo's artifact API; if the instance refuses it, `https://code.forgejo.org/forgejo/upload-artifact@v4`), 30-day retention. Not a required check (it doesn't run on every PR).
- `tools/ci/click.sh`: installs the pinned docker CLI and finds the dind gateway as `stalwart.sh` does (share the code by sourcing a small `tools/ci/dind.sh` if it stays simple, without changing `stalwart.sh`'s behaviour); creates a container from the pinned clickable arm64 image; copies the source in with `git archive HEAD | docker cp - "$c:/src"` (no bind mount); runs `clickable build --arch arm64 --container-mode` and `clickable review --arch arm64` in it with `ENVIO_COMMIT` set from the host checkout; copies the `.click` and the review output out with `docker cp`; removes the container in a trap. Fails on a build error or a click-review error (warnings are printed, not fatal).
- The image: the one `clickable build --arch arm64 --verbose` uses locally for `ubuntu-touch-24.04-1.x`, pinned by digest. If it lacks clickable or `click-review`, install the clickable version the controller uses locally (`clickable --version`) with pip inside the container, pinned; if that needs more than one install line, stop and report (a `ut-click-ci` image in the infrastructure repo is the owner's call).

**Invariants:** no secrets used; the job never pushes or publishes; `ci.yml` is unchanged.

**Proving:** the Part B PR touches `.forgejo/workflows/`, so its own run builds the click: the job is green, the artifact downloads, `click info` on it shows `envio.aps`, the version from `manifest.json` and architecture `arm64`. Also break `envio.apparmor` on a scratch commit, see the job fail in click-review, then drop the commit. Report the job's duration.

---

### Task B2: Security-only dependency updates (#25)

**Files (Renovate):** create `renovate.json`, `.forgejo/workflows/renovate.yml`. **Files (fallback):** create `.forgejo/workflows/advisories.yml`, `tools/ci/advisory-issue.mjs`.

**Renovate (try first):**
- `renovate.json`: `{"enabledManagers": ["cargo"], "packageRules": [{"matchPackageNames": ["*"], "enabled": false}], "osvVulnerabilityAlerts": true, "vulnerabilityAlerts": {"enabled": true, "labels": ["security"]}, "lockFileMaintenance": {"enabled": false}, "dependencyDashboard": false}`. Both `Cargo.lock` files are covered (the root workspace and `qt/`).
- `renovate.yml`: `schedule: cron "23 4 * * *"` and `workflow_dispatch` with inputs `base` (default `master`) and `dry_run` (`full` or empty). Job on `runs-on: docker` with `container: ghcr.io/renovatebot/renovate:<exact version ≥ 2 weeks old>`; one `run: renovate` step with `RENOVATE_PLATFORM=forgejo` (`gitea` on images without it), `RENOVATE_ENDPOINT=https://code.austinschaefer.com/api/v1`, `RENOVATE_REPOSITORIES=schaefera/envio`, `RENOVATE_BASE_BRANCHES=${{ inputs.base || 'master' }}`, `RENOVATE_DRY_RUN=${{ inputs.dry_run }}`, `RENOVATE_ONBOARDING=false`, `RENOVATE_REQUIRE_CONFIG=required`, token `${{ secrets.GITHUB_TOKEN }}` (the workflow's built-in token). No new secret.

**Proof (no new tokens, nothing left behind):**
- [ ] 1. Push a scratch branch `renovate-proof` from the Part B branch's head (it must carry `renovate.json` and `renovate.yml`; master has neither) that pins one crate with a known RustSec advisory and a patched release, in `xtask`'s dev-dependencies so nothing ships it, and also has an ordinary outdated dependency. The crate must be absent from both `Cargo.lock` files (same major), or the pin cannot resolve: `smallvec`, `time` 0.3 and `crossbeam-channel` are already locked at newer versions. Candidate: `remove_dir_all = "=0.5.3"` (RUSTSEC-2023-0018, patched in 0.8.0). Before pushing, run `cargo generate-lockfile` and `cargo audit` on the scratch tree: exactly that one advisory, with a patched version, or pick another crate.
- [ ] 2. Dispatch `renovate.yml` on ref `renovate-proof` (so the workflow file exists) with `base=renovate-proof`, `dry_run=full`. Pass: the log plans exactly one branch, a vulnerability fix for the pinned crate, and nothing for the ordinary outdated one.
- [ ] 3. Dispatch again without dry run. Pass: a PR against `renovate-proof` is opened with the `security` label. Close it, delete its branch and `renovate-proof`.
- [ ] Renovate is kept only if 2 and 3 pass. If the built-in token is refused (Renovate's `/user` call, push or PR creation) or step 2 shows ordinary updates or no fix, remove the two Renovate files and use the fallback. Record which, with the run links, in the PR and in #25.

**Fallback:**
- `advisories.yml`: `schedule: cron "23 4 * * *"`, `workflow_dispatch` (input `base`); `runs-on: rust-ci` (it has `cargo audit`, as `ci.yml`'s audit job, and node for actions); runs `cargo audit --json` on both lock files and `node tools/ci/advisory-issue.mjs` with `GITHUB_TOKEN`.
- `advisory-issue.mjs` (node 18+, built-in `fetch`, no dependencies): from the two JSON reports, the advisory IDs, crates, versions and fixed versions; finds the open issue titled "Security advisories in locked dependencies" (API search, labels `security`); when there are advisories, creates it or updates its body (sorted table, so a rerun changes nothing) and comments only when the set of IDs changed; when there are none and the issue is open, comments and closes it. Exit code 0 unless the API call fails.
- Proof: the same scratch branch, dispatched with `base=renovate-proof`: the issue is opened; a second dispatch leaves it unchanged (no new comment); a dispatch on master closes it. If the built-in token can't write issues, stop: a bot token is the owner's decision (comment on #25 with the error).

**Proving:** the run links above; `node --check tools/ci/advisory-issue.mjs` if the fallback is used.

---

### Task B3: Empty test selection, live (#16) (controller)

No code. This plan is committed alone in a docs-only PR before Parts A and B start. On that PR's CI run, check the `rust` job: "Pick crates to test" prints `Testing: no crates changed`, "Unit and crate integration tests (changed crates)" is skipped, the cross-crate step still runs, and the job is green. Close #16 with a link to that run and job.

---

## Done when

The plan PR is merged and #16 closed; Part A is merged with #97, #92, #91, #93, #90, #99 and #80 closed (phone screenshots named in each closing comment for #97, #93, #91, #92, #90; latencies for #80); Part B is merged with #7 closed (link to the master run and its artifact) and #25 closed with the chosen route and its proof runs; `docs/phone-checklist.md` has the new rows; CI is green on master.

## Self-review

- Coverage: #97 A1, #92 A2, #91 A3, #93 A4, #90 A5, #99 and #80 A6, phone A7; #7 B1, #25 B2, #16 B3.
- Type consistency: `BannerTab` ↔ `offlineText(tab, folder, waiting)` ↔ InboxPage `root.tab`/`root.folderId`; `Event::Detected.servers` ↔ `Signal::Detected.servers` ↔ qt `detected(…, servers)` ↔ WizardPage `onDetected(email, text, needsConfirm, json, servers)` ↔ `SignInStep.serversText`; `race::Settle` used only by `find`.
- Numbers decided at run time have a rule: A5's notice height bound (3× measured), A6's note line (1.5 s logged).

## Pre-flight scan

Part A runs in order in one worktree; Part B in another, in parallel with Part A.

| Overlap | Tasks | Why it matters |
|---|---|---|
| `qt/src/core_object.rs` (311 lines), `qt/src/events.rs` | A3, A4 | bodies in `status_calls.rs`; only declarations grow here |
| `tests/qml/MockCore.qml` | A3, A4 | each adds members (`offlineText`, `detected` arg) |
| `crates/envio-ui/src/signals.rs`, `convert.rs` | A4 | `Detected.servers` |
| `crates/envio-core/src/worker/rows.rs`, `read.rs` | A2 | one `sender_label` for list and reader |
| `po/envio.aps.pot` | A3, A4 | run `cargo make pot` at the end of each task; regenerate on conflict |
| `tests/qml/tst_inbox.qml` | A2, A3 | stamps; banners |
| `crates/envio-mime/src/sanitize.rs`, `widths.rs` | A5 | linearise after `images::fit`; `images.rs` (300 lines) changes only the `sane_attribute` call |
| `.forgejo/workflows/` | B1, B2 | separate files; `ci.yml` untouched by both |
