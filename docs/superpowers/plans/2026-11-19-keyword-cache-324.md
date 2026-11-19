# Envio Plan: cache IMAP keywords so Edit never replaces a draft that carries them (#324)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Edit on a server-only draft links the local draft to the original (so the first save replaces it) only when the cache knows the original has no IMAP keywords. A message with `$Forwarded`, `$Label1`, a Gmail label or any other keyword, or a message whose keywords the cache does not know yet, is imported detached and the server original stays.

**Architecture:** `envio-net` already parses keywords (`MessageFlags.keywords`, `imap/query.rs:27`; `message_flags`, `imap/ops.rs:315`, maps every `Custom` flag). They are dropped at `envio_sync::convert::store_flags` ("Keywords are not cached"). New: one nullable column `messages.keywords` (migration 0005), written next to the flag bits by the sync driver, compared on every flag check, read by Edit in `envio-core`. No `Flags` change (it is a `u8` bitset serialised as a number, `messages.rs:20-27`), no `NewMessage` or `Message` change (about 32 `NewMessage` literals would all break), no cxx-qt or QML change.

## Global Constraints

- Dependencies point one way: ui → core → sync → net, store, mime. The column and its accessors live in `envio-store`, the conversion and the planner in `envio-sync`, the decision in `envio-core`. `envio-net` is not touched.
- Migrations are numbered and never edited after merge. **0005** is the next number: origin/master has 0001 to 0004, and neither open PR (#364, #350) adds a migration. If another migration lands first, renumber at rebase, before merge.
- **Unknown is never "none".** `NULL` means unknown, and unknown counts as lossy. Only a stored empty list (`''`) means "no keywords".
- Files stay around 300 lines. New code goes in new modules (`messages.rs` is already about 440 lines with tests, so the accessors go in a new `keywords.rs`).
- `cargo make ci` passes at every commit. Tests use `example.org` only.
- No user-facing text, so no `cargo make pot`.

## Decisions

Taken on the owner's behalf; the owner may override any of them.

1. **Storage:** `keywords TEXT` (nullable, no default) holding the keywords sorted, deduplicated and joined by one space. IMAP keywords are atoms (no space, RFC 3501 `flag-keyword`), so the join is unambiguous. `NULL` unknown, `''` none. Exact case (IMAP compares keywords case-insensitively, but a case-only difference only costs one harmless rewrite).
2. **No `NewMessage` field.** `upsert_message` neither writes nor touches the column (its `ON CONFLICT DO UPDATE` list at `messages.rs:195-199` stays as it is), so a fresh row is `NULL` until the sync step calls `set_keywords` in the same transaction. Fewer broken literals, and an upsert from Undo (`driver/actions.rs:147`) or a body download (`driver/body.rs:113`) cannot wipe known keywords.
3. **Undo of a delete or move** re-creates the row through `upsert_message`, so its keywords are unknown (`NULL`) until the next flag check. That is the safe side. Carrying them in `Restore::Row` is out of scope.
4. **Backfill by one full flag check.** The planner forces `FlagCheck::All` for a folder that has any `NULL`-keyword row, even when CONDSTORE says nothing changed. After that pass every cached row the server still lists is known. Rows the server no longer lists are removed by the existing plan. A row the server never answers for stays `NULL` and so lossy.
5. **Any keyword is lossy**, including `$MDNSent`, `$Junk`, `NonJunk`. Strict is the safe side; relaxing to an allowlist can follow if real drafts turn out to carry harmless ones.
6. **Keyword-only changes do not count as `flags_changed`**: they change nothing the list shows, so they must not fire the "N messages changed" path in `envio-core/src/worker/run/sync.rs:92`.
7. **Edit checks the server live before it links.** The cache can be stale (another client added `$Forwarded` after our last sync), and the issue says "never". So the cache is only a pre-filter: unknown or non-empty cached keywords detach with no network; a cache that says "none" is confirmed by one `fetch_flags(&[uid], None)` (system flags and keywords of that UID) before the draft may be linked. A failed fetch, a paused or offline account, or a message the server no longer lists detaches. The fresh answer also refreshes the cache (same `apply_flags` as `replay.rs:221`).

## Tasks

Order: T1 → T2 → T3 → T4 → T5. T1 to T4 are trunk. T2 and T3 may be one PR if small. T4 needs T1 (the cache pre-filter) and T2 (`apply_flags` keeps the cache fresh after the live check), and ships after T3 so the cache is filled (otherwise every Edit would detach until the backfill ran). T4 may be split in two PRs: the sync function and `import.rs` first, the worker step and its tests second.

**Open PRs touching the same code (do not depend on them; rebase over them when they merge):**
- **#364** (#266 T2, draft): `envio-sync/src/plan/drafts.rs`, `driver/drafts.rs`, `plan/refusal.rs`, `envio-core/src/drafts/import.rs` (6 lines, a `ServerCopy` literal) and `envio-core/tests/edit.rs`. No overlap with T1 to T3. **T4 touches `import.rs` and `tests/edit.rs`: merge T4 after #364 or rebase onto it.**
- **#350** (#285 T7, draft): `envio-core/src/worker/{sender_check,sender_lookup,shown,store,testing}.rs`, `tests/support/mod.rs`. Overlap only in shared test helpers (`worker/testing.rs`, `tests/support/mod.rs`): if T4 needs a helper change there, rebase over #350. `shown.rs` and `edit.rs` are different files.

---

### T1: Store: column and accessors (store)

**Tier:** trunk (`envio-store`, a migration). **Testing:** unit plus `crates/envio-store/tests/`.

**Files:** new `crates/envio-store/migrations/0005_message_keywords.sql` (`ALTER TABLE messages ADD COLUMN keywords TEXT;` with a comment saying NULL is unknown and why, in the style of 0004); `migrate.rs` (new entry at the end of `MIGRATIONS`, and the previous-schema test its module doc demands); new `crates/envio-store/src/keywords.rs` (encode/decode and the accessors below, wired in `lib.rs`); new `crates/envio-store/tests/keywords.rs`.

**Signatures:**
- `Db::set_keywords(&self, folder: FolderId, uid: u32, keywords: &[String]) -> Result<()>` (sorts and dedups; `found` semantics like `set_flags`, so a missing row is an error)
- `Db::keywords_of(&self, folder: FolderId, uid: u32) -> Result<Option<Vec<String>>>` (`None` for unknown and for a missing row; callers treat both as lossy)
- `Db::keywords_by_uid(&self, folder: FolderId) -> Result<Vec<(u32, Option<Vec<String>>)>>`, ascending by UID, like `flags_by_uid` (`lookup.rs:30`)
- `Db::has_unknown_keywords(&self, folder: FolderId, uids: &[u32]) -> Result<bool>` (true if any listed UID has NULL keywords; T3 passes the cached UIDs inside the sync window, never the whole folder)
- `fn encode(&[String]) -> String` and `fn decode(Option<&str>) -> Option<Vec<String>>` (private, pure)

**Invariants:**
- `NULL` decodes to `None`, `''` to `Some(vec![])`, `"$A $B"` to `Some(["$A","$B"])`; encode then decode is the sorted dedup of the input.
- A migrated database's existing rows have `keywords IS NULL`, and their other columns are untouched.
- `upsert_message` on an existing row leaves `keywords` as it was; on a new row it is `NULL`.
- `reset_folder` and the FTS triggers are unaffected (the column is not in `messages_fts`).

**Tests (written first; they fail to compile or assert before the code exists):**
- `migrate.rs`: fill the 0004 schema with rows, migrate, rows survive with `NULL` keywords, `user_version` is 5.
- Round trip through `set_keywords` and `keywords_of`: none (`Some([])`), one, several unsorted with a duplicate.
- `keywords_of` of a never-set row and of a missing row is `None`.
- `upsert_message` over a row with keywords keeps them (fails-first against a naive `excluded.*` upsert).
- `has_unknown_keywords` true for a fresh row among the listed UIDs, false after `set_keywords` on each, a NULL row not listed is ignored, per folder only.
- `keywords_by_uid` order and `None`s.

**Blast radius:** a migration on every user's database. Additive and nullable, so no data rewrite (`ADD COLUMN` without a default is O(1) in SQLite).

---

### T2: Sync: write keywords, compare them on flag checks (sync)

**Tier:** trunk (`envio-sync`). **Testing:** unit (pure planner) plus `crates/envio-sync/tests/` with the fake server.

**Files:** `crates/envio-sync/src/convert.rs` (the "Keywords are not cached" doc line becomes "Keyword bits do not exist: keywords go beside the bits, see `keyword_list`"; new `keyword_list`); `plan/sync.rs` (`FlagChanges`, `plan_flags`); `driver/folders.rs` (`store_summaries` `:257`, `apply_flags` `:314`); `crates/envio-sync/tests/folder_sync.rs` and the fake server in `tests/support/` (a test must be able to set keywords on a message and change them later).

**Signatures:**
- `pub fn keyword_list(flags: &MessageFlags) -> Vec<String>` (pure: sorted, deduplicated clone of `flags.keywords`)
- `FlagChanges` gains `pub keywords: Vec<(u32, Vec<String>)>`: UIDs whose keyword list differs from the cache's, or whose cached keywords are unknown. `set` and `remove` keep their meaning.
- `plan_flags(cached: &[(u32, Flags)], cached_keywords: &[(u32, Option<Vec<String>>)], updates: &[FlagUpdate], pending: &Pending) -> FlagChanges`. Pending changes are flag bits only, so keywords come straight from the server.

**Invariants:**
- `store_flags` is unchanged; a keyword never sets a flag bit.
- A fetched batch stores keywords in the same `db.step` transaction as its rows (`store_summaries`): after the step no row of the batch has `NULL` keywords. A summary without keywords stores `Some([])`, not `NULL`.
- A flag check writes `set_keywords` for every entry of `FlagChanges.keywords` in the same step as the bit changes and the removals.
- A message the server marks `\Deleted` is removed and gets no keyword write.
- `apply_flags` returns `set.len() + remove.len()` only (Decision 6), so `Synced.flags_changed` ignores keyword-only changes.
- UIDs in `updates` that are not cached are ignored, as today.

**Implementer notes:**
- `apply_flags` keeps its signature (`local, id, updates, pending`) and reads `keywords_by_uid` itself, so its other caller, `driver/replay.rs:221` (a one-UID `fetch_flags` after a replayed action), compiles unchanged and now also refreshes that message's keywords. That is wanted (T4 uses the same call); add one replay test that a keyword changed on the server reaches the cache.
- The `plan_flags` unit tests in `plan/sync.rs` (from ~`:375`, helper `update()`, and the 200 000-update test at ~`:402`) pass the new `cached_keywords` argument: `&[]` where keywords are irrelevant. An empty slice means "unknown" for every UID (the lookup misses), so those tests would now also fill `FlagChanges.keywords`: give them `Some(vec![])` entries for their UIDs, or assert only on `set` and `remove`.

**Tests:**
- `keyword_list`: none, unsorted, duplicates. The existing `flags_go_both_ways_without_keywords` and `flags_round_trip_for_every_combination` stay as they are (regression guard that bits ignore keywords).
- `plan_flags`: a keyword-only change appears in `keywords` and not in `set` (fails-first: today it appears nowhere); a change in both appears in both; unchanged keywords give nothing; unknown (`None`) cached keywords give an entry even when the server list is empty; a `\Deleted` update only removes.
- Driver, fake server: the first sync stores keywords for every row; a later server-side keyword change (the CONDSTORE path and the plain `FlagCheck::All` path) reaches the cache with `flags_changed == 0`; a bit change still counts.
- A pending local flag change does not hide a server keyword change.

**Blast radius:** the core sync loop. The only new I/O is one extra UPDATE per changed keyword set, inside the existing transaction.

---

### T3: Sync: backfill rows cached before the migration (sync)

**Tier:** trunk (`envio-sync`). **Testing:** unit (planner) plus `crates/envio-sync/tests/folder_sync.rs`.

**Files:** `plan/sync.rs` (`FolderView`, `flag_check` `:141`, the `view()` test helper at `:228`); the place in `driver/folders.rs` that builds `FolderView`; `envio-store/src/keywords.rs` (T1's `has_unknown_keywords` takes a UID set, see below).

**Why the input is computed in the driver:** `plan_folder` takes a `FolderView` and returns the `FolderPlan` that holds `check`, so the flag to force `All` cannot depend on `plan.check`. `FolderView` already carries both inputs that define the check set: `window` (the UIDs the server lists in the sync window) and `cached_uids`. The driver, which builds the view, intersects them (a cached UID that is also in the window) and asks the store about exactly those.

**Signatures:**
- T1's accessor becomes `Db::has_unknown_keywords(&self, folder: FolderId, uids: &[u32]) -> Result<bool>`: true if any of `uids` has `NULL` keywords in that folder. Chunk the `IN` list (SQLite's variable limit) or use a temp table; an empty slice is `false`.
- `FolderView` gains `pub unknown_keywords: bool`. The driver sets it from `has_unknown_keywords(folder, uids_in_window_and_cached)`. It is computed once per folder per sync, before `plan_folder`.
- `flag_check(view, nothing_cached) -> FlagCheck`: with `view.unknown_keywords` it returns `FlagCheck::All` where it would return `Skip` or `ChangedSince`; `nothing_cached` still wins (`Skip`).

**Invariants:**
- A `NULL` row **outside** the window never forces `All`: it is not in the set the driver passes. Without this a row the server no longer lists in the window would force a full check on every sync.
- After one successful sync of a folder, no cached row inside the window is `NULL` unless the server did not answer for it (T2: an update for a row with unknown keywords always writes them). The next sync is back to `Skip` or `ChangedSince`.
- CONDSTORE state (`highestmodseq`) is stored exactly as before: forcing `All` does not change `finish`. The test at `sync.rs:325` stays.

**Implementer notes:** every `FolderView { .. }` literal gets the field: the `view()` helper in `plan/sync.rs` unit tests (`:228-241`, it builds `server, window, cached, cached_uids, kept, condstore`; add `unknown_keywords: false` there, and a `with_unknown()` sibling of `with_kept` for the new tests) and the driver's construction.

**Tests:**
- Planner: `unknown_keywords` turns `Skip` and `ChangedSince` into `All`; `nothing_cached` stays `Skip`; with the field `false` the existing tests pass unchanged.
- Store (T1's test file, since the signature changed): `has_unknown_keywords(folder, uids)` true only when a listed UID is `NULL`; a `NULL` UID not in the list gives false; empty list false; a list longer than the variable limit works.
- Driver, fake server: seed a cache with `NULL` keywords (the fake server holds `$Label1` on one message), sync with the server's MODSEQ unchanged: the row now has `["$Label1"]`, and a second sync does no full check (count the fake server's FETCH calls).
- Driver, outside the window: a cached row with `NULL` keywords whose UID is not in the sync window, everything else known and MODSEQ unchanged: the sync plans `Skip` and makes no flag FETCH (fails-first against a whole-folder `has_unknown_keywords`).

**Blast radius:** one full `FETCH 1:* FLAGS` per folder after the upgrade, within the window the planner already uses.

---

### T4: Core: Edit links only after a live check shows no keywords (core, sync)

**Tier:** trunk (`envio-core`, plus one function in `envio-sync`). **Testing:** unit plus `crates/envio-core/tests/edit.rs`.

**How Edit reaches the network today:** `Worker::edit_or_import` (`worker/edit.rs:25`) is synchronous. With a cached body it imports at once, with no network. Without one, `start_from_message` queues `live.wants.bodies` (and `whole`), the run loop turns that into `Step::Body(id)` (`plan.rs:158`, `run/mod.rs:111`), `sync::body` (`run/sync.rs:140`) fetches over the account's `ImapSession` and ends in `worker.body_fetched`, which resumes the Edit through `compose_body_fetched`. The live check follows the same path: a want, a step, a resume.

**Files:**
- `crates/envio-core/src/drafts/import.rs`: `Source`, `loses_nothing`, the `plain_flags` doc comment (`:38-40`), unit tests.
- `crates/envio-core/src/worker/edit.rs`: the decision (below).
- `crates/envio-core/src/worker/plan.rs`: `Wants.keywords: Vec<MessageRowId>` and `Step::Keywords(MessageRowId)` (next to `Step::Body`, `:158`, and the label match at `:98`); `run/mod.rs` (`run_step` arm); `run/sync.rs` (new `keywords` step function beside `body`).
- `crates/envio-sync/src/driver/` (a small new function next to `apply_flags`, `folders.rs:314`; or a new `driver/keywords.rs`, exported like `fetch_body`).
- `crates/envio-core/tests/edit.rs`.

**Signatures:**
- `Source` gains `pub keywords: Option<&'a [String]>`: the keywords the **server just reported** for the original, `None` when not confirmed. `plain_flags` stays (bits: Seen and Draft at most; its comment now says system flags only).
- `loses_nothing` adds `&& matches!(src.keywords, Some([]))`. This is the only place that decides.
- `pub async fn refresh_flags(server: &mut impl MailServer, local: &Local<'_>, message: &Message, pending: &Pending) -> Result<Option<Vec<String>>>` in `envio-sync`: `server.fetch_flags(&[message.uid], None)` (the same call as `replay.rs:221`), then `apply_flags` (refreshes the cache and removes it if `\Deleted`), then returns the keyword list of that UID from the reply. `None` when the server returned no entry or marked it `\Deleted`.
- `Worker::keywords_checked(&mut self, id: MessageRowId, fresh: Option<Vec<String>>)` and `Worker::keywords_failed(&mut self, account: Option<&str>)` (the failure twin, like `open_failed`; also called when the step is skipped because the account is paused or offline).
- `Worker.awaiting_keywords: BTreeMap<MessageRowId, Vec<(ComposeId, Vec<u8>)>>`: the Edits parked with the original's raw bytes (the body may be a big message not kept in the cache, so the bytes are kept in memory for the wait).

**Decision flow in `edit_or_import`, after the existing local-draft and damaged-file lookups and with `raw` at hand:**
1. Cache pre-filter: `db.keywords_of(folder.id, message.uid)` (T1). `None`, `Some(non-empty)`, a store error (logged at `warn`) or `plain_flags == false` all import now with `keywords: None` or the cached non-empty list. Detached, no network. This covers the common lossy cases for free.
2. Cache says `Some([])` and the other checks could link: if the account cannot run (`disk_full`, `is_paused`, `!can_run`, as `start_from_message:60-65`) import detached (`keywords: None`). Otherwise push `id` into `wants.keywords`, park `(compose, raw.to_vec())` in `awaiting_keywords`, and return. A second Edit of the same message while one is parked joins the list (the same dedup as `composing`, `start.rs:72`).
3. `keywords_checked(id, fresh)`: take the parked Edits; for each, import with `Source.keywords = fresh.as_deref()`. Only `Some([])` links. `keywords_failed`: import every parked Edit detached (`keywords: None`), never refuse, since a detached copy is always safe. The lookups of step 1 are repeated at resume (another Edit may have imported the message meanwhile, as `edit_or_import`'s own doc says).

**Invariants:**
- Link only when the server, asked now, reports no keywords. A stale cache of `[]` with a live `[$Forwarded]` detaches; so do an unreachable server, a paused or offline account, a failed fetch, an expunged message and a timeout.
- The detached copy's content is identical to a linked one's; only `link` differs, and `server.imported_from` is still set (as for the other lossy cases), the original stays on the server.
- A message with `\Seen` and `\Draft` only, cache `[]` and live `[]` links exactly as before.
- No network at all for Edits that already detach, and none for Edits of a draft that already has a local file.
- The extra step is one `UID FETCH <uid> (FLAGS)`; it never blocks the compose screen longer than the body fetch does (the Edit already waited for `ComposeReady` in that case).

**Tests (written first):**
- `import.rs`: `a_keyword_on_the_original_is_never_replaced` for `Some(["$Forwarded"])` (`assert_detached`, "keywords") and `a_message_with_unconfirmed_keywords_is_never_replaced` for `None`; they fail first because `Source` has no field. The `source()` helper gets `keywords: Some(&[])` so every existing test keeps its meaning; `flags_beyond_seen_and_draft_are_never_replaced` stays.
- `refresh_flags` (sync, fake server): returns the server's keywords; refreshes the cache's flags and keywords; `\Deleted` and a missing UID give `None`.
- `tests/edit.rs` with the fake server:
  - cache `[]`, server `[$Forwarded]`: Edit detaches, the first save uploads a new message and the original is still on the fake server (fails-first: today it is expunged), and the cache now holds `["$Forwarded"]`.
  - cache `NULL` and cache `["$Label1"]`: detaches with no FLAGS fetch (assert the fake server saw none).
  - cache `[]`, server `[]`: links and replaces as before.
  - the flags fetch fails (fake server error) and the account is paused: detaches, no `ComposeRefused`.
  - two Edits of one message during the wait: one draft, not two.
- Worker unit tests beside `edit.rs`'s: an Edit that needs the check parks and sets `wants.keywords`; `keywords_checked` and `keywords_failed` resume it (the style of `the_fetched_body_opens_the_import_and_a_failed_fetch_refuses`).

**Blast radius:** what Edit does to user mail on the server. The change only ever removes a replace, so the failure mode is a duplicate draft on the server, never a lost one. The new step runs only for Edits that would have linked.

---

### T5: End to end and docs (leaf)

**Tier:** leaf (`crates/envio-e2e/` tests, docs). **Testing:** `cargo make e2e-it` (throwaway Stalwart).

**Files:** a new test file in `crates/envio-e2e/tests/` (cross-crate: net, sync, store, core); the spec.

**Behaviour:** against Stalwart, put a plain-text draft in Drafts, add `$Label1` with STORE +FLAGS, sync, Edit and save: the original is still there and the account has two drafts. Then remove the keyword on the server, sync (a flag check), Edit a fresh copy: the original is replaced. This proves the stale-cache half end to end, which the fake server only approximates.

**Docs:** one sentence in the spec's Drafts paragraph (`docs/superpowers/specs/2026-11-07-envio-design.md`): a server draft that carries IMAP keywords, or whose keywords are not cached yet, is never replaced by Edit; it stays on the server and Edit saves a new draft. The PR that lands T5 closes #324 (T4's says "Refs #324").

## Out of scope

- Showing, setting or filtering by keywords or labels in the UI.
- Carrying keywords through Undo (Decision 3), or into the draft that replaces an original (a draft that carried keywords is never replaced).
- Gmail `X-GM-LABELS`: labels over the Gmail extension are not IMAP keywords and `envio-net` does not fetch them. A Gmail label counts only when the server also exposes it through FLAGS.
