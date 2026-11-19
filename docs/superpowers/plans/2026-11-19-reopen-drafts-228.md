# Envio Plan: reopen saved drafts (#228)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A saved draft can be reopened in Compose. A message in a Drafts-role folder opens in the reader with one **Edit** button instead of Reply, Reply all and Forward. Edit fills Compose from the local draft file, found by Message-ID, or else from the server message. Every local draft with no current server copy also shows in the Drafts list as a row with a **Not uploaded** chip (this covers #266's notice). Tapping that row opens Compose at once. A draft never shows twice. Saving a plain-text draft replaces its server copy. An HTML draft, or one with an unusable Message-ID, is edited as a copy and the original stays (Decision 4).

**Design:** `docs/design/carbon-draft-edit.html` (the notes under each frame are part of the spec). The `docs/10-design.md` row is "Reopen a draft". It is Carbon only, and the chip uses tint A, so there is no new colour role.

**Depends on #284** (parallel PR) **and respects #299** (a deleted draft's file stays until the journal replays the action; see Decision 2): deleting a Drafts-folder message also deletes the local draft file and its attachment copies. This plan does not touch that path. T6 (the QML that shows Edit) must not merge before #284, or Edit could bring back a draft the user deleted. T1 and T2 can land first because they show nothing new. T3 waits for #299, whose rule it reuses (Decision 2), and T4 and T5 build on T3.

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
   - **Being deleted hides it (interaction with #299).** A Delete or Move removes the cache row at once (`envio-sync/src/driver/actions.rs:72-74`). Under #299 the local file stays until the journal replays the action after the Undo window, and offline it stays longer. Without a rule, the draft would come back as a "Not uploaded" pseudo-row during that time, and tapping and saving it would upload it again.
   - **Rule: hide exactly what #299 will forget.** #299's `drafts_to_forget` (`envio-sync/src/plan/drafts.rs`) forgets a draft for an action that is a `Delete`, or a `Move` whose destination has the Trash role, whose `target.folder` has the Drafts role, and whose `target.message_id` equals the draft's `server.message_id`, in the same account, when the draft is current (`uploaded_ms == updated_at_ms`). It skips a draft whose compose is open. T3 extracts that per-action test from `drafts_to_forget` into one public function, and both callers use it, so the two rules cannot drift (see T3). A local draft gets no pseudo-row while `Journal::pending()` holds an action that passes that test and matches the draft the same way: same account, `server.message_id` equal after `trim()`, and `uploaded_ms == updated_at_ms`. The open-compose case is not copied: a draft with an open compose is kept by #299 and uploaded again when it closes, so it is not leaving.
   - **Edits after the copy was deleted still show.** A draft with a save the server copy lacks (`updated_at_ms > uploaded_ms`) is kept by #299 and uploaded again. Hiding it would hide unsent work, so it gets its pseudo-row during the window. On Undo the server row comes back, the draft matches it, and the pseudo-row goes.
   - **No other way out of Drafts.** Any other move out of a Drafts-role folder (Archive, or Move to a non-Trash folder) would leave the file behind after the replay. The draft would then show as "Not uploaded" for good although it sits on the server, and an edit would upload it into Drafts again. So those actions are not offered for Drafts-role messages (Decision 12), and core refuses them (T5). Delete, and a Move to Trash, are the only ways out, and #299 handles both.
   - **Why "hide" rather than "count as a server copy":** the user asked for the draft to go, so the honest state is "leaving", and neither row should show. On Undo the action is withdrawn, `Restore::Row` brings the server row back, and the draft shows as the server row again. Once the action replays, #299 removes the file. If the replay is skipped (the journal's `skipped()`), the action is no longer pending and the next sync restores the server row, so the draft never vanishes for good. Counting it as a server copy would have the same effect on screen, but it would put a false fact into a predicate that T1 and the planner also read.
   - **Remote deletes still show it.** When another device deletes the uploaded copy, nothing is pending here, so the draft shows as "Not uploaded" and is not re-uploaded until it changes. This follows #299's rule that a remote event never destroys a local draft. (This was open question 2, decided on the owner's behalf; they can override it.) A T3 test covers it: a draft whose `message_id` is neither on the server nor in `leaving` gives a row.
3. **A local file is newer than the server copy.** When Edit finds a local draft by Message-ID, Compose opens the local file even if the reader showed older server text (an edit that has not uploaded yet). The server message is imported only when no local file matches.
4. **Import marks the draft as current only when nothing can be lost.** There are two kinds of import:
   - **Linked import**, the mockup's case. The server message's Message-ID passes `envio_sync::plan::message_id::search_id`, and the message has no `text/html` part. That is a plain-text draft, which is what Envio itself uploads (`draft_mail::draft_eml` writes text only). The imported draft gets `server.message_id = <that Message-ID>` and `uploaded_ms = updated_at_ms`. Unchanged, it is never uploaded. After the first edit, the existing upload replaces that copy, because `replaced()` returns the old id. So "first save replaces it on the server" needs no new sync code.
   - **Detached import**, for an unusable Message-ID (missing, `<>`, or one `search_id` rejects) or for any message with a `text/html` part. Compose holds only plain text, so replacing the original would drop its formatting, and an unusable id cannot be found to replace. The imported draft gets no `ServerCopy` (`message_id: None`). It uploads as a new draft on the first save and never removes the original. Compose shows the info banner "Editing a copy. The original stays in Drafts." for as long as it is open (mockup frame "Edit a copy in Compose"). The original stays readable and deletable in the reader. The cost is a second draft in the Drafts folder, which is visible and loses nothing.
   - **A detached import remembers its origin.** `ServerCopy` gains `imported_from: Option<String>` (`#[serde(default)]`, so old files load). It holds the origin's usable Message-ID. T1's `draft_for` also matches it, so a second Edit of the same original reopens the copy instead of importing again. The sync planner and `replaced()` ignore the field. The field must survive the copy's upload: `Drafts::mark_uploaded` (`drafts_server.rs:146`) rebuilds `server` from `ServerCopy::default()` today and would reset it to `None`, so T2 makes it keep `imported_from`. Every other write of `server` already keeps it: `save` copies the old file's `server` (`drafts.rs:125`), `salvage_fields` reads the whole `server` object, and `mark_uploading`, `mark_failed` and `mark_tried` edit single fields in place. For an unusable Message-ID there is nothing to match, so each Edit makes a new copy. That case is rare and loses nothing, and a test pins the behaviour.
5. **Import waits for the whole body, as Forward does,** so attachments come along. They are copied into the draft's folder with `Drafts::keep_bytes`. With no connection and no cached whole body, Edit is refused with `text::read::message_unavailable()`, the same as Forward.
6. **Pseudo-row ids are negative.** The worker keeps `LocalIds`, a map from draft id to `i64` starting at -1 and counting down. Ids are stable for the session and never reused. Cached rows have positive SQLite rowids, so the two can't collide, and `ShownList` keys `(date, id)` keep working. The cursor is `{ date: updated_at_ms / 1000, id }`.
7. **Pseudo-rows are merged per page in core** (`list_messages` for a `ListRef::Folder` whose role is Drafts). A page shows the cached rows plus the local drafts whose key lies in the page's range (`(after, last cached key]`, or everything older when `end`), sorted newest first. `end` still comes from the cached page alone. The unified inbox, search and other folders never get pseudo-rows.
8. **Taps are routed by Rust.** A row has `opens: "reader" | "compose"`. Pseudo-rows open Compose (`startCompose("draft", 0, "", draftId)`) and every other row opens the reader, as the mockup says ("Tap opens it in Compose at once. Any other row opens the reader."). The feasibility note's "tapping a Drafts row opens Compose" is read as the pseudo-rows. A server row goes through the reader's Edit.
9. **Drafts rows show To instead of From** (server rows and pseudo-rows). Core fills `from` with the translated "To: %s", taking the recipients' names or addresses joined with ", " (or "(no recipients)"), and fills `from_address` with the first recipient so the stamp tint works. The stamp, preview and date stay where they are.
10. **List refresh:** a new `Event::LocalDraftsChanged { account }`. It is sent when a draft file is created, closed (saved or discarded), deleted (Send, the #284 path, T5's Delete), or changes upload state (after `sync_drafts`). It is not sent on every throttled keystroke save, so the list does not reload once a second behind Compose. `envio-ui` turns it into a first-page refresh when the current list is that account's Drafts folder.
11. **Selection:** pseudo-rows can be selected (mockup: "Long-press selects"). In a Drafts list the selection bar never offers Archive or Move (Decision 12). While the selection holds a pseudo-row it offers only Delete. Delete of pseudo-rows goes through `Act { Delete }` with the usual Undo: core holds the draft ids for the Undo window and calls `Drafts::delete` when the window closes. If the app is killed during the window, the draft survives, which is the safe direction. The Undo stays (this was open question 3, decided on the owner's behalf). A confirmation dialog would need a new mockup, and no other Delete in the app asks first.
12. **Reader of a draft** (`MessageView.is_draft`, true when the message's folder role is Drafts): the bottom bar is one filled Edit button with a pencil, and Reply all leaves More. Archive is hidden, and Move leaves More. Delete, Flag and Add sender stay. In a Drafts-role list the selection bar hides Archive and Move too. Reason: the mockup removes the reply actions and Archive, and Move goes because a draft moved anywhere but Trash would leave its local file behind (Decision 2). Delete is the only way out. Search results can still hold a Drafts message under a bar that offers Move, so core refuses Archive and a non-Trash Move from a Drafts-role folder as well (T5).
13. **One compose per draft.** Edit or a pseudo-row tap for a draft that an open compose already holds is refused with a new text, "This draft is already open." Today `DraftBook::adopt` would let two composes write over each other.

## Tasks

Order: T1 → T2 → T3 → T4 → T5 → T6 → T7. T1 to T3 are core (T3 adds one store query and starts after #299 merges). T4 is `envio-ui` plus `qt/`. T5 is core plus ui. T6 is QML and waits for #284. T7 is e2e, phone and docs.

---

### T1: Edit a cached Drafts message that has a local file (core, qt kind)

**Tier:** trunk (`envio-core`, `envio-store`, `qt/`). **Testing:** unit plus core integration (`crates/envio-core/tests/`, existing fake-server support), plus `cargo make app-docker` because `qt/` changes.

**Files:**
- New `crates/envio-core/src/drafts/lookup.rs` (pure).
- `crates/envio-core/src/api/command.rs`: `StartFrom::Edit`.
- New `crates/envio-core/src/worker/edit.rs`: `start_edit`.
- `worker/compose.rs`: dispatch only.
- `drafts/book.rs`: `holder`.
- `api/view.rs` and `worker/read.rs:144`: `is_draft`.
- `text/compose.rs`: `draft_already_open`.
- `crates/envio-store/src/drafts_server.rs`: `ServerCopy.imported_from` (Decision 4). Serde default, no migration, and old files still load (test it).
- `qt/src/convert.rs`: kind `"edit"`.
- `qt/src/json.rs::message_view_value`: emit `isDraft`, which is how the reader view reaches QML.
- `MessageView` struct literals that gain `is_draft: false`: `qt/src/json.rs` tests (lines 327, 353 and 390) and `crates/envio-ui/tests/mirror.rs` (lines 110 and 170). The compiler lists any more.

**Produces:**
```rust
// api/command.rs
pub enum StartFrom { …, /// A cached message in a Drafts-role folder.
    Edit { id: MessageRowId } }
// api/view.rs
pub struct MessageView { …, /// The message's folder has the Drafts role.
    pub is_draft: bool }
// drafts/lookup.rs
/// The local draft whose server copy is `message_id` (its `message_id`,
/// `pending_id` or `imported_from`, trimmed), in `account`. Pure over a listing.
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
- `draft_for` matches on `message_id`, on `pending_id` and on `imported_from`, ignores surrounding whitespace, misses another account, misses `<>`.
- Store: a draft file without `imported_from` loads with `None`, and the field round-trips.
- Integration: a cached Drafts message plus a local file with the same `server.message_id` gives `ComposeReady` with the file's text.
- Integration: a non-Drafts message gives `ComposeRefused`.
- Integration: a second Edit while the first compose is open gives `ComposeRefused` with "This draft is already open."
- `is_draft` is true for a Drafts-role folder and false for the Inbox.
- qt: `start_from("edit", 7, …)` gives `StartFrom::Edit { id: 7 }`, and `message_view_value` carries `isDraft`.

---

### T2: Import a server-only draft (core)

**Tier:** trunk (`envio-core`, `envio-store`, `envio-ui`, `qt/`). **Testing:** unit, store integration and core integration, `cargo make ci`, plus `cargo make app-docker` because `qt/` changes.

**Files:**
- New `crates/envio-core/src/drafts/import.rs` (pure).
- `worker/edit.rs`: wait for the body and finish.
- `worker/start.rs`: `compose_body_fetched` routes `Edit` waiters to `edit.rs`. Touch nothing else there.
- `crates/envio-store/src/drafts_server.rs`:
  - `mark_uploaded` keeps `imported_from`. It still clears every other field, as it does today.
  - New `create_imported`, because `create` cannot store a `ServerCopy`: it calls `save`, and `save` replaces `draft.server` with the old file's (`drafts.rs:125`), which for a new id is `ServerCopy::default()` (the `Err(_)` arm). Without it a linked import would lose `server.message_id` and upload as a second copy, and a detached one would lose `imported_from`.
  - No other `ServerCopy` rebuild exists (`git grep -n "ServerCopy {"`: only `mark_uploaded`, the struct and `envio-sync` test literals, which use `..ServerCopy::default()`; `drafts.rs:125` copies the old value; `salvage_server`'s default is for a file that names no copy).
- `ComposeReady` gains `note` (step 4). Every construct and destructure site:
  - `crates/envio-core/src/api/event.rs:234`: the variant.
  - `crates/envio-core/src/worker/compose.rs:69`: the one `emit`. Its helper `compose_ready` takes the note, and every existing caller passes `String::new()`.
  - `crates/envio-core/src/text/compose.rs`: `editing_a_copy`.
  - `crates/envio-ui/src/signals.rs:93`: `Signal::ComposeReady` gains `note: String`.
  - `crates/envio-ui/src/convert.rs:231-238`: maps `note` across. It names every field without `..`, so it fails to build until it does.
  - `crates/envio-core/tests/compose.rs:224`: the pattern names every field without `..`. Add `note` (the test asserts it is empty).
  - `qt/src/events.rs:171-177`: passes `&q(&note)` to `compose_ready`.
  - `qt/src/core_object.rs:113-114`: the `composeReady` signal gains a fourth parameter, `note: &QString`.
  - Patterns with `..` need no change: `crates/envio-core/tests/compose.rs:313`, `tests/drafts.rs:105` and `:340`, `tests/outbox.rs:214` and `:263`, `crates/envio-e2e/tests/core_stalwart.rs:236`, `drafts_stalwart.rs:59`, `:111` and `:134`, and `support/ui_help.rs:73`.
  - QML: the existing handlers `ComposePage.qml:330` (`compose, draft, quote`) and `AppShell.qml:381` (`compose`) ignore an extra argument, so T2 changes no QML. T6 adds `note` to the ComposePage handler and to `tests/qml/MockCore.qml:61`.

**Produces:**
```rust
// drafts/import.rs
/// A local draft built from a server draft's raw bytes. Pure.
/// Linked (Decision 4): `server.message_id = Some(id)`, `uploaded_ms = updated_at_ms = now_ms`.
/// Detached: `server = ServerCopy { imported_from: usable id or None, ..Default }`.
pub(crate) fn draft_from_raw(raw: &[u8], account: &str, message_id: Option<&str>, now_ms: i64) -> Imported;
pub(crate) struct Imported { pub draft: Draft, pub attachments: Vec<(String, Vec<u8>)>, pub linked: bool }
/// Linked only for a usable Message-ID (`search_id`) and no text/html part anywhere in the message.
pub(crate) fn can_link(parsed: &envio_mime::Parsed, message_id: Option<&str>) -> bool;
// envio-store drafts_server.rs
/// Like `create`, but writes `draft.server` as given instead of the
/// "never uploaded" default. For a draft imported from the server.
pub fn create_imported(&self, draft: &Draft) -> Result<String>;
// mark_uploaded, changed body:
//   draft.server = ServerCopy { message_id: Some(..), pending_id: None, uploaded_ms,
//       imported_from: draft.server.imported_from.take(), ..ServerCopy::default() };
// api/event.rs
ComposeReady { compose: ComposeId, draft: DraftInput, quoted: String,
               /// A note Compose shows while it is open; empty for none.
               note: String },
```
- Recipients come from To, Cc and Bcc, shown as `start::displayed` does.
- The subject is taken unchanged. In-Reply-To and References are carried over.
- The body is the text/plain part, else the HTML turned into text through envio-mime's existing helper. Pick the one Reply uses for quoting. Do not write a new converter.
- Flow:
  1. `Drafts::create_imported` with the draft and its `server`.
  2. `keep_bytes` for each attachment, then `save` with the paths. `save` keeps the `server` that step 1 wrote. The second save keeps `updated_at_ms = now_ms`, so a linked import still has `uploaded_ms == updated_at_ms` and is not uploaded.
  3. `adopt`, then `compose_ready`.
  4. For a detached import, `ComposeReady` carries `note: text::compose::editing_a_copy()` ("Editing a copy. The original stays in Drafts."). `Event::ComposeReady` gains `note: String` (empty for none), which is not a transient `Notice`. The qt glue passes it through with the compose's other fields (sites listed under Files).

  If the body is missing or partial, wait for the whole body exactly as Forward does (`live.wants.bodies` plus `whole`, and `composing` with `StartFrom::Edit`).

**Invariants:**
- An unchanged linked import is never uploaded. The planner sees it as current (Decision 4).
- The first saved change to a linked import uploads with `replaced() == Some(old)`, so the server copy is replaced, never duplicated.
- A detached import never causes a server message to be removed: `replaced()` and `cut_copy()` see no id, and `imported_from` is never read by sync.
- `imported_from` survives every upload of the copy, so an original never gets more than one local copy (for a usable Message-ID).
- An HTML original is never overwritten by plain text.
- If an import fails halfway (a write error), `Drafts::delete` removes the new file and Compose is refused. No half-filled draft is left behind.

**Tests:**
- Unit: `draft_from_raw` on a multipart with an attachment gives the recipients, subject, body, `in_reply_to` and one attachment, and `server.message_id` equals the input.
- Unit: an HTML-only draft gives the text body and `linked == false`, with `server.message_id == None` and `imported_from == Some(id)`.
- Unit: a multipart/alternative draft (text plus HTML) is detached too.
- Unit: a plain-text draft whose Message-ID is missing, `<>` or `"  "` is detached, with `imported_from == None`.
- Integration: Edit on a cached HTML draft, then a change and a close, uploads a new copy. The server keeps both copies, the original's bytes are unchanged, and `ComposeReady.note` held the copy text. A linked import has an empty `note`.
- Store: `mark_uploaded` on a draft with `imported_from: Some(x)` keeps `Some(x)` and still sets `message_id`, `uploaded_ms`, and clears `pending_id` and the failure fields.
- Store: `create_imported` writes the given `server` (read back with `load`), and a following `save` keeps it.
- Integration, in this order: Edit the cached HTML original O, change the body and close. Run `sync_drafts`, so the copy uploads as C' and `mark_uploaded` runs. Edit O again. It reopens the same local draft (`ComposeReady.draft` has the changed body), no new draft file exists, and after another `sync_drafts` the server holds exactly O and one copy.
- Integration: a plain-text draft with no Message-ID, edited twice, gives two local drafts. This pins the documented behaviour, and nothing is deleted.
- Integration: Edit on a cached Drafts message with no local file creates one draft file with `server.message_id` set and gives `ComposeReady`. A second Edit on the same message (after closing) finds that file through T1 instead of importing again.
- Integration: a body not yet cached gives `ComposeReady` after the fetch.
- Sync planner, unit in `envio-sync` if not already covered: a draft with `uploaded_ms == updated_at_ms` and `message_id` set plans no upload. After `updated_at_ms` grows it plans an upload that names the old id as replaced.

---

### T3: Pseudo-rows for local drafts in the Drafts list (core, one store query)

**Tier:** trunk (`envio-core`, `envio-store`, `envio-sync`). **Testing:** unit plus store integration plus core integration. This is the largest task. Keep the merge pure and the I/O in `read.rs` thin. **Starts after #299 merges**, because it reuses #299's rule.

**Files:**
- `crates/envio-store/src/messages.rs`: `message_ids_in`, which reads the indexed column and adds no migration.
- `crates/envio-sync/src/plan/drafts.rs` (after #299): extract the per-action test of `drafts_to_forget` into `pub fn leaves_drafts(action: &Action, folders: &[Folder]) -> Option<&str>`. It gives `target.message_id` when the action is a `Delete`, or a `Move` whose `to` has the Trash role, and `target.folder` has the Drafts role. Otherwise it gives `None`. `drafts_to_forget` calls it and keeps its draft matching, and its existing tests stay green unchanged. This is a refactor of #299's code with no change in behaviour.
- Journal read for Decision 2's hide rule: new pure `leaving(pending: impl Iterator<Item = &Action>, account: &str, folders: &[Folder]) -> HashSet<String>` in `local_rows.rs`. It returns the trimmed message ids that `leaves_drafts` gives for the pending actions of `account`. `folders` are the account's folders, as `drafts_to_forget` takes them. Core reads `self.store.journal.pending()` and `self.store.db.folders(account)`. The store's journal API does not change.
- `crates/envio-core/src/api/view.rs`: `MessageRow.local_draft`.
- `crates/envio-core/src/api/event.rs`: `LocalDraftsChanged`.
- New `crates/envio-core/src/worker/local_rows.rs`: the pure merge, `LocalIds`, the To line.
- `worker/read.rs`: `list_messages` calls the merge for a Drafts-role folder.
- `worker/rows.rs`: the To line for Drafts-role server rows. Make `subject` `pub(super)` so `local_rows.rs` can share the placeholder. It is private today.
- `MessageRow` struct literals that gain `local_draft: None`: `worker/rows.rs::row_of` and its test helper, and `crates/envio-ui/tests/support/mod.rs::msg_in`. The compiler lists any more.
- `text/read.rs` or a new `text/list.rs`: "To: %s", "(no recipients)", "Not uploaded".
- `drafts/keeper.rs` and `drafts/book.rs`: send `LocalDraftsChanged` on create and close.
- `worker/run/mail.rs`: send it after `sync_drafts` when anything was uploaded, refused, failed or deleted.
- `worker/compose.rs` and `worker/outbox.rs`: send it where a draft is deleted.

**Produces:**
```rust
// envio-store messages.rs
/// The Message-IDs of the live messages in `folder`, each `trim()`med; NULL and empty skipped.
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
/// `on_server` from `message_ids_in`, `leaving` from the journal; both trimmed.
/// A draft is hidden as leaving only when its trimmed `server.message_id` is
/// in `leaving` and `uploaded_ms == updated_at_ms` (#299 keeps the others).
pub(super) fn local_rows(drafts: &[(String, Draft)], account: &str, folder: FolderId,
                         (on_server, leaving): (&HashSet<String>, &HashSet<String>),
                         ids: &mut LocalIds) -> Vec<MessageRow>;
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
- A draft that #299 will forget once a pending action replays (a Delete, or a Move to Trash, of its current server copy) shows no row at all: not during the Undo window, and not offline. A draft #299 would keep (newer local edits) keeps its row.
- `leaving` and `drafts_to_forget` agree on every action, because both go through `leaves_drafts`.
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
  - a current draft (`uploaded_ms == updated_at_ms`) whose `message_id` is not on the server but is in `leaving` gives no row;
  - a draft with newer edits (`updated_at_ms > uploaded_ms`) whose `message_id` is in `leaving` still gives a row;
  - a draft whose only match in `leaving` would be its `pending_id` gives a row (#299 matches `message_id` only);
  - Message-IDs with stray whitespace still match;
  - a never-uploaded draft gives a row;
  - a refused draft (`failures >= GIVE_UP`) gives a row;
  - another account's draft gives no row;
  - `LocalIds` keeps a draft's id across calls and never reuses one.
- `leaves_drafts` (envio-sync unit): `Delete` from the Drafts-role folder gives the id. `Move` to the Trash-role folder gives it. `Move` to an Archive-role folder, `Move` to a folder with no role, `SetFlags`, an action from a non-Drafts folder, and an action with no `target.message_id` give `None`.
- `leaving`:
  - takes a pending `Delete`, and a pending `Move` whose `to` has the Trash role, from the Drafts-role folder;
  - ignores a `Move` to the Archive-role folder or to a folder with no role (Decision 2: #299 does not forget these. Once T5 and T6 land they cannot be recorded from a Drafts-role folder, but older journal entries may hold one, and `leaving` must agree with #299 for them);
  - ignores `SetFlags`, actions that are no longer pending, actions from other folders and actions of other accounts.
- `to_line`: names, addresses, and an empty list.
- Store integration: `message_ids_in` skips other folders and rows with a NULL Message-ID.
- Core integration:
  - `ListMessages` on a Drafts folder with one cached server draft and two local drafts (one matching the server row) gives two rows, and one of them has `local_draft`;
  - the Inbox list has no pseudo-rows;
  - closing a compose sends `LocalDraftsChanged`;
  - **#299 window:** a cached Drafts server row plus its uploaded local file. `Act { Delete }` on the server row, then `ListMessages` inside the Undo window gives no row for that draft, neither server nor pseudo. The same holds with the account offline after the window. After `Undo`, the server row is back and there is still no pseudo-row.
  - the same window check with a `Move` to the Trash-role folder instead of `Delete`.

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

### T5: Delete selected pseudo-rows with Undo, and no other way out of Drafts (core, ui)

**Tier:** trunk (`envio-core`, `envio-ui`). **Testing:** unit plus core integration.

**Files:**
- `crates/envio-core/src/worker/act.rs`: split negative ids off before `action_kind`. `action_kind` gains the source folder's role: `action_kind(action, from: FolderRole, folders)`. From a Drafts-role folder, `Archive` gives `None`, and `Move { to }` gives `None` unless `to` has the Trash role (Decision 2). `act_one` passes `folder.role`, and refuses that `None` with the new `text::undo::drafts_delete_only()` ("A draft can only be deleted."). The existing `no_archive` text stays for a missing Archive folder elsewhere.
- `text/undo.rs`: `drafts_delete_only`.
- New `worker/act_local.rs`: hold the ids, delete them on expiry, put them back on Undo.
- `undo.rs` or the `Offered` enum: a new variant `LocalDrafts { ids: Vec<String> }`.

**Behaviour:**
- `Act { ids, Delete }` with negative ids resolves them through `LocalIds`. Core hides them from the next list reads at once, through a "held" set in `local_rows`. It sends `LocalDraftsChanged` and one `UndoOffer`, together with any server ids in the same Act.
- On expiry, `Drafts::delete` runs on each id. That writes the server-delete note if the draft ever had a server copy, so a stale copy is removed too.
- On Undo, the held set is cleared and `LocalDraftsChanged` is sent.
- Every other action on negative ids skips those ids. The UI does not offer such actions (Decision 11).
- `Archive`, or a `Move` to a non-Trash folder, of a message in a Drafts-role folder records nothing and shows the refusal notice. Other messages in the same Act (from search results, say) go ahead as today. `Delete`, flag and read actions are unchanged.

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
- `action_kind` unit: from a Drafts-role folder, `Archive` and `Move` to an Archive-role or no-role folder give `None`, and `Move` to the Trash-role folder and `Delete` give their kinds. From the Inbox nothing changes (the existing tests pass `FolderRole::Inbox`).
- Integration: `Act { Move }` to a non-Trash folder on a Drafts message leaves the journal without a new pending action, keeps the cache row, and sends the refusal notice. In the same Act an Inbox message moves.
- envio-ui: `selection_has_local` is true while a pseudo-row is selected.

---

### T6: QML (after #284 merges)

**Tier:** leaf (`qml/pages/**`). If anything in `qml/Main.qml` or `Theme.qml` changes, that file is trunk. None is expected. **Testing:** `cargo make qml-test` plus `cargo make app-docker`, then `cargo make pot`.

**Files:**
- `qml/pages/reader/ReplyBar.qml`: when `isDraft`, one filled `AppButton` with `objectName: "edit"`, a pencil icon and `qsTr("Edit")`.
- `ReaderMore.qml`: hide `moreReplyAll` and `moreMove` when `isDraft` (Decision 12). The items are chosen by `pick`, so filter the model rather than renumbering the `pick` cases. The reader's top bar has no Move button (`act-archive`, `act-delete`, `act-more` only), so More is the only reader entry to Move.
- `ReaderPage.qml`: hide `act-archive` when `isDraft`. `edit()` calls `core.startCompose("edit", messageId, view.account, "")`, and then `composeOpened(id)` as `compose()` does.
- `qml/pages/inbox/MessageRow.qml`: a `Chip` with tint A and `qsTr("Not uploaded")` when `model.notUploaded`. Its ink comes from the existing tint A theme values.
- `MessageList.qml:96`: when not selecting, `model.opens === "compose"` emits a new `draftRequested(model.draftId)`, and every other row emits `messageRequested`.
- `InboxPage.qml`: `onDraftRequested` calls `core.startCompose("draft", 0, "", draftId)` and opens Compose like the "new" path in `AppShell.qml:92`. Move that wiring to AppShell if InboxPage has no compose hook.
- `qml/pages/compose/ComposePage.qml`: a second `Banner` (`objectName: "composeNote"`, `kind: "info"`) under the top bar and above `composeError`, showing the compose's `note` while it is non-empty. It follows the mockup frame **"Edit a copy in Compose"** in `docs/design/carbon-draft-edit.html`: the info icon, the images-banner style and no button. The text comes from Rust (gettext), so QML adds no string.
- `SelectionBar.qml`: hide `act-archive` and `act-move` when `core.listIsDrafts` (Decision 12). Also hide `act-more` when `core.selectionHasLocal`, so a selection with a pseudo-row offers only `act-delete`.
- `ComposePage.qml:330`: the `onComposeReady` handler takes the fourth argument `note` (T2 added it to the signal), and `tests/qml/MockCore.qml:61` declares `signal composeReady(real compose, var draft, string quoted, string note)`.

**Invariants:**
- No swipe handlers.
- Every new string uses `qsTr`.
- No Lomiri API outside `qml/platform/ut/`.
- The reader of a draft never shows Reply, Reply all, Forward, Archive or Move.
- A Drafts-role list's selection bar never shows Archive or Move.

**Tests (QML):**
- ReplyBar with `isDraft` has `edit` and no `reply` or `forward`.
- ReaderMore with `isDraft` has no `moreReplyAll` and no `moreMove`. `moreFlag` (or `moreUnflag`) and `moreContact` still pick their actions. Without `isDraft` it still has `moreMove`.
- ReaderPage with `isDraft` has no visible `act-archive`, and `act-delete` is visible.
- Tapping a row with `opens: "compose"` calls `startCompose("draft", 0, "", id)`.
- Tapping a server Drafts row emits `messageRequested`.
- The chip is visible only when `notUploaded`.
- ComposePage with a non-empty `note` shows `composeNote` with that text, and with an empty note hides it.
- The selection bar in a Drafts list has no `act-archive` and no `act-move`, and keeps `act-delete` and `act-more`. With `selectionHasLocal` it has only `act-delete`. In the Inbox it still has `act-archive` and `act-move`.

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
- **#299 Undo window:** delete an uploaded draft's Drafts row from the reader, with its local file still present.
  - Inside the Undo window, the Drafts list shows no row for it, neither server nor "Not uploaded".
  - With the account taken offline before the replay, still no row.
  - After the replay, the server holds no copy and the local file is gone.
  - In a second run, Undo instead brings the server row back. The server holds exactly one copy and no pseudo-row appears.
- **HTML original:** APPEND an HTML draft to Drafts directly, Edit it, change it and close.
  - The server then holds the untouched original plus one new plain-text copy.
  - A second Edit of the original reopens the same local copy.

**Phone** (ask before taking the phone; it is shared). Use `ute2e shot` for the Drafts list with a Not uploaded row, the reader of a draft, the Edit compose and the "Edit a copy" compose with its banner. A separate agent then reviews the shots against `carbon-draft-edit.html` and flags anything that differs.

**Spec:** add one sentence. A Drafts-folder message opens with Edit, and local drafts without a server copy show in the Drafts list as "Not uploaded". Close #266 with the PR that lands T6, because the chip is its notice.

## Out of scope

- **Accounts with no Drafts-role folder** (`done.no_folder`) have no Drafts list, so their local drafts still show nowhere. This is a follow-up issue (a local-only Drafts entry in the drawer, or creating the folder on the server), and it needs its own mockup. It was decided on the owner's behalf; they can override it.

Former open questions 2 and 3 are now decisions: remote deletes in Decision 2, Undo in Decision 11.
