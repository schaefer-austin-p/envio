# Envio Microsoft Graph backend design

Release 0.4.0. This spec covers:
- #382: a Graph backend for Outlook.com personal accounts;
- #342: moving existing Outlook.com IMAP accounts onto it;
- the wizard's "Choose a provider" step, which also hosts #221
  (app-password help) and #384 (MX lookup).

It builds on the main spec, `docs/superpowers/specs/2026-11-07-envio-design.md`,
whose privacy and unsent-work rules still apply.

The design was reviewed against the code in nine rounds by fresh agents.
The facts under "Platform facts" were measured on a real Outlook.com
account.

## Goals and non-goals

Goals:
- Outlook.com, Hotmail and Live accounts sync, send and keep drafts
  through Microsoft Graph instead of IMAP and SMTP. Microsoft's IMAP for
  consumer accounts is unreliable with OAuth (#342).
- The planner, Journal replay, Outbox, drafts and search keep their
  logic. Graph sits behind the existing `MailServer` and `MailSender`
  traits as an adapter.
- New-mail notifications keep working for those accounts, as they do
  today over IMAP IDLE.
- Existing Outlook.com accounts move to Graph after one more sign-in,
  with no lost Outbox items, drafts or pending actions.

Non-goals:
- Graph for Microsoft 365 work and school accounts. Those stay on IMAP
  with OAuth.
- Graph push (change notifications or webhooks). They need a public
  endpoint, so polling is used instead.
- Partial body download on Graph. Bodies always download whole.
- A folder rename on the server that keeps the folder's cache. A rename
  is treated as remove plus add.

## Platform facts (measured 2026-11-21, consumer tenant)

- The `tid` claim of a personal account's id_token is
  `9188040d-6c67-4c5b-b112-36a304b66dad`.
- MIME draft create (`POST /me/mailFolders/drafts/messages`, base64
  `text/plain`) keeps our Message-ID, and `isDraft` is true.
- MIME `sendMail` keeps our Message-ID, and the server files the Sent
  copy itself. That copy appears asynchronously.
- A MIME copy POSTed into Sent stays `isDraft`, and `PR_MESSAGE_FLAGS`
  cannot clear that.
- Delta reports changes to read, flag and categories.
- Immutable IDs (`Prefer: IdType="ImmutableId"`) survive a move.
- The folder list carries `wellKnownName`, and the well-known aliases
  resolve.
- A list call returns `internetMessageHeaders` and size
  (`Integer 0x0E08` via `singleValueExtendedProperties`).
- `$search` pages through `@odata.nextLink`.
- Received copies have no Bcc header. The Sent copy keeps Bcc.
- MIME create accepted 6 MB, and sendMail accepted 12 MB (202). The
  6 MB mail was delivered. Delivery of the 12 MB mail was not confirmed,
  and a 202 does not mean delivered.
- Successful calls return no throttle headers.
- Not measured: `$batch`, first-sync time on a big mailbox, Bcc to
  another mailbox, a Bcc-only mail, delivery above 12 MB.

## Architecture

```
core  ── Backend::{Imap(ImapSession, SmtpSession), Graph(GraphServer)}
         generic step functions over MailServer / MailSender
sync  ── GraphServer: MailServer + MailSender      (adapter, uid map)
net   ── graph::GraphClient (HTTP/1.1, JSON parsers, Bearer)
store ── migration 0006 graph_folders / graph_messages, graph_map fns
```

The dependency direction is unchanged: GraphServer lives in envio-sync,
which may use both net and store. Net and store stay apart.

### envio-net `graph/`

**HTTP client**
- HTTP/1.1 on tokio and rustls, reusing `tls.rs`, `trust.rs` and
  `verifier.rs`. The plan decides between hand-written code and a crate
  that passes `cargo audit`.
- Supports any method, custom headers, chunked and Content-Length
  bodies, and keep-alive. Every request sends
  `Prefer: IdType="ImmutableId"`. Other preferences go in the same
  header value, comma-separated.
- Connection reuse:
  - `send_mime` and `create_mime` always use a fresh connection. A
    reused socket that the server already closed would make a send
    look unconfirmed.
  - GET, PATCH, DELETE and the `$batch` POST (a read) retry once on a
    fresh connection if a reused connection fails before any response
    byte arrives. `move_to` (POST) does not retry; a failure there is
    `Busy`, and replay retries it, because a move is idempotent by
    immutable id.

**Token**
- Signature:
  `trait Bearer { fn bearer(&self, force: bool) -> LocalBoxFuture<'_, Result<SecretString>>; }`.
- This is the same non-Send pattern as `Refresher` in
  core/worker/token.rs. Core implements it over `TokenHandle`. It keeps
  the last `Granted` behind a `RefCell`, so that `force` can call
  `renew(&granted)`.
- Core's problems map to net errors:
  - `Temporary` and `Offline` → `Busy`;
  - `SignIn` and any other pause → `SignIn`;
  - every other problem → `Io` with its message.
- On a 401 the client calls `bearer(true)` once and retries. It returns
  `SignIn` only if that fresh token is also refused.
- If no `Granted` is held yet, `bearer(true)` calls `access(true)`
  instead of `renew`.

**Calls**
- `folders()`: `includeHiddenFolders=true`, walks `childFolders`.
- `delta(folder_id, link)`:
  - `$select=receivedDateTime,isRead,flag,categories,isDraft`;
  - `Prefer: odata.maxpagesize=200`;
  - returns changes, removals, and the next or delta link.
- `summaries(ids)`:
  - JSON `$batch`, at most 20 sub-requests per call;
  - each sub-request is
    `GET /me/messages/{id}?$select=internetMessageHeaders,receivedDateTime&$expand=singleValueExtendedProperties($filter=id eq 'Integer 0x0E08')`
    and carries the ImmutableId `Prefer`;
  - responses are matched by `id`, not by order;
  - each sub-status maps through the error table:
    - A sub-404 drops that message.
    - Any sub-429 or sub-503 makes the whole call `Busy`, and the good
      results in that batch are thrown away (accepted). Retry-After is
      read from the sub-response's `headers`.
    - A sub-401 makes the whole call go through the 401 rule.
    - Any other sub-status makes the whole call fail with that status's
      error.
  - If `$batch` fails the fixture check (plan task 1), the fallback is
    one GET per id over keep-alive.
- `message_mime(id)`: `GET /me/messages/{id}/$value` → `Vec<u8>`.
- `get_categories(id)`: read just before a keyword change.
- `patch(id, read?, flag?, categories?)`.
- `move_to(id, folder_id)`.
- `permanent_delete(id)`.
- `create_mime(folder_id, eml)`.
- `send_mime(eml)`.
- `search(folder_id, text)`: `$search`, paged.
- `find_message_id(folder_id, mid)`: `$filter=internetMessageId eq '<…>'`.
  The id is passed through as `search_id` returns it (message_id.rs:15),
  with its angle brackets. This filter is not measured on a folder yet,
  so it is part of plan task 1.
- `delta` returns `DeltaPage::Page{..}` or `DeltaPage::Expired` (on 410
  or `syncStateNotFound`). Expired is not a `NetError`.

**Parsers**
- Pure functions, separate from I/O, tested on recorded fixtures.

**Errors (status → `NetError`)**

| Status | Result |
|---|---|
| 401 | Fresh token once, then `SignIn` |
| 403 | `Refused`. On `sendMail`, an error code that signals a send limit gives `Busy` instead |
| 404 on a message read | Gone (left out, or `Ok(None)`) |
| 404 on patch, move or delete | `Ok(())` |
| 410 / `syncStateNotFound` on delta | `DeltaPage::Expired` (the adapter resets the folder) |
| 409, 423 | `Busy` |
| 429, 500, 502, 503, 504 | `Busy`. The client retries once inside the call only when `Retry-After` ≤ 10 s; otherwise the existing health backoff applies |
| 400 (any), 413 | `Refused` |
| 507 (mailbox full) | `Refused` |
| Any other status | `Refused` with the status in the message |
| TLS `Untrusted` / `PinChanged` | `NetError::Tls` (Graph has nowhere to store a pin, so no accept action) |

`send_mime` is stricter, because a requeued send that Graph already took
goes out twice:
- Confirmed not sent: only 400, 401 (after the fresh-token retry), 403,
  409, 413, 423, 429, 503 and 507. These map as in the table above.
- Any other status (500, 502, 504, and so on), and any timeout or
  dropped connection after the body was written, is `unconfirmed`. The
  item becomes `MayHaveBeenSent` and goes through Resolve.
- In-call retries on `send_mime`: only the 401 fresh-token retry, and a
  429 or 503 with `Retry-After` ≤ 10 s. A 500, 502 or 504 is never
  retried inside the call.
- `create_mime` has no in-call status retry, so a draft is never
  created twice by the client.

`NetError` gets one new variant,
`Storage { kind: StorageKind, msg }`, with
`StorageKind = Damaged | DiskFull | Other`. GraphServer fills it from
`StoreError`.

### envio-store: migration 0006 and `graph_map`

```
graph_folders(
  folder_row_id INTEGER PRIMARY KEY REFERENCES folders ON DELETE CASCADE,
  graph_id TEXT NOT NULL, generation INTEGER NOT NULL,   -- non-zero u32
  uid_next INTEGER NOT NULL, low_next INTEGER NOT NULL,
  modseq INTEGER NOT NULL, delta_link TEXT NOT NULL DEFAULT '')
graph_messages(
  folder_row_id INTEGER NOT NULL REFERENCES folders ON DELETE CASCADE,
  graph_id TEXT NOT NULL, uid INTEGER NOT NULL, received_at INTEGER NOT NULL,
  flags INTEGER NOT NULL, categories TEXT NOT NULL, modseq INTEGER NOT NULL,
  PRIMARY KEY (folder_row_id, uid), UNIQUE (folder_row_id, graph_id))
```

- envio-sync has no rusqlite dependency, so the store exposes:
  - `GraphMap::open(path)`, its own connection: WAL, `foreign_keys=ON`,
    a 5 s busy_timeout, no migrations. Its methods each run in their own
    transaction.
  - `Db` methods for the calls made inside a step (`graph_reset`,
    `graph_upsert_folder`, `graph_commit`).
- Both sides share private `graph_map` functions that take a
  `&rusqlite::Connection` and open no transaction of their own.
- `reset(conn, folder_row_id)`:
  - mints a new generation that is non-zero and differs from the old
    one;
  - deletes the folder's message rows;
  - sets `uid_next = 2^31`, `low_next = 2^31 - 1`, `modseq = 0` and
    `delta_link = ''`.
- The map lives exactly as long as its folder row. Dropping a cache,
  removing a folder or removing an account cascades to it, and that is
  intended.
- A store rebuild (`rebuild.rs`) loses it. The random generation then
  makes that a clean resync, and Journal actions fall back to
  Message-ID.

### envio-sync: `GraphServer`

GraphServer owns:
- a `GraphClient`;
- a `GraphMap` (see above);
- the in-memory path → graph_id map;
- each folder's pending delta.

**Connection rule**
- GraphMap's connection is used only outside `db.step` closures. Drivers
  never call the server inside a step.
- The two trait hooks that do run inside a step write only through the
  `&Db` they are given.
- GraphMap reads generation, `uid_next` and `low_next` fresh on every
  call and caches none of them, so a reset made through `&Db` is seen.

**Trait additions** (all have a default that does nothing, so IMAP is
unchanged)
- `refresh(&mut self, folder) -> Result<()>`.
- `commit_folders(&mut self, db: &Db, account) -> Result<()>`.
- `commit_folder(&mut self, db: &Db, folder_id) -> Result<()>`.

`Caps` gets `no_partial: bool`, which defaults to false.

**UID map**
- uid ranges:
  - High range, `[2^31, 2^32)`, increasing: everything a refresh sees.
    Delta is unfiltered, so that is the whole folder.
  - Low range, `[1, 2^31)`, counting down: only messages that search,
    Resolve or a replay lookup find before a refresh maps them. Such a
    message keeps its low uid and never notifies; that is accepted.
  - When either range runs out, the folder is reset.
- The folder is reset when `uid_next` would reach `2^32 - 1`, so it
  always fits the u32 `FolderState.uid_next`.
- Every new graph_id → uid row is written before the uid is handed out,
  and a uid is never reused. This is what makes multi-call syncs, Load
  older and crashes safe.
- The generation is reported as `uid_validity`. A rebuilt map is
  therefore a new UID space, so `plan::replay::find` falls back to
  Message-ID instead of acting on a reused uid.

**`refresh(folder)`** (only `sync_folder_as` calls it, before `select`)
1. If a delta for this folder is already pending under the stored link,
   it returns at once. A multi-call first sync therefore lists the
   folder once.
2. Otherwise it drains every delta page into memory.
3. It sorts the new ids by `receivedDateTime` and allocates their
   high-range uids in one GraphMap transaction, so the planner's
   "highest uid first" means newest first.
4. It applies flag changes, bumping modseq, and removals.
5. It keeps the new delta link pending.
6. On `DeltaExpired`, or when a range runs out, it calls
   `graph_map::reset` on GraphMap's connection, drops the pending delta
   and starts again from an empty link. A full delta reports no
   removals, so only a new generation is safe.
- A pending delta is lost when the connection closes. Re-draining is
  idempotent per graph_id.

**`select(folder)`** (no network)
- Returns the generation as `uid_validity`, plus `uid_next`,
  `exists` and `highest_modseq`.
- `commit_folders` creates the `graph_folders` row and mints its
  generation there; `select` and the first refresh keep it.
- GraphServer keeps path → (graph_id, folder_row_id) in memory, filled
  by `commit_folders` and by the lookup-miss walk.

**`commit_folder(db, id)`**
- Called inside the step that runs `set_sync_state` (folders.rs:228).
- Saves the pending delta link and modseq through `db`, then clears the
  pending delta.

**`commit_folders(db, account)`**
- Called inside the `sync_folders` step (folders.rs:25-40), after the
  folder rows are upserted.
- For every listed selectable folder, it looks up the row id through
  `db` and upserts `graph_folders.graph_id`.
- An existing path whose graph_id changed is reset through
  `graph_map::reset` on `db`.

**Folder paths**
- The Inbox (`wellKnownName = inbox`) is `INBOX`, and its children are
  `INBOX/<name>`.
- Other folders join their displayName segments with `/`.
- Inside a segment, `/` becomes U+2215 and `.` becomes U+2024, because
  `folder_order.rs` and `ui/convert.rs` split on both. The store has no
  name column (the shown name is derived from the path), so the drawer
  shows these look-alike characters. That is accepted.
- Hidden and non-mail folders are filtered out in the adapter.
- A path that is not in memory triggers one fresh folder walk.

**Messages**
- `search_since(date)` and `search_all` answer from `graph_messages`
  by `received_at`.
- `fetch_summaries(uids)` maps through the map, then calls
  `summaries(ids)`.
  - `Summary.header` is rebuilt from `internetMessageHeaders`,
    `internal_date` comes from `receivedDateTime`, and size from 0x0E08.
  - Flags come from the `graph_messages` row.
  - Messages that are gone are left out.
- `fetch_flags(changedsince)` returns rows with modseq greater than
  `changedsince`.
- `fetch_body` uses `message_mime`. `fetch_partial` and `fetch_section`
  are never reached, because `no_partial = true`.
- Flags:
  - `\Seen` ↔ `isRead`;
  - `\Flagged` ↔ `flag.flagStatus == flagged`;
  - `\Draft` ← `isDraft` (read-only);
  - keywords ↔ `categories`. Before a PATCH, the adapter reads the
    current categories and applies the add or remove to that set, so
    another client's categories are not overwritten.
  - `\Answered` is dropped on write and never reported. A test shows
    replay does not loop on it.
- `store_flags`, `move_to(path)` and `delete` resolve uid → graph_id
  and path → graph_id. Delete is `permanent_delete`, and moving to
  Trash is `move_to`.
- `append` is used for drafts only (`create_mime` into Drafts).
- `search_message_id`, `search_message_id_deep` and `search_text` use
  the server's filter and search. Hits without a mapping get low-range
  uids.

**`send`**
- Uses `send_mime` on the copy of the eml that goes to Graph.
- The Bcc list is the envelope recipients whose bare address,
  compared case-insensitively, is not in To or Cc. One folded `Bcc:`
  header goes at the end of the header block, replacing any existing
  one. The Outbox eml is never changed.
- A timeout or a dropped connection after the body was written means
  unconfirmed.

**Caps**
- `condstore` on (emulated), `qresync` off, `idle` off, `move` on,
  `special_use` on, `no_partial` on.

### envio-core

**Backend**
- `enum Backend { Imap(ImapSession, SmtpSession), Graph(GraphServer) }`
  replaces the hard-wired `ImapSession`.
- The step functions in run/mod.rs, sync.rs, mail.rs and prefetch.rs,
  and in worker/older.rs, become generic over the traits.
  `take_changes` becomes a trait default.
- `Step::Send`, which today bypasses `imap()`, gets a Graph arm that
  uses the same GraphServer.
- This refactor lands first, as its own PR, with no Graph code.

**Caps and size checks**
- `Backend::caps()` gives the backend's static caps (`GRAPH_CAPS` for
  Graph), even while `Conn::Closed`.
- One helper, `big(caps, size)`, replaces the direct `is_big` uses in
  body.rs, run/sync.rs and read.rs.
- Prefetch on Graph skips bodies over 10 MB.

**Sent filing**
- `files_itself` comes from the backend (Graph → true), not from the
  SMTP host (run/mail.rs:65).

**Errors**
- `ErrorKind::Damaged` is new.
- `net_kind` (sync error.rs:44):
  - `Storage{Damaged}` → Damaged;
  - `Storage{DiskFull | Other}` → Local.
- `SyncError::kind()` for `Store(e)` with `e.is_damage()` changes from
  Local to Damaged (error.rs:39).
- `from_kind(Damaged)` gives `Problem::Damaged`, which leads to a
  rebuild, on every path: folder steps, replay, send and file.
- Matches to extend:
  - `plan/replay.rs:121` and `plan/outbox.rs:70`, where Damaged is a
    Stop and not a Retry;
  - `problem::from_kind`;
  - `problem::from_net`, where `Storage` maps like `from_store`
    (Damaged, DiskFull, else Local);
  - `cause::net_line` (the exhaustive `NetError` match, cause.rs:54);
  - `sync::net_kind`, for the new `Storage` variant;
  - `driver/account.rs:61-63`, where Damaged goes next to Local.
- `driver/drafts.rs:147` has a catch-all and needs no change.
- `cause` and `from_sync` label Graph errors "Outlook.com", not
  "IMAP".

**Rebuild**
- `run/mod.rs::rebuild()` first sets `live.conn = Closed` for every
  Graph account. That drops each GraphServer and its connection. Then
  it calls `Store::rebuild`.
- IMAP connections are left alone.

**Background poll**
- `Timing.poll_ms` is new. turn.rs sets it to
  `Some(BACKGROUND_POLL_MS)` (5 min) for an account that notifies and
  has `live.idle_capable == Some(false)`. The existing
  `Timing.idle_capable` bool is unchanged.
- Graph accounts start with `live.idle_capable = Some(false)`, so they
  never run `begin_open` or `prepare_idle`.
- `next()`: with `!foreground && poll_ms.is_some() && now >= next_full_ms`
  it returns `Run`. In its Sleep branch it returns
  `until_ms = Some(next_full_ms)`.
- `account_wake` gets the same background arm.
- In the background Run branch:
  1. `next_full_ms = now + BACKGROUND_POLL_MS`, set unconditionally, so
     there is no spin;
  2. the Inbox is added to `wants.folders`;
  3. `background_steps` runs.
- IMAP servers without IDLE get this poll too, which is intended.

**Resolve for an unconfirmed Graph send**
- `OutboxMeta` gets `resolve_tries: u32` and `resolve_after_ms: i64`,
  both `#[serde(default)]`.
- `Outbox::set_resolve(id, tries, after)` reads, modifies and writes
  the meta and never changes `state`. `set_state` to Queued or Accepted
  zeroes both fields.
- Resolve retries apply to Graph accounts only. On IMAP a miss still
  asks at once.
- `file_sent` gains a `now` parameter. `set_resolve` leaves the store's
  `unwritten` set alone.
- `file_sent(.., now)` skips an item while `now < resolve_after_ms`,
  before it selects Sent.
- A miss, or a `Refused` on the Sent select, goes into
  `Filed.retry: Vec<OutboxId>`, and core sets `after = now + 10 s`.
  The third miss goes into `ask`.
- `next_resolve(metas, after)` gives the earliest
  `resolve_after_ms > after.max(0)` among `MayHaveBeenSent` items. The
  `handled_resolve_ms` watermark and an `on_timers` branch then set
  `wants.file`.
- The retry waits for the foreground.
- The tries persist across restarts, so a restart neither resets nor
  skips them.
- A Graph account without a Sent folder asks at once.
- Three tries 10 s apart is a starting value, kept in one constant and
  tuned after the manual gate measures how long the Sent copy takes to
  appear.

**"Move to Sent"**
This is gated on the backend, not on `files_itself`, so IMAP Gmail
(where `files_itself` is also true) keeps "Move to Sent".

How `can_file` flows:
- `OutboxStatus::MayHaveBeenSent { asked, can_file }`. `can_file` is
  `backend != Graph`; `worker/outbox.rs:48` looks the backend up.
- In the UI:
  - `core::outbox::status` takes `can_file`; tests that build
    `MayHaveBeenSent` (outbox.rs:173, convert.rs:421) and any QML mock
    outbox model change too;
  - `OutboxRowUi` gets `can_file`;
  - `ui/convert.rs` fills it;
  - `rows.rs` `OUTBOX` gets a `canFile` role;
  - `OutboxRow.qml` shows "Move to Sent" only when it is true.
- `text/outbox.rs` uses a status line without "or move to Sent" when
  `can_file` is false.
- `decide(.., graph: bool)` turns `MoveToSent` into `NotNow` on Graph,
  so a stale command cannot remove the item without a Sent copy. The
  existing `decide` tests get the new argument.

On Graph the choices are "Send again" and "Delete".

### Accounts and tokens

**Accounts file**
- `AccountSettings` gets `backend: Imap | Graph` (serde default Imap),
  and `imap` and `smtp` become `Option<Server>`.
- The file is written as v4 only when it holds a Graph account or an
  account that is mid-migration. Otherwise it stays at v2 or v3, as
  today's `names_auth` rule does, so downgrades keep working.
- Every `.imap` and `.smtp` use is handled:
  - `pin_problem`, `unreadable`, `stand_in`;
  - `conn.rs`, `trust.rs`, the sender check in `read.rs`;
  - `setup.rs`, `setup_thread.rs`, `oauth.rs`.

**Provider**
- `Provider::microsoft_graph()` uses the same client ID, the
  `Mail.ReadWrite Mail.Send offline_access openid email profile`
  scopes, and `ProviderKind::MicrosoftGraph`.
- `Identity` gets `tid`.

**Token storage**
- `OAuthTokens` and `TokenCells` are keyed by `(account, ProviderKind)`.
  - `get`, `set`, `remove` and `handle` all take the kind.
  - `Cell.key` holds both.
- Account removal (`remove.rs`) and `TokenCells::forget` drop both
  kinds.
- A Graph account keeps `auth: "microsoft"`, so `is_oauth()` is
  unchanged.
- `tid` is read from the id_token even when `email` and
  `preferred_username` are missing; `identity_of`, `judge` and
  `identity_matches` change to match.
- `set_paused` from `Worker::report` and from `worker/run/mod.rs:266` applies to the token of the
  account's active backend. During a migration, only the IMAP token is
  in use before the switch.

**oauth.json**
- v1, today: `{"version":1,"tokens":{"<account>":"<refresh>"}}`.
- v2:
  `{"version":2,"tokens":{"<account>":{"imap":"<refresh>","graph":"<refresh>"}}}`,
  where either key may be missing.
- The file is written as v2 only when an account holds a Graph token;
  otherwise it stays v1. An older build refuses v2, which fails closed.
- v2 loads v1 files with every token as `imap`.

### Wizard

New first step, "Choose a provider":

| Choice | Path |
|---|---|
| Outlook.com, Hotmail, Live | Microsoft sign-in with Graph scopes |
| Microsoft 365 (work or school) | Microsoft sign-in, IMAP with OAuth |
| iCloud, Gmail, Yahoo | App-password help (#221), then IMAP |
| Other | Auto-detect, plus the MX lookup (#384) |

- After any Microsoft sign-in, `tid` decides: the consumer tenant gives
  Graph, any other tenant gives IMAP with OAuth.
- The token's scopes match only the picked path. On a mismatch (a work
  account picked as Outlook.com, or the reverse), the wizard says
  "This is a work or school account" or "This is a personal account"
  and runs a second sign-in with the other scope set.
- The second token must have the first token's email and `tid`. If it
  does not, or the tenant again does not match, the wizard takes the
  refusal path below. There is no third sign-in.
- If consent is refused, either by the user or by a work tenant's admin
  for Mail.ReadWrite, the wizard shows the refusal and goes back to the
  provider step. Nothing is saved.
- The frames are mocked up in `docs/design/carbon-wizard.html`, and a
  separate agent checks them before implementation.

### Moving existing Outlook.com accounts (#342)

**Eligibility**
- A consumer account qualifies by `tid`, or for an old account by the
  `outlook.com`, `hotmail.*`, `live.*`, `msn.com` or `windowslive.com` domain with the Microsoft
  provider.
- Old accounts store no `Identity`, so a consumer account on a custom
  domain does not qualify. It stays on IMAP.
- "Update sign-in" signs in with Graph scopes. If the `tid` is not
  the consumer tenant, the Graph token is dropped and the account stays
  on IMAP.
- Such an account shows "Update sign-in" (mocked up with the wizard),
  and keeps working on IMAP until the user does it.

**Consent**
- After consent, the Graph token is stored as the account's second
  token. The IMAP token is untouched.

**Migration**
- A resumable one-shot migration runs. Its step is recorded in the
  accounts file, which forces v4.
  1. Pending Journal actions are replayed on IMAP first: the replay
     step runs until the Journal is empty.
     - Folder paths differ between the backends ("Deleted" against
       "Deleted Items"), so actions cannot carry over.
     - If IMAP is offline or refuses sign-in, the migration waits at
       this step.
     - After 3 failed replay runs, the user can choose "Switch anyway",
       which drops the remaining actions with a notice.
     Step 1 also waits until no `Accepted` Outbox item is left to
     file on IMAP. Its failure count is kept in the accounts file
     with the step. Actions queued after step 1 ends are dropped with
     the same notice.
  2. The account's cache rows and bodies are cleared, IMAP folder rows
     included.
  3. The backend switches to Graph, and the IMAP token and `Server`
     entries are removed.
  4. Core closes `conn`, sets `idle_capable = Some(false)`, sets
     `wants.full` and resets `idle_synced_first`, as `Live::new` does.
     A full sync follows.
- The Outbox and drafts live outside the cache and are kept.

## Testing

- **Parsers:** tests on recorded JSON fixtures for folders, delta,
  `$batch`, search, errors and the id_token.
- **Client:** tests against a mock Graph server in envio-net, covering
  401 then a fresh token, 410, 429 inside `$batch`, a dead keep-alive
  connection, and the fresh connection on send.
- **GraphServer against the mock, on a temp-file DB** (not in memory,
  because two connections are needed):
  - crash replay of the same delta;
  - a multi-call first sync that lists the folder once;
  - 410 leading to a new generation and a wipe;
  - a changed graph_id leading to a reset;
  - newmail ignoring low uids;
  - Bcc injection;
  - categories read-merge-write;
  - `\Answered` without a replay loop.
- **Core:**
  - the background poll fires and does not spin;
  - Resolve retries three times and then asks;
  - MoveToSent is refused on Graph;
  - Damaged leads to a rebuild from replay;
  - the migration resumes after a crash at each step.
- **Plan task 1:** record fixtures on the test account (the owner signs
  in), including `$batch` and `find_message_id` on a folder.
- **Manual gate before release, on the test account:**
  - a big-mailbox first sync, timed, plus a restart in the middle;
  - Bcc to another mailbox;
  - a Bcc-only mail;
  - delivery above 12 MB;
  - migrating an existing IMAP account;
  - a background notification while Envio runs in the background;
  - how long the Sent copy takes to appear after sendMail (this tunes
    Resolve).
- The gate blocks the release. If Bcc delivery fails, Graph send is held
  back until a fix is found; one candidate is a separate sendMail per
  Bcc recipient.

## Delivery order

1. The `Backend` refactor in core: no behaviour change, its own PR.
2. envio-net `graph/`: client, Bearer, parsers, fixtures.
3. Store migration 0006 and `graph_map`.
4. `GraphServer` and the trait hooks.
5. Core wiring: errors, the background poll, Resolve, MoveToSent, caps.
6. Accounts and tokens, and the wizard provider step (after its mockup
   check).
7. The #342 migration.
8. The manual gate.
