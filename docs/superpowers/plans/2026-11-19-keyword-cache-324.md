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
7. **Edit with `NULL` cached keywords** does not fetch them live: it imports detached. After T3's backfill this is rare (a row that has never had a flag check).

## Tasks

Order: T1 → T2 → T3 → T4 → T5. T1 to T4 are trunk. T2 and T3 may be one PR if small. T4 needs only T1's accessor to compile, but ships after T3 so the cache is filled before Edit relies on it (otherwise every Edit would detach until the backfill ran).

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
- `Db::has_unknown_keywords(&self, folder: FolderId) -> Result<bool>`
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
- `has_unknown_keywords` true for a fresh row, false after `set_keywords` on every row, per folder only.
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

**Tests:**
- `keyword_list`: none, unsorted, duplicates. The existing `flags_go_both_ways_without_keywords` and `flags_round_trip_for_every_combination` stay as they are (regression guard that bits ignore keywords).
- `plan_flags`: a keyword-only change appears in `keywords` and not in `set` (fails-first: today it appears nowhere); a change in both appears in both; unchanged keywords give nothing; unknown (`None`) cached keywords give an entry even when the server list is empty; a `\Deleted` update only removes.
- Driver, fake server: the first sync stores keywords for every row; a later server-side keyword change (the CONDSTORE path and the plain `FlagCheck::All` path) reaches the cache with `flags_changed == 0`; a bit change still counts.
- A pending local flag change does not hide a server keyword change.

**Blast radius:** the core sync loop. The only new I/O is one extra UPDATE per changed keyword set, inside the existing transaction.

---

### T3: Sync: backfill rows cached before the migration (sync)

**Tier:** trunk (`envio-sync`). **Testing:** unit (planner) plus `crates/envio-sync/tests/folder_sync.rs`.

**Files:** `plan/sync.rs` (`FolderView`, `flag_check` `:141`); the place in `driver/folders.rs` that builds `FolderView` (it asks the store `has_unknown_keywords(folder)`).

**Signatures:**
- `FolderView` gains `pub unknown_keywords: bool`.
- `flag_check(view, nothing_cached) -> FlagCheck`: with `unknown_keywords` it returns `FlagCheck::All` where it would return `Skip` or `ChangedSince`; `nothing_cached` still wins (`Skip`).

**Invariants:**
- After one successful sync of a folder, `has_unknown_keywords` is false unless the server did not answer for some cached UID (T2: any update for a row with unknown keywords writes them).
- The next sync is back to `Skip` or `ChangedSince`, so the full check is a one-off per folder. A row that stays `NULL` (the server never answers for it) would force `All` on every sync; to bound that, the driver computes `unknown_keywords` over the UIDs in `plan.check` only, so a row outside the sync window never forces it.
- CONDSTORE state (`highestmodseq`) is stored exactly as before: forcing `All` does not change `finish`. The test at `sync.rs:325` stays.

**Tests:**
- Planner: `unknown_keywords` turns `Skip` and `ChangedSince` into `All`; `nothing_cached` stays `Skip`; with the field `false` the existing tests pass unchanged.
- Driver: seed a cache with `NULL` keywords (the fake server holds `$Label1` on one message), sync with the server's MODSEQ unchanged: the row now has `["$Label1"]`, and a second sync does no full check (count the fake server's FETCH calls).

**Blast radius:** one full `FETCH 1:* FLAGS` per folder after the upgrade, within the window the planner already uses.

---

### T4: Core: Edit treats a keyword as lossy (core)

**Tier:** trunk (`envio-core`). **Testing:** unit plus `crates/envio-core/tests/edit.rs`.

**Files:** `crates/envio-core/src/drafts/import.rs` (`Source`, `loses_nothing`, the `plain_flags` doc comment at `:38-40`, its unit tests); `crates/envio-core/src/worker/edit.rs:101-104` (read the keywords and pass them); `crates/envio-core/tests/edit.rs`.

**Signatures:**
- `Source` gains `pub keywords: Option<&'a [String]>`: the cache's keywords of the original, `None` when unknown. `plain_flags` stays (bits: Seen and Draft at most) and its comment now says it covers system flags only.
- `loses_nothing` adds `&& matches!(src.keywords, Some([]))`. This is the only place that decides.
- `edit.rs` calls `db.keywords_of(folder.id, message.uid)`; a store error counts as unknown (`None`) and is logged at `warn`, never fails the Edit.

**Invariants:**
- Link only with `Some([])`. `Some(["$Forwarded"])`, `Some(["$Label1","$Junk"])` and `None` all give a detached copy: `link: None`, `server.imported_from` still set (as for the other lossy cases), the original stays on the server.
- A message with `\Seen` and `\Draft` only and `Some([])` links exactly as before.
- The detached copy's content is identical to a linked one's; only `link` differs.

**Tests (written first):**
- `import.rs`: `a_keyword_on_the_original_is_never_replaced` for `Some(["$Forwarded"])` (`assert_detached`, "keywords") and `a_message_with_unknown_keywords_is_never_replaced` for `None`; they fail first because `Source` has no field. The `source()` helper gets `keywords: Some(&[])` so every existing test keeps its meaning; `flags_beyond_seen_and_draft_are_never_replaced` stays.
- `tests/edit.rs`: a server draft cached with keywords stored via `set_keywords`: Edit opens a detached draft, the first save uploads a new message and the original is still on the fake server (fails-first: today it is expunged). The same with `NULL` keywords (the post-migration row). With `Some([])` it links and replaces as before.

**Blast radius:** what Edit does to user mail on the server. The change only ever removes a replace, so the failure mode is a duplicate draft on the server, never a lost one.

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
