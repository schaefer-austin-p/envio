# Envio Plan: reopen saved drafts (#228)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A saved draft can be reopened in Compose. A message in a Drafts-role folder opens in the reader with one **Edit** button instead of Reply, Reply all and Forward. Edit fills Compose from the local draft file, found by Message-ID, or else from the server message. Every local draft with no current server copy also shows in the Drafts list as a row with a **Not uploaded** chip (this covers #266's notice). Tapping that row opens Compose at once. A draft never shows twice, and saving never adds a second server copy.

**Design:** `docs/design/carbon-draft-edit.html` (the notes under each frame are part of the spec). The `docs/10-design.md` row is "Reopen a draft". It is Carbon only, and the chip uses tint A, so there is no new colour role.

**Depends on #284** (parallel PR): deleting a Drafts-folder message also deletes the local draft file and its attachment copies. This plan does not touch that path. T6 (the QML that shows Edit) must not merge before #284, or Edit could bring back a draft the user deleted. T1 to T5 can land first because they show nothing new.

**Architecture:** All the matching is done in `envio-core`: which local draft belongs to a server row, which local drafts get a pseudo-row, and how a server message becomes a draft. `envio-ui` carries two new row fields and one list flag. `qt/` adds roles, a property and a compose kind. QML only shows what it is given. No migration and no new crate. Drafts stay JSON files under `drafts/`, outside SQLite.

**Already there (do not rebuild):**
- `StartFrom::Draft` / `start_from_draft` in `crates/envio-core/src/worker/compose.rs:76` (it refuses with `draft_unavailable` when there is no file), and qt kind `"draft"` in `qt/src/convert.rs:96`.
- `ServerCopy { message_id, pending_id, uploaded_ms, failed_ms, failures }` in `crates/envio-store/src/drafts_server.rs`.
- Upload mints a new Message-ID and removes the copy it replaces (`crates/envio-sync/src/driver/drafts.rs:146`). The planner counts a draft as current when `message_id.is_some() && uploaded_ms == updated_at_ms` (`crates/envio-sync/src/plan/drafts.rs:25`).
- `GIVE_UP = 5`, `mark_failed`, and `DraftsSynced.refused`, which is only logged at `crates/envio-core/src/worker/run/mail.rs:97`.
- `messages.message_id` (indexed) and `Db::find_by_message_id(account, id)`.
- `start_from_message` / `finish_start` in `crates/envio-core/src/worker/start.rs` already wait for a missing or partial body, as Forward does.

## Global Constraints

- Dependencies point one way: ui → core → sync → net, store, mime. `envio-store` gets one read query and nothing from `envio-mime`. Parsing a server message into a draft happens in `envio-core`.
- Logic lives in Rust. QML only shows things: it branches on roles the Rust side sets (`opens`, `notUploaded`, `isDraft`, `listIsDrafts`), never on folder names or ids.
- Unsent work stays in files outside SQLite. Importing a server draft writes a draft file. Pseudo-rows are built from the files on every list read and are never cached in SQLite.
- User-facing text is translated: `qsTr()` in QML, gettext (`crate::text`) in Rust, domain `envio.aps`. Run `cargo make pot` after string changes. Do not commit a pot refresh that only changes the date.
- No swipe actions. Long-press selects, as everywhere else.
- Files stay around 300 lines. `worker/compose.rs` (434) and `worker/start.rs` already exceed that, so new code goes in new modules.
- `cargo make ci` passes at every commit. QML tasks also run `cargo make qml-test`. Tasks that touch `qt/` or `qml/` also run `cargo make app-docker`.
- Test addresses use `example.org` only.

## Decisions

1. **Message-IDs are matched in core, and the UI never sees them.** The feasibility check noted that `MessageRowUi` lacks `message_id`. It keeps lacking it. The reader sends the row id (`StartFrom::Edit { id }`) and core looks up the Message-ID. Reason: one place that knows about brackets and pending ids, and nothing extra crosses cxx-qt.
2. **"Has a current server copy" means:** the account's Drafts-role folder in the cache holds a message whose Message-ID equals the draft's `server.message_id` or `server.pending_id`, compared after `trim()`. A local draft without such a message gets a pseudo-row. Reason: the mockup says the server row takes the local row's place, matched by Message-ID, so a draft never shows twice. Matching `pending_id` as well covers the moment between APPEND and `mark_uploaded`. Known effect: right after a cache rebuild, before the Drafts folder syncs, uploaded drafts show as Not uploaded for one sync. That errs toward showing too much, never hiding.
3. **A local file is newer than the server copy.** When Edit finds a local draft by Message-ID, Compose opens the local file even if the reader showed older server text (an edit that has not uploaded yet). The server message is imported only when no local file matches.
4. **Import marks the draft as current.** The imported draft gets `server.message_id = <that message's Message-ID>` and `uploaded_ms = updated_at_ms`. Unchanged, it is never uploaded. After the first edit, the existing upload replaces that copy, because `replaced()` returns the old id. So the mockup's "first save replaces it on the server" needs no new sync code.
5. **Import waits for the whole body, as Forward does,** so attachments come along. They are copied into the draft's folder with `Drafts::keep_bytes`. With no connection and no cached whole body, Edit is refused with `text::read::message_unavailable()`, the same as Forward.
6. **Pseudo-row ids are negative.** The worker keeps `LocalIds`, a map from draft id to `i64` starting at -1 and counting down. Ids are stable for the session and never reused. Cached rows have positive SQLite rowids, so the two can't collide, and `ShownList` keys `(date, id)` keep working. The cursor is `{ date: updated_at_ms / 1000, id }`.
7. **Pseudo-rows are merged per page in core** (`list_messages` for a `ListRef::Folder` whose role is Drafts). A page shows the cached rows plus the local drafts whose key lies in the page's range (`(after, last cached key]`, or everything older when `end`), sorted newest first. `end` still comes from the cached page alone. The unified inbox, search and other folders never get pseudo-rows.
8. **Taps are routed by Rust.** A row has `opens: "reader" | "compose"`. Pseudo-rows open Compose (`startCompose("draft", 0, "", draftId)`) and every other row opens the reader, as the mockup says ("Tap opens it in Compose at once. Any other row opens the reader."). The feasibility note's "tapping a Drafts row opens Compose" is read as the pseudo-rows. A server row goes through the reader's Edit.
9. **Drafts rows show To instead of From** (server rows and pseudo-rows). Core fills `from` with the translated "To: %s", taking the recipients' names or addresses joined with ", " (or "(no recipients)"), and fills `from_address` with the first recipient so the stamp tint works. The stamp, preview and date stay where they are.
10. **List refresh:** a new `Event::LocalDraftsChanged { account }`. It is sent when a draft file is created, closed (saved or discarded), deleted (Send, the #284 path, T5's Delete), or changes upload state (after `sync_drafts`). It is not sent on every throttled keystroke save, so the list does not reload once a second behind Compose. `envio-ui` turns it into a first-page refresh when the current list is that account's Drafts folder.
11. **Selection:** pseudo-rows can be selected (mockup: "Long-press selects"). In a Drafts list the selection bar never offers Archive. While the selection holds a pseudo-row it offers only Delete. Delete of pseudo-rows goes through `Act { Delete }` with the usual Undo: core holds the draft ids for the Undo window and calls `Drafts::delete` when the window closes. If the app is killed during the window, the draft survives, which is the safe direction.
12. **Reader of a draft** (`MessageView.is_draft`, true when the message's folder role is Drafts): the bottom bar is one filled Edit button with a pencil, and Reply all leaves More. Archive is hidden. Delete, Move, Flag and Add sender stay. Reason: the mockup removes only the reply actions and Archive.
13. **One compose per draft.** Edit or a pseudo-row tap for a draft that an open compose already holds is refused with a new text, "This draft is already open." Today `DraftBook::adopt` would let two composes write over each other.

## Tasks

Order: T1 → T2 → T3 → T4 → T5 → T6 → T7. T1 to T3 are core (T3 adds one store query). T4 is `envio-ui` plus `qt/`. T5 is core plus ui. T6 is QML and waits for #284. T7 is e2e, phone and docs.

---

### T1: Edit a cached Drafts message that has a local file (core, qt kind)

**Tier:** trunk (`envio-core`, `qt/`). **Testing:** unit plus core integration (`crates/envio-core/tests/`, existing fake-server support).

**Files:** new `crates/envio-core/src/drafts/lookup.rs` (pure), `crates/envio-core/src/api/command.rs` (`StartFrom::Edit`), new `crates/envio-core/src/worker/edit.rs` (`start_edit`), `worker/compose.rs` (dispatch only), `drafts/book.rs` (`holder`), `api/view.rs` and `worker/read.rs` (`is_draft`), `text/compose.rs` (`draft_already_open`), `qt/src/convert.rs` (kind `"edit"`), `qt/src/core_object.rs` (`isDraft` on the reader view, if the view is not exposed as one object).

**Produces:**
```rust
// api/command.rs
pub enum StartFrom { …, /// A cached message in a Drafts-role folder.
    Edit { id: MessageRowId } }
// api/view.rs
pub struct MessageView { …, /// The message's folder has the Drafts role.
    pub is_draft: bool }
// drafts/lookup.rs
/// The local draft whose server copy is `message_id` (its `message_id` or
/// `pending_id`, trimmed), in `account`. Pure over a listing.
pub(crate) fn draft_for(drafts: &[(String, Draft)], account: &str, message_id: &str) -> Option<String>;
pub(crate) fn same_id(a: &str, b: &str) -> bool;
// drafts/book.rs
pub fn holder(&self, draft_id: &str) -> Option<ComposeId>;
```
`start_edit(compose, id)`:
1. The message must exist and its folder role must be Drafts. Otherwise refuse with `message_unavailable`.
2. Look up `message.message_id`. If `draft_for` finds a local file, continue as `start_from_draft(Some(id))` does, after the `holder` check.
3. If there is no match, continue with T2 (until T2 lands, refuse with `draft_unavailable`).

**Invariants:**
- Edit never matches a draft of another account.
- An empty or `<>` Message-ID never matches anything (use `envio_sync::plan::message_id::usable_message_id`, which is public, and core already depends on sync).
- A draft held by an open compose is not adopted a second time.

**Tests:**
- `draft_for` matches on `message_id`, matches on `pending_id`, ignores surrounding whitespace, misses another account, misses `<>`.
- Integration: a cached Drafts message plus a local file with the same `server.message_id` gives `ComposeReady` with the file's text.
- Integration: a non-Drafts message gives `ComposeRefused`.
- Integration: a second Edit while the first compose is open gives `ComposeRefused` with "This draft is already open."
- `is_draft` is true for a Drafts-role folder and false for the Inbox.
- qt: `start_from("edit", 7, …)` gives `StartFrom::Edit { id: 7 }`.

---

### T2: Import a server-only draft (core)

**Tier:** trunk (`envio-core`). **Testing:** unit plus core integration.

**Files:** new `crates/envio-core/src/drafts/import.rs` (pure), `worker/edit.rs` (wait for the body and finish), `worker/start.rs` (`compose_body_fetched` routes `Edit` waiters to `edit.rs`. Touch nothing else there).

**Produces:**
```rust
// drafts/import.rs
/// A local draft built from a server draft's raw bytes. Pure.
/// `server.message_id = Some(message_id)`, `uploaded_ms = updated_at_ms = now_ms`.
pub(crate) fn draft_from_raw(raw: &[u8], account: &str, message_id: &str, now_ms: i64) -> Imported;
pub(crate) struct Imported { pub draft: Draft, pub attachments: Vec<(String, Vec<u8>)> }
```
- Recipients come from To, Cc and Bcc, shown as `start::displayed` does.
- The subject is taken unchanged. In-Reply-To and References are carried over.
- The body is the text/plain part, else the HTML turned into text through envio-mime's existing helper. Pick the one Reply uses for quoting. Do not write a new converter.
- Flow:
  1. `Drafts::create`.
  2. `keep_bytes` for each attachment, then `save` with the paths.
  3. `adopt`, then `compose_ready`.

  If the body is missing or partial, wait for the whole body exactly as Forward does (`live.wants.bodies` plus `whole`, and `composing` with `StartFrom::Edit`).

**Invariants:**
- An unchanged import is never uploaded. The planner sees it as current (Decision 4).
- The first saved change uploads with `replaced() == Some(old)`, so the server copy is replaced, never duplicated.
- If an import fails halfway (a write error), `Drafts::delete` removes the new file and Compose is refused. No half-filled draft is left behind.

**Tests:**
- Unit: `draft_from_raw` on a multipart with an attachment gives the recipients, subject, body, `in_reply_to` and one attachment, and `server.message_id` equals the input.
- Unit: an HTML-only draft gives the text body.
- Integration: Edit on a cached Drafts message with no local file creates one draft file with `server.message_id` set and gives `ComposeReady`. A second Edit on the same message (after closing) finds that file through T1 instead of importing again.
- Integration: a body not yet cached gives `ComposeReady` after the fetch.
- Sync planner, unit in `envio-sync` if not already covered: a draft with `uploaded_ms == updated_at_ms` and `message_id` set plans no upload. After `updated_at_ms` grows it plans an upload that names the old id as replaced.

---

### T3: Pseudo-rows for local drafts in the Drafts list (core, one store query)

**Tier:** trunk (`envio-core`, `envio-store`). **Testing:** unit plus store integration plus core integration. This is the largest task. Keep the merge pure and the I/O in `read.rs` thin.

**Files:**
- `crates/envio-store/src/messages.rs`: `message_ids_in`, which reads the indexed column and adds no migration.
- `crates/envio-core/src/api/view.rs`: `MessageRow.local_draft`.
- `crates/envio-core/src/api/event.rs`: `LocalDraftsChanged`.
- New `crates/envio-core/src/worker/local_rows.rs`: the pure merge, `LocalIds`, the To line.
- `worker/read.rs`: `list_messages` calls the merge for a Drafts-role folder.
- `worker/rows.rs`: the To line for Drafts-role server rows.
- `text/read.rs` or a new `text/list.rs`: "To: %s", "(no recipients)", "Not uploaded".
- `drafts/keeper.rs` and `drafts/book.rs`: send `LocalDraftsChanged` on create and close.
- `worker/run/mail.rs`: send it after `sync_drafts` when anything was uploaded, refused, failed or deleted.
- `worker/compose.rs` and `worker/outbox.rs`: send it where a draft is deleted.

**Produces:**
```rust
// envio-store messages.rs
/// The Message-IDs (as stored) of the live messages in `folder`.
pub fn message_ids_in(&self, folder: FolderId) -> Result<HashSet<String>>;
// api/view.rs
pub struct MessageRow { …, /// Set on a pseudo-row: the local draft's id (no server copy yet).
    pub local_draft: Option<String> }
// api/event.rs
/// The local drafts of `account` changed in a way the Drafts list shows.
LocalDraftsChanged { account: String },
// worker/local_rows.rs
pub(super) struct LocalIds { /* draft id -> negative i64, next */ }
impl LocalIds { pub fn id_of(&mut self, draft_id: &str) -> MessageRowId; pub fn draft_of(&self, id: MessageRowId) -> Option<&str>; }
/// Local drafts of `account` with no current server copy (Decision 2), as rows.
pub(super) fn local_rows(drafts: &[(String, Draft)], account: &str, folder: FolderId,
                         on_server: &HashSet<String>, ids: &mut LocalIds) -> Vec<MessageRow>;
/// The cached page with the local rows in its range merged in, newest first (Decision 7).
pub(super) fn merge_page(page: Vec<MessageRow>, local: Vec<MessageRow>, after: Option<Cursor>, end: bool) -> Vec<MessageRow>;
pub(super) fn to_line(to: &[String]) -> String;
```
A pseudo-row has:
- `folder` set to the Drafts folder;
- `from` set to `to_line(draft.to)`, and `from_address` set to the first recipient's address;
- `subject` through `rows::subject` (so the placeholder works), and `preview` set to the first non-empty line of the body (trimmed, about 100 chars);
- `date` from `updated_at_ms / 1000`, with default flags, and `has_attachments` true when `!draft.attachments.is_empty()`.

**Invariants:**
- A draft never appears twice, either as two pseudo-rows or as a pseudo-row next to its server row.
- Every local draft of the account with no current server copy appears in exactly one page.
- Pages never overlap. The last key a non-end page shows is always a cached row, so the next `after` works against SQLite.
- The unified inbox, search and non-Drafts folders are unchanged, which existing tests check.
- `LocalDraftsChanged` is not sent on throttled text saves.

**Tests:**
- `merge_page`:
  - a local row inside the range is inserted at its place;
  - a local row older than the last cached row is left for the next page when `!end`;
  - all remaining local rows are added when `end`, including on an empty cached page;
  - a local row newer than `after` is excluded from a later page;
  - keys with the same date are ordered by id.
- `local_rows`:
  - a draft whose `message_id` is on the server gives no row;
  - a draft whose `pending_id` is on the server gives no row;
  - a never-uploaded draft gives a row;
  - a refused draft (`failures >= GIVE_UP`) gives a row;
  - another account's draft gives no row;
  - `LocalIds` keeps a draft's id across calls and never reuses one.
- `to_line`: names, addresses, and an empty list.
- Store integration: `message_ids_in` skips other folders and rows with a NULL Message-ID.
- Core integration:
  - `ListMessages` on a Drafts folder with one cached server draft and two local drafts (one matching the server row) gives two rows, and one of them has `local_draft`;
  - the Inbox list has no pseudo-rows;
  - closing a compose sends `LocalDraftsChanged`.

---

### T4: Rows, list flag and roles to QML (envio-ui, qt)

**Tier:** trunk (`envio-ui`, `qt/`). **Testing:** unit in `envio-ui` plus `cargo make app-docker`.

**Files:**
- `crates/envio-ui/src/rows.rs`: `MessageRowUi.draft_id`, `not_uploaded` and `opens`.
- The `envio-ui` conversion from `MessageRow`.
- `crates/envio-ui/src/ui.rs` and `ui/messages.rs`: handle `LocalDraftsChanged`, and add the `list_is_drafts` getter, from the current folder's role as the `Folders` event gave it.
- `ui/selection.rs`: `selection_has_local`.
- `qt/src/…`: the message model roles `draftId`, `notUploaded` and `opens`; the core properties `listIsDrafts` and `selectionHasLocal`; and the reader's `isDraft` if T1 did not add it.

**Produces:**
```rust
pub struct MessageRowUi { …, /// Empty for a server row.
    pub draft_id: String, pub not_uploaded: bool, /// "reader" or "compose".
    pub opens: &'static str }
impl Ui { pub fn list_is_drafts(&self) -> bool; pub fn selection_has_local(&self) -> bool; }
```
**Invariants:**
- `opens == "compose"` exactly when `draft_id` is non-empty.
- `LocalDraftsChanged` for another account, or while another list is shown, does nothing.
- The refresh goes through `merge_first`, so the user's scroll position and selection survive.
- A pseudo-row that disappears (because it uploaded) leaves the selection through `prune_selection`.

**Tests:**
- A `MessageRow` with `local_draft` converts to `opens == "compose"` and `not_uploaded`.
- `LocalDraftsChanged` on the shown Drafts list gives one first-page `ListMessages`. On the Inbox it gives none.
- `list_is_drafts` follows the folder role.
- After an upload, the merged page replaces the pseudo-row with the server row (a remove and an insert, no duplicate).

---

### T5: Delete selected pseudo-rows with Undo (core, ui)

**Tier:** trunk (`envio-core`, `envio-ui`). **Testing:** unit plus core integration.

**Files:**
- `crates/envio-core/src/worker/act.rs`: split negative ids off before `action_kind`.
- New `worker/act_local.rs`: hold the ids, delete them on expiry, put them back on Undo.
- `undo.rs` or the `Offered` enum: a new variant `LocalDrafts { ids: Vec<String> }`.

**Behaviour:**
- `Act { ids, Delete }` with negative ids resolves them through `LocalIds`. Core hides them from the next list reads at once, through a "held" set in `local_rows`. It sends `LocalDraftsChanged` and one `UndoOffer`, together with any server ids in the same Act.
- On expiry, `Drafts::delete` runs on each id. That writes the server-delete note if the draft ever had a server copy, so a stale copy is removed too.
- On Undo, the held set is cleared and `LocalDraftsChanged` is sent.
- Every other action on negative ids skips those ids. The UI does not offer such actions (Decision 11).

**Invariants:**
- No draft file is deleted before the Undo window closes.
- A held draft that a compose already holds is not deleted. The Delete skips it and logs it.
- A crash during the window loses nothing.

**Tests:**
- Delete of one pseudo-row hides it, and its file still exists.
- Expiry deletes the file and its `files/<id>/` folder.
- Undo brings the row back.
- A mixed Act (one server row, one pseudo-row) gives one offer, and Undo restores both.
- MarkRead on a pseudo-row changes nothing and does not panic.
- envio-ui: `selection_has_local` is true while a pseudo-row is selected.

---

### T6: QML (after #284 merges)

**Tier:** leaf (`qml/pages/**`). If anything in `qml/Main.qml` or `Theme.qml` changes, that file is trunk. None is expected. **Testing:** `cargo make qml-test` plus `cargo make app-docker`, then `cargo make pot`.

**Files:**
- `qml/pages/reader/ReplyBar.qml`: when `isDraft`, one filled `AppButton` with `objectName: "edit"`, a pencil icon and `qsTr("Edit")`.
- `ReaderMore.qml`: hide `moreReplyAll` when `isDraft`.
- `ReaderPage.qml`: hide `act-archive` when `isDraft`. `edit()` calls `core.startCompose("edit", messageId, view.account, "")`, and then `composeOpened(id)` as `compose()` does.
- `qml/pages/inbox/MessageRow.qml`: a `Chip` with tint A and `qsTr("Not uploaded")` when `model.notUploaded`. Its ink comes from the existing tint A theme values.
- `MessageList.qml:96`: when not selecting, `model.opens === "compose"` emits a new `draftRequested(model.draftId)`, and every other row emits `messageRequested`.
- `InboxPage.qml`: `onDraftRequested` calls `core.startCompose("draft", 0, "", draftId)` and opens Compose like the "new" path in `AppShell.qml:92`. Move that wiring to AppShell if InboxPage has no compose hook.
- `SelectionBar.qml`: hide Archive when `core.listIsDrafts`. Hide Move and More when `core.selectionHasLocal`.

**Invariants:**
- No swipe handlers.
- Every new string uses `qsTr`.
- No Lomiri API outside `qml/platform/ut/`.
- The reader of a draft never shows Reply, Reply all or Forward.

**Tests (QML):**
- ReplyBar with `isDraft` has `edit` and no `reply` or `forward`.
- ReaderMore with `isDraft` has no `moreReplyAll`.
- Tapping a row with `opens: "compose"` calls `startCompose("draft", 0, "", id)`.
- Tapping a server Drafts row emits `messageRequested`.
- The chip is visible only when `notUploaded`.
- The selection bar in a Drafts list has no `act-archive`, and with `selectionHasLocal` has only `act-delete`.

---

### T7: End to end, phone, docs

**Tier:** leaf (`crates/envio-e2e/`, docs). **Testing:** `cargo make e2e-it` (Stalwart), phone screenshots, design review.

**Files:** new `crates/envio-e2e/tests/drafts_reopen.rs`; spec `docs/superpowers/specs/2026-11-07-envio-design.md` (the Drafts paragraph at line ~121).

**Tests (e2e, Stalwart):**
- **Device A to device B:**
  1. Device A writes and closes a draft, and it uploads.
  2. Device B is a second data dir on the same account. It syncs, sees one Drafts row and Edits it, which imports the draft.
  3. Device B changes the body and closes it, and it uploads.
  4. The server's Drafts folder holds exactly one copy, with the new text and a new Message-ID.
- **Server refusal:** make the Drafts folder read-only, or use Stalwart's quota if `up.sh` can set it. Closing a compose then gives a pseudo-row with `local_draft`. Once the folder is writable again, the next save uploads, and the list shows the server row only.
- **Delete through #284:** delete a Drafts row from the reader. Then Edit cannot bring the draft back, because the file is gone.

**Phone** (ask before taking the phone; it is shared). Use `ute2e shot` for the Drafts list with a Not uploaded row, the reader of a draft and the Edit compose. A separate agent then reviews the shots against `carbon-draft-edit.html` and flags anything that differs.

**Spec:** add one sentence. A Drafts-folder message opens with Edit, and local drafts without a server copy show in the Drafts list as "Not uploaded". Close #266 with the PR that lands T6, because the chip is its notice.

## Open questions

1. **An account with no Drafts-role folder** (`done.no_folder`) has no Drafts list, so its local drafts still show nowhere. The choices are a local-only "Drafts" drawer entry for such accounts, or creating a Drafts folder on the server. Neither is in the mockup. The suggestion is a follow-up issue, not this plan.
2. **An uploaded copy deleted on another device:** under Decision 2 the local draft then shows as Not uploaded, and the planner treats it as current, so it is not re-uploaded until it changes. The alternative is to treat the copy's disappearance as a delete (#284 across devices) and remove the local file. Showing it is the safe default chosen here. Confirm, or file it as its own issue.
3. **Undo for deleting a pseudo-row (T5)** costs a new `Offered` variant. If that is too much for 0.3.0, the fallback is a confirmation dialog ("Delete this draft? It is not on the server."), which needs a mockup first. A third option is to drop selection of pseudo-rows for now and delete through Compose's Discard.
