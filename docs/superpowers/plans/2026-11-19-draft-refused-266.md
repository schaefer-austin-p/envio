# Envio Plan: say why a draft is not on the server (#266)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When the server refuses a draft upload, the "Not uploaded" chip on its Drafts row carries the reason in at most four words: "Mailbox full", "Drafts read-only", "Too large", "Server refused". Refused drafts keep retrying with a growing delay instead of giving up after 5 runs, and a later success clears the reason.

**Design:** option **A+B, "Reason in the chip"**, chosen by the owner, in `docs/design/carbon-draft-refused.html` and `airmail-draft-refused.html` (section "A+B"). The row layout is the #228 board's (`carbon-draft-edit.html`). The chip uses tint A, so there is no new colour role.

**Depends on** #228 T4 (merged: `notUploaded`, `draftId`, `opens`, `LocalDraftsChanged`) and, for T5, on #228 T6 (the QML chip, not merged yet).

**Architecture:** the refusal text already reaches `envio-sync` as `NetError::Refused(String)`; the driver drops it today (`driver/drafts.rs:105-112` calls `mark_failed(&id, updated_at_ms, 1)` only). A pure classifier in `envio-sync` turns the text into `RefusalKind`, `envio-store` keeps the kind (never the server text) in `ServerCopy`, `envio-core` builds the translated label into the pseudo-row, `envio-ui` passes it as a role, and QML shows it. No SQL migration, no cxx-qt change (`qt/src/models.rs:158-176` maps roles generically).

## Global Constraints

- Dependencies point one way: ui → core → sync → net, store, mime. The enum lives in `envio-store`, the classifier in `envio-sync`, the text in `envio-core`. `envio-net` is not touched.
- Logic in Rust. QML shows `model.notUploadedText` and never branches on a reason.
- User-facing text: gettext in Rust (`crates/envio-core/src/text/`, `tr` in `i18n.rs:6`), domain `envio.aps`, no single quotes in strings. Run `cargo make pot` after string changes; do not commit a date-only pot refresh.
- The draft file keeps no server text, only the enum.
- Files stay around 300 lines. New code goes in new modules.
- `cargo make ci` passes at every commit. T5 also runs `cargo make qml-test` and `cargo make app-docker`.
- Test addresses use `example.org` only.

## Decisions

Decisions 1 to 7 were taken on the owner's behalf after the feasibility check; the owner may override any of them. Decisions 8 to 11 were added while writing this plan and are on the same footing.

1. **"Too large"** is `[TOOBIG]`, or the words "too large", "too big" or "size" inside a NO that is already `Refused` (a best guess). `[LIMIT]` stays busy (`is_busy`, `envio-net/src/imap/client.rs:118`): it ends the run as `Temporary` and shows "Not uploaded".
2. **A lost connection or offline** keeps a stored reason: it is still true.
3. **BAD (`Protocol`) retries too**, with the same backoff. `GIVE_UP` is removed. A BAD is stored as `Other`.
4. **Retries back off:** the delay after the n-th failure of one save is `min(5 min * 2^(n-1), 1 h)`, checked in the pure planner `needs_upload(draft, now_ms)`. A new save (`failed_ms != updated_at_ms`) and a manual refresh reset it. `LocalDraftsChanged` is sent only when a stored reason changes.
5. **The reason shows only while `failed_ms == updated_at_ms`.** A newer save shows "Not uploaded".
6. **Store `RefusalKind { OverQuota, ReadOnly, TooLarge, Other }`**, never the server text. The label is built in core with gettext.
7. **A local NO** (`Error::Validate`, `Error::Append`, `client.rs:111-112`, also `Refused`) shows "Server refused". Accepted; noted on the board.
8. **Classifier input and order.** The text is `NetError::Refused`'s string, which has the form `code: <Option<Code>>, info: <Option<str>>` (`client.rs:128-132`). `imap-proto` 0.17 does not parse OVERQUOTA, NOPERM, TOOBIG or LIMIT, so they stay in `info`; READ-ONLY appears as `code: Some(ReadOnly)`. Match case-insensitively, first hit wins: OverQuota (`overquota`, `quota`), ReadOnly (`ReadOnly` code, `noperm`, `permission`, `read-only`), TooLarge (Decision 1), else Other. `TRYCREATE` is `Other`.
9. **Field and writer.** `ServerCopy` gains `refusal: Option<RefusalKind>` and `failed_at_ms: i64`, both serde-default (old files load). `mark_failed` takes the kind and the time. `mark_uploaded` rebuilds `server` from the default and so clears both, which is how a later success clears the reason (the A reset). `mark_tried` leaves them alone.
10. **A manual refresh** clears `failed_at_ms` of the account's drafts through a new `Drafts::clear_backoff(account)`, called where core starts a manual full run (check `worker/plan.rs` before coding; the plan found full runs at `plan.rs:162-183`, every `POLL_MS` = 5 min). The count stays; only the wait is lifted.
11. **T5 extends #228 T6's chip; it does not fold into it.** #228 T6 waits for #284 and is written against ui roles that are merged. Folding the text binding into it would make T6 wait for T1 to T4 here, and would mix two reviews. T5 is a few lines on top (`qsTr("Not uploaded")` becomes `model.notUploadedText`). If T1 to T4 finish first, T5 waits for #228 T6.

## Tasks

Order: T1 → T2 and T3 (T3 needs only T1's field) → T4 → T5 → T6. T1 to T4 are trunk.

---

### T1: Store the refusal (store)

**Tier:** trunk (`envio-store`). **Testing:** unit.

**Files:** `crates/envio-store/src/drafts_server.rs` (ServerCopy fields, `mark_failed`, `clear_backoff`), new `crates/envio-store/src/refusal.rs` (the enum, `Serialize`/`Deserialize`, `Copy`). ServerCopy literals to update: `envio-core/src/drafts/import.rs:117`, `drafts/lookup.rs`, `envio-core/tests/edit.rs:321`, and sync tests.

**Signatures:**
- `pub enum RefusalKind { OverQuota, ReadOnly, TooLarge, Other }`
- `Drafts::mark_failed(&self, id: &str, saved_ms: i64, weight: u32, refusal: RefusalKind, at_ms: i64) -> Result<bool>`
- `Drafts::clear_backoff(&self, account: &str) -> Result<usize>`

**Invariants:**
- Old draft files without the new fields load as `None` and 0, no migration.
- `mark_failed` for a different `saved_ms` than the stored `failed_ms` starts the count at `weight` (as today) and replaces the kind.
- `mark_uploaded` leaves `refusal: None, failed_at_ms: 0`.
- `mark_tried` and `mark_uploading` keep the kind.

**Tests:** old-file load; kind and time stored by `mark_failed`; new save resets the count and replaces the kind; `mark_uploaded` clears; `clear_backoff` zeroes `failed_at_ms` of that account only and keeps `failures`.

---

### T2: Sync: classifier, backoff, driver (sync)

**Tier:** trunk (`envio-sync`). **Testing:** unit (table test) plus `crates/envio-sync/tests/` with the fake server.

**Files:** new `crates/envio-sync/src/plan/refusal.rs`; `plan/drafts.rs` (`needs_upload`, drop `GIVE_UP`, `:18`, `:26-32`); `driver/drafts.rs:105-112` (pass kind and time; BAD gives `Other`); `DraftsSynced` gets `reasons_changed: bool`.

**Signatures:**
- `pub fn classify(refusal_text: &str) -> RefusalKind` (pure)
- `pub fn retry_delay_ms(failures: u32) -> i64`
- `pub fn needs_upload(draft: &Draft, now_ms: i64) -> bool` (the driver passes `run.now_ms`)

**Invariants:**
- `classify` follows Decision 8, case-insensitive, first hit wins.
- `retry_delay_ms(1) = 5 min`, doubling, capped at 1 h, no overflow for any `u32`.
- A draft with `failed_ms == updated_at_ms`, `failures > 0` and `now_ms < failed_at_ms + retry_delay_ms(failures)` is not due. Every other rule of `needs_upload` is unchanged.
- `Busy` and `Io` never reach `mark_failed` (Temporary path), as today.
- `reasons_changed` is true when a draft's stored kind differs after the run (a new kind, or one cleared by upload).

**Tests:**
- Table test for `classify`: `OVERQUOTA`, `NOPERM`, `code: Some(ReadOnly)`, `TOOBIG`, "message too large", `[LIMIT]` (gives `Other`; busy is decided earlier in `net`), a bare NO, `TRYCREATE`, mixed case.
- `plan/drafts.rs:150-156` (the give-up test) becomes a backoff test: not due inside the delay, due after it, due after a new save.
- `crates/envio-sync/tests/drafts.rs` `a_refused_draft_is_retried_then_left_alone_until_it_changes` (~:183-196) becomes: retried with growing waits, never left alone; kind stored; a later success clears it.
- BAD stores `Other` and retries.

---

### T3: Core: label, row text, event on change (core)

**Tier:** trunk (`envio-core`). **Testing:** unit plus core integration (existing fake-server support).

**Files:** new `crates/envio-core/src/text/draft_chip.rs` (one function per label, with a translator comment: keep to about 20 characters); `api/view.rs:56-80` (`MessageRow.not_uploaded_text: String`); `worker/local_rows.rs:99-112` (fill it); `worker/run/mail.rs:~121-129` (emit `LocalDraftsChanged` when `done.reasons_changed`, not on every refused id); the manual-refresh path (Decision 10).

**MessageRow literal sites** (add `not_uploaded_text: String::new()`): `worker/rows.rs:67`, `local_rows/tests.rs:163`, `envio-ui/tests/support/mod.rs:15`, `envio-ui/tests/drafts.rs:9`, `envio-e2e/tests/support/core_run.rs:~255`.

**Signatures:** `fn chip_text(server: &ServerCopy, updated_at_ms: i64) -> String`: "Not uploaded" unless `failed_ms == updated_at_ms` and `refusal` is `Some`, then "Mailbox full", "Drafts read-only", "Too large" or "Server refused".

**Invariants:**
- A server row has an empty `not_uploaded_text`.
- A pseudo-row always has a non-empty one.
- No event when the sync run changed nothing the user sees.

**Tests:** `chip_text` per kind and the newer-save case; `local_rows/tests.rs:126-129` updated; a run that stores the same kind twice emits one event; manual refresh clears `failed_at_ms`.

---

### T4: UI role (ui)

**Tier:** trunk (`envio-ui`). **Testing:** unit plus `crates/envio-ui/tests/drafts.rs`.

**Files:** `crates/envio-ui/src/rows.rs` (`MessageRowUi.not_uploaded_text`, role `notUploadedText` in `MESSAGE` `:177-195` and `value()` `:267`); `ui/messages.rs:~140-165` (copy from the row).

**Invariants:** `notUploaded` stays; `notUploadedText` is empty exactly when `notUploaded` is false. No change in `qt/`.

**Tests:** role present and per-row value; server row is empty; a changed reason produces an `Update` op for that row, not a reset.

---

### T5: QML (after #228 T6 merges)

**Tier:** leaf (`qml/pages/**`), but if the chip's colours need `qml/platform/Theme.qml` that file is trunk. **Testing:** `cargo make qml-test`, `cargo make app-docker`, `cargo make pot`.

**Files:** the chip #228 T6 adds in `qml/pages/inbox/MessageRow.qml`; one new component `qml/pages/inbox/StatusChip.qml`; the upload-off glyph in the icon set.

**Behaviour:** the chip is a **new 24 px status pill, not `Chip.qml`** (that is the 44 px filter button). It has the upload-off icon, the text from `model.notUploadedText`, one line, elided, tint A. Its ink comes from theme tokens, so it is right in the dark theme. No `qsTr` for the labels: they come from Rust.

**Invariants:** no swipe handler; no Lomiri API outside `qml/platform/ut/`; the label never wraps (German and French labels are about 215 to 228 px of the 328 px row, so it fits; the elide is the guard).

**Tests (QML):** chip shows `notUploadedText` when `notUploaded`, hidden otherwise; long text elides on one line; dark theme uses the token ink.

---

### T6: Phone, e2e decision, docs

**Tier:** leaf (docs, `envio-e2e`). **Testing:** phone shots, design review.

- **E2E with Stalwart: no, not in this plan.** `crates/envio-net/tests/stalwart/up.sh` creates a domain and throttles through JMAP and sets no quota or read-only folder, so Stalwart cannot be made to send `OVERQUOTA` without new setup that nobody has tried. Coverage instead: `classify` table test (T2), the fake-server refusal tests in `envio-sync/tests/drafts.rs`, and `envio-net/tests/imap.rs:466-490`, which already feeds `NO [OVERQUOTA]`. If a spike finds Stalwart's per-account quota settable over the management API, add one `envio-e2e` test then; that is a separate issue.
- **Phone** (ask first; the phone is shared): `ute2e shot` of a Drafts list with a "Mailbox full" row (the sync fake or a debug draft file with `refusal` set), then a separate agent compares the shot with the board.
- **Docs:** `docs/10-design.md` row for `*-draft-refused`; the spec's Drafts paragraph (`docs/superpowers/specs/2026-11-07-envio-design.md`, ~line 121) gets one sentence on the reasons and the backoff. The PR that lands T5 closes #266.

## Out of scope

- Retry buttons, banners, toasts and account problem lines (options B, C and D of the boards). The longer advice ("Free up space to save it.") could go into a C banner later.
- Accounts with no Drafts folder (#228's out-of-scope item).
