# Envio Plan: Microsoft Graph backend for Outlook.com (#382, #342)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Outlook.com, Hotmail and Live accounts sync, send and keep drafts through Microsoft Graph behind the existing `MailServer` / `MailSender` traits; existing Outlook.com IMAP accounts move over after one "Update sign-in"; the wizard gains a "Choose a provider" first step.

**Architecture:** A pure-adapter design. envio-net gains `graph/` (a hand-written HTTP/1.1 client, JSON parsers, a `Bearer` token trait). envio-store gains migration 0006 (`graph_folders`, `graph_messages`) and `graph_map` functions. envio-sync gains `GraphServer`, which implements both traits and keeps a uid ↔ graph_id map. envio-core switches from a hard-wired `ImapSession` to a `Backend` enum; the planner, Journal replay, Outbox, drafts and search keep their logic.

**Tech Stack:** Rust 2024 (rust-version 1.94), tokio, rustls 0.23 / tokio-rustls 0.26, serde_json, rusqlite, cxx-qt, QML (Lomiri).

**Spec:** `docs/superpowers/specs/2026-11-21-envio-graph-backend-design.md`. Mockups: `docs/design/carbon-provider.html`. Executors read both; this plan does not repeat the spec's tables (the status → `NetError` table, the `send_mime` confirmed-not-sent set, the uid ranges, the migration steps). Where this plan and the spec differ, the **Decisions** section below says so and wins.

## Global Constraints

- Dependencies point one way: ui → core → sync → net, store, mime. net, store and mime never depend on each other. `GraphServer` lives in envio-sync; envio-sync gets no rusqlite dependency.
- Migrations are numbered and never edited after merge. **0006** is next (master has 0001–0005). If another migration lands first, renumber at rebase, before merge.
- Unsent work (Outbox, drafts, pending Journal actions) lives outside the SQLite cache. Nothing here may move it into the cache, and the #342 migration must never lose it.
- Logic in Rust; Qt and QML only for UI and platform APIs. `unsafe` only in `qt/`. Lomiri/Suru APIs only in `qml/platform/ut/`.
- Files around 300 lines, one concept per module, pure planners and thin drivers. New Graph code goes in new modules.
- User-facing text: `qsTr()` in QML, gettext in Rust, domain `envio.aps`. Run `cargo make pot` after string changes (don't commit date-only pot refreshes).
- No swipe actions on list rows.
- Tests use `example.org` addresses only. **The Outlook test account's address never appears in commits, fixtures, PR bodies or issues**; recorded fixtures are redacted (T2). Never ask for, store or log its password; the owner types it.
- Tokens and message bodies are never logged. Graph errors are logged with status and Graph error code only.
- `cargo make ci` passes at every commit; QML changes also pass `cargo make qml-test`.
- Single-crate integration tests in `crates/<crate>/tests/`; cross-crate in `envio-e2e`.

## Decisions

Taken on the owner's behalf after reading the code; the owner may override.

1. **HTTP client: hand-written HTTP/1.1.** envio-net already speaks hand-written HTTP/1.0 (`http.rs`, `http/chunked.rs`, `oauth/client.rs`, `autoconfig/fetch.rs`) on `tls::connect`. Graph needs methods, headers, chunked replies and keep-alive, which is a small step from that code. No new crate, so `cargo audit` and the Click size are unaffected. The existing HTTP/1.0 users stay as they are.
2. **`Backend` shape differs from the spec's `Imap(ImapSession, SmtpSession)`.** Core holds no SMTP session today (`connect_smtp` runs per send, `conn.rs`). So `Conn::Ready` holds `Backend::{Imap(ImapSession), Graph(GraphServer)}`, and `Step::Send` on IMAP keeps connecting SMTP per send. Spec intent (one enum, generic steps, Graph send through the same GraphServer) is unchanged.
3. **`take_changes`** is an inherent `ImapSession` method today (`imap/idle.rs:43`). It becomes a `MailServer` trait method with default `false`; `ImapSession` forwards.
4. **Rebuild** is `Worker.store.rebuild()` (`worker/store.rs`), called from `run/mod.rs::rebuild` (`:291`). The "close Graph conns first" rule goes in `run/mod.rs::rebuild` before that call.
5. **`BACKGROUND_POLL_MS = 5 * 60 * 1000`** is a new constant in `worker/plan.rs` next to `POLL_MS`.
6. **`big(caps, size)`** lives in `envio-sync/src/driver/body.rs` beside `is_big` and replaces `is_big` at `driver/body.rs:59,71`, `envio-core/src/worker/read.rs:472` and `worker/run/sync.rs:166`. `is_big` stays as `big` with IMAP caps' behaviour (`!caps.no_partial && size > PARTIAL_ABOVE`); on `no_partial` it returns false (bodies always download whole). Prefetch on Graph uses a separate `PREFETCH_MAX_BYTES = 10 MiB` skip.
7. **Fixture recording** (spec's "plan task 1") is a dev-only example binary in envio-net (`examples/graph_record.rs`), not a feature of the app. It runs the existing PKCE sign-in with Graph scopes; the owner opens the printed URL and pastes the redirect URL back, as in the spike.
8. **Wizard step numbering.** The app shows no step counter today; the mockups show "STEP 1" and "2 of 3 / 2 of 4". The provider step adds the counter as the mockups show, for every wizard path.
9. **#384 (MX lookup) is its own lane** (separate issue, separate plan). Here, "Other" goes to today's auto-detect (`EmailStep`). #221's app-password help screens are in T17, since the provider step is where they live.
10. **uid exhaustion resets the folder** (implements spec "When either range runs out, the folder is reset"). When `allocate` returns `Exhausted` or `allocate_low` returns `None`, GraphServer resets the folder (new generation, clean resync), which the planner already handles as a uid_validity change.
11. **`BackendKind` is defined once, in envio-store** (`accounts.rs`, `pub enum BackendKind { Imap, Graph }`), and core and sync import it. Core's connection enum stays `Backend` (T1). The account and token data model (T15, T16) ships before core wiring (P5 below), because core wiring reads it.
12. **The Graph test mock lives in envio-net `src/graph/mock.rs` behind `feature = "test-mock"`** from T4 on, so envio-sync and envio-core tests use the same mock through a dev-dependency with that feature. No cross-PR move.
13. **Message-ID search stays inside one folder.** A search over `/me/messages` for a draft's Message-ID also finds the Sent copy, and the drafts driver deletes its hits (`driver/drafts.rs:230-238`). So `find_message_id` always takes a folder, and `search_message_id_deep` equals `search_message_id` on Graph.

## PRs and order

| PR | Tasks | Tier | Depends on |
|---|---|---|---|
| P1 Backend refactor | T1 | trunk | — |
| P2 Graph client | T2, T3, T4, T5 | trunk | — (T2 needs the owner) |
| P3 Store map | T6 | trunk | — |
| P4 GraphServer | T7, T8, T9 | trunk | P2, P3 |
| P5 Accounts and tokens data model | T15, T16 | trunk | P1 (both touch `conn.rs`) |
| P6 Core wiring | T10, T11, T12, T13, T14 | trunk | P1, P4, P5 |
| P7 Wizard provider step | T17 | trunk | P6 |
| P8 Move existing accounts | T18, T19 | trunk | P7 |
| P9 Manual gate | T20 | — | P8 |

P1, P2 and P3 run in parallel (max 3 PRs in flight); P5 takes the first free lane after P1 merges. Each PR follows shipping-a-pr.

---

### T1: `Backend` enum and generic step functions (core) — P1

**Tier:** trunk (envio-core, envio-sync, envio-net). **Testing:** standard; no behaviour change, so the existing suites are the proof.

**Files:** `crates/envio-core/src/worker/conn.rs` (`Conn`, `Worker::imap` → `Worker::backend`, the IDLE return path at about `:410-426`); `worker/open.rs` (`Job.have`, `Opened.session`, `Conn::Ready(session)` at `:186`, `:198`, the test's `worker.imap(&account)` at `:309`); new `crates/envio-core/src/worker/backend.rs`; `worker/run/{mod,sync,mail,prefetch}.rs`; `worker/older.rs`; `crates/envio-sync/src/server.rs` (`take_changes` on the trait); `crates/envio-net/src/imap/idle.rs` (stays inherent; the trait impl forwards).

**Signatures:**
- `pub(crate) enum Backend { Imap(ImapSession) }` in `backend.rs`; the `Graph` variant comes in T11. It implements `MailServer` by forwarding (a `match self` per method; keep it in `backend.rs`).
- `Backend::caps_static(&self) -> Caps` is not added yet (T11).
- `Conn::Ready(Backend)` replaces `Conn::Ready(ImapSession)`.
- `Worker::backend(&mut self, account: &str) -> Result<Backend, Problem>` replaces `Worker::imap`. IMAP behaviour is unchanged.
- Every step function that takes `session: &mut ImapSession` (`run/sync.rs` folders/folder/search_server/body/keywords/part/replay, `run/mail.rs` file/drafts, `run/prefetch.rs::run`, `Worker::older_step`, `run/mod.rs::changed_folder`) takes `server: &mut impl MailServer` instead. They get no other change.
- `MailServer::take_changes(&mut self) -> bool { false }` as a default; `impl MailServer for ImapSession` forwards to the inherent method.
- `prepare_idle` keeps taking `&mut ImapSession` (IDLE is IMAP-only) and is reached through `if let Backend::Imap(s)`. `begin_open` takes no session: it takes the session out of `live.conn` and matches `Conn::Ready(Backend::Imap(session))`; a `Conn::Ready(Backend::Graph(_))` is put back untouched, never dropped (that arm comes with T11; in T1 the match has one variant).
- `Backend` carries `#[allow(clippy::large_enum_variant)]`, as `Conn` does.

**Invariants:**
- No behaviour change: same steps, same order, same errors and the same IDLE handling.
- `Step::Send` still returns early through `mail::send`. Remove the unreachable `Step::Send => Ok(())` arm only if the compiler allows it without a catch-all.

**Tests:** existing `cargo make ci` (envio-core, envio-sync, envio-e2e) passes unchanged. Add one unit test in `backend.rs` showing `Backend::Imap` forwards `caps()` and `take_changes()`. CI's e2e-it lane runs if `cargo xtask affected` selects it.

---

### T2: Record Graph fixtures on the test account (net, dev tool) — P2

**Needs the owner:** they sign in to the Outlook test account. Ask first (`needs input`, `~/.claude/hooks/notify.sh`), when T3 is ready to use the fixtures.

**Files:** new `crates/envio-net/examples/graph_record.rs`; new `crates/envio-net/tests/fixtures/graph/*.json`; new `crates/envio-net/tests/fixtures/graph/README.md` (what each file is, and the date).

**Behaviour:**
- Prints the authorize URL: `Provider::microsoft_template().with_client_id(<id from env ENVIO_MS_CLIENT_ID>)` with `scopes` overridden to the Graph scopes (T16's `microsoft_graph()` does not exist yet; a comment points to T16, and T16 switches the example to it), and the PKCE flow in `oauth/`. The consumer tid constant `CONSUMER_TENANT` is defined in T3 (`oauth/token.rs`), not T16, so T2's redactor and T16 share it. Reads the pasted redirect URL from stdin, and exchanges the code. The token stays in memory only.
- Records raw replies for: `folders` (with `childFolders`), one `delta` first page plus a `@odata.nextLink` page plus the final `@odata.deltaLink` page, a delta after marking one message read, a delta with a removal, `$batch` with 3 ids including one deleted id (sub-404), `$search` page 1 and its `nextLink`, `find_message_id` (`$filter=internetMessageId eq '<…>'`) on Inbox and on Sent, one 404 error body, one 400 error body, a 410 or `syncStateNotFound` body if it can be provoked with a stale link, and the id_token claims (`tid`, no signature).
- Redacts with an **allow-list** before writing:
  - `internetMessageHeaders`: keep only From, To, Cc, Subject, Date, Message-ID, In-Reply-To, References, MIME-Version, Content-Type; drop all others (Received, `X-MS-Exchange-*`, Authentication-Results, …).
  - Email addresses → `alice@example.org` / `bob@example.org` (stable per address); display names → "Alice"/"Bob"; subjects → "Subject N"; bodies and previews dropped.
  - Message-IDs and `internetMessageId` → `<fake-N@example.org>`; `changeKey`, `conversationId`, `parentFolderId` and every id → stable fake ids (`AAMk-1`, …), consistent across files.
  - Folder display names: well-known folders keep their English name, all others → "Folder N".
  - The id_token: store only `{"tid": "<tid>"}`, and only if it is the consumer tid; otherwise a fake GUID.
  - `@odata` links → `https://graph.microsoft.com/v1.0/...` with fake tokens.
- A final check refuses to write any file that still contains: the real address or its local part, the account's display name, `@outlook.`, `@hotmail.`, `@live.`, any GUID-shaped string other than the consumer tid, or any IPv4/IPv6 literal.

**Result recorded in the README:** whether `$batch` worked (if not, T5 uses the per-id GET fallback) and whether `find_message_id` matched on a folder with angle brackets.

**Tests:** none (dev tool); `cargo build --examples -p envio-net` in CI via clippy.

---

### T3: Graph JSON parsers (net) — P2

**Tier:** trunk (envio-net). **Testing:** unit, on T2's fixtures plus hand-written edge cases.

**Files:** new `crates/envio-net/src/graph/mod.rs`, `graph/model.rs` (types), `graph/parse.rs` (pure parsers), `graph/error.rs` (status → `NetError`); `lib.rs` (`pub mod graph`).

**Types (`model.rs`):**
- `pub struct GFolder { pub id: String, pub parent_id: Option<String>, pub display_name: String, pub well_known: Option<String>, pub hidden: bool, pub class: Option<String>, pub child_count: u32 }`
- `pub struct GMeta { pub id: String, pub received_ms: i64, pub is_read: bool, pub flagged: bool, pub categories: Vec<String>, pub is_draft: bool }`
- `pub enum DeltaPage { Page { changed: Vec<GMeta>, removed: Vec<String>, next: DeltaNext }, Expired }`; `pub enum DeltaNext { More(String), Done(String) }` (nextLink vs deltaLink).
- `pub struct GSummary { pub id: String, pub received_ms: i64, pub size: u32, pub headers: Vec<(String, String)> }`
- `pub struct GraphError { pub status: u16, pub code: String, pub retry_after_s: Option<u64> }`

**Parsers (`parse.rs`), all `fn(&[u8]) -> Result<T, NetError>` with `Protocol` on bad JSON:**
- `folders(body) -> (Vec<GFolder>, Option<String> /*nextLink*/)`
- `delta(body) -> DeltaPage` (an item with `@removed` goes to `removed`)
- `batch(body) -> Vec<(String /*request id*/, u16, Option<u64> /*Retry-After*/, Vec<u8> /*sub-body*/)>` matched by `id`, not order
- `summary(body) -> GSummary` (size from `singleValueExtendedProperties` `Integer 0x0E08`; missing → 0)
- `search(body) -> (Vec<GMeta>, Option<String>)`
- `error(status, body, retry_after) -> GraphError`
- `pub const CONSUMER_TENANT: &str = "9188040d-6c67-4c5b-b112-36a304b66dad";` in `oauth/token.rs`.
- `oauth::token::Identity` gains `pub tid: Option<String>`. `identity_of` (`oauth/token.rs:131-141`) today returns `None` unless `email` or `preferred_username` is present: change it to read and clean `tid` too and return `Some` when any of the three is present. `identity_matches` and core's `judge` handle an `Identity` with no email (match on `tid` plus the typed address, as the spec says). Existing `Identity { .. }` literals in tests gain `tid: None`.

**`error.rs`:** `pub fn to_net(e: &GraphError, op: Op) -> Mapped` with `pub enum Op { Read, Write, Delta, Send, Create }` and `pub enum Mapped { Err(NetError), Gone, Expired, Retry401 }`. It implements the spec's status table exactly, including `send_mime`'s confirmed-not-sent set (`Op::Send`: 400, 401, 403, 409, 413, 423, 429, 503, 507 map as in the table; every other status is `Mapped::Err(NetError::Unconfirmed)`). T3 adds the `NetError::Unconfirmed(String)` variant (described under T4's `NetError` additions) with its stub arms, so T3 compiles on its own; T4 adds `Storage`. 403 on `Op::Send` with a send-limit error code (`ErrorExceededMessageLimit`, `SendAsDenied` is *not* one) → `Busy`.

**Tests (first, failing):**
- Each fixture parses; ids, flags, categories, `isDraft`, the size property and headers come through.
- Delta: a nextLink page gives `More`, the last gives `Done`; a removal lands in `removed`; a 410 body and a `syncStateNotFound` code both give `Expired`.
- Batch: responses out of order are matched by id; a sub-404 is visible; a sub-429's `Retry-After` header is read.
- Status table: one test per row of the spec table, and per `Op::Send` status in and out of the confirmed set (500, 502, 504 → unconfirmed).
- `identity_of` returns `tid` from a token with no `email` and no `preferred_username`.

---

### T4: HTTP/1.1 client with keep-alive (net) — P2

**Tier:** trunk. **Testing:** unit plus the mock TLS server (Decision 12) in `crates/envio-net/tests/graph_http.rs`.

**Files:** new `crates/envio-net/src/graph/http.rs` (connection, request writing, on `tls::connect`); new `graph/reply.rs` (an incremental per-reply reader: status line, headers, then exactly Content-Length bytes or chunked framing, leaving the connection at the next reply; the existing `http/chunked.rs` decodes a fully buffered body read to EOF, so this reader is new, reusing only chunk-size parsing made `pub(crate)`); new `graph/mock.rs` behind `feature = "test-mock"` (a TLS server on 127.0.0.1 with rcgen certs that serves scripted replies and counts accepts and requests; `rcgen` and tokio `rt-multi-thread`/`sync` become optional deps enabled by the feature); `Cargo.toml` (`[features] test-mock = ["dep:rcgen", "tokio/rt-multi-thread", "tokio/sync"]`; `rcgen` moves from dev-dependencies to an optional dependency; `[[test]] name = "graph_http"` and `name = "graph_client"` with `required-features = ["test-mock"]`, and `cargo make ci` runs envio-net tests with `--features test-mock`). The mock returns its own `Roots` trusting its rcgen CA (`mock::Server::roots()`), since `src` cannot use `tests/support`; `crates/envio-net/src/error.rs` (new variants).

**Signatures:**
- `pub struct Request<'a> { pub method: &'static str, pub path: &'a str, pub headers: Vec<(&'static str, String)>, pub body: Option<(&'static str /*content-type*/, Vec<u8>)> }`
- `pub struct Response { pub status: u16, pub headers: Vec<(String, String)>, pub body: Vec<u8> }`
- `pub struct Http { endpoint: Endpoint, roots: Roots, conn: Option<TlsStream<TcpStream>> }` with `Http::new(host: &str, port: u16, roots: Roots)` (builds an `Endpoint` with TLS security and no pin, for `tls::connect`), `async fn send(&mut self, req: &Request<'_>, fresh: bool, idempotent: bool) -> Result<Response, Sent>`.
- `pub enum Sent { Before(NetError) /*nothing written or no byte read on a reused conn*/, After(NetError) /*body written, reply lost*/ }`.
- Every request sends `Prefer: IdType="ImmutableId"` (more preferences comma-joined in one header), `Host`, `Authorization: Bearer …` (added by T5), `Connection: keep-alive`, `Accept: application/json`.
- Reply size cap: 64 MiB for `$value`, 4 MiB otherwise. Over the cap: `$value` → `Refused` (so one huge mail does not loop through `give_up`'s `Protocol` handling), others → `Protocol`.
- TLS errors: `Untrusted` and `PinChanged` from `tls::connect` become `NetError::Tls(reason)` (spec: no accept action on Graph; core shows `Problem::Certificate(None)`). Timeouts: `COMMAND_TIMEOUT` per read; a send with a body uses a timeout scaled to the body size (≥ 60 s).

**`NetError` additions** (both are new variants, `Unconfirmed` added in T3 and `Storage` here; every exhaustive match gets an arm so each commit compiles — `sync::error::net_kind`, `problem::from_net`, `cause::net_line`, `is_temporary`. Here they are stubs: `Storage` → `ErrorKind::Local` / `Problem::Local`, `Unconfirmed` → `Temporary`; T10 replaces them):
- `Storage { kind: StorageKind, msg: String }` with `pub enum StorageKind { Damaged, DiskFull, Other }` (spec).
- `Unconfirmed(String)`: a Graph send whose outcome is unknown. `is_temporary` false. `net_kind` → `ErrorKind::Temporary`; the send driver treats it as "may have been sent" (T9).

**Invariants:**
- `fresh = true` always opens a new connection (used by `send_mime`, `create_mime`).
- On a reused connection, a failure before any reply byte with `idempotent = true` retries once on a fresh connection. With `idempotent = false` it returns `Sent::Before` (caller maps to `Busy`).
- A failure after the body was fully written and before a complete reply is `Sent::After`.
- `Connection: close` in a reply drops the connection after reading it.

**Tests:**
- GET over keep-alive reuses one TCP connection for two requests (mock counts accepts).
- The mock closes the idle connection; the next idempotent GET retries once on a fresh connection and succeeds; a non-idempotent POST returns `Sent::Before`.
- `fresh = true` opens a new connection even when one is idle.
- The mock reads the full body and then drops the connection: `Sent::After`.
- Chunked and Content-Length replies, two in a row on one connection; a JSON reply over the cap is `Protocol`, a `$value` over the cap is `Refused`.
- A server certificate the roots don't trust → `NetError::Tls`.
- The `Prefer` header carries ImmutableId on every request.

---

### T5: `GraphClient` calls and `Bearer` (net) — P2

**Tier:** trunk. **Testing:** `crates/envio-net/tests/graph_client.rs` on T4's `test-mock` server, serving T2 fixtures.

**Files:** new `crates/envio-net/src/graph/client.rs`, `graph/batch.rs`, `graph/bearer.rs`; `graph/mod.rs` exports.

**Signatures:**
- `pub trait Bearer { fn bearer(&self, force: bool) -> LocalBoxFuture<'_, Result<SecretString>>; }` (`futures::future::LocalBoxFuture`; non-Send, like core's `Refresher`).
- `pub struct GraphClient { http: Http, bearer: Box<dyn Bearer> }`, `GraphClient::new(roots: Roots, bearer: Box<dyn Bearer>)`; host `graph.microsoft.com`, port 443, base `/v1.0`. `#[doc(hidden)] pub fn with_host(host: &str, port: u16, roots: Roots, bearer: Box<dyn Bearer>)` for tests.
- Calls (all `async`, `Result<_>` = `envio_net::Result`):
  - `folders(&mut self) -> Result<Vec<GFolder>>`: `/me/mailFolders?includeHiddenFolders=true&$top=100`, then each folder's `childFolders` recursively, following `nextLink`.
  - `delta(&mut self, folder_id: &str, link: Option<&str>) -> Result<DeltaPage>`: first call `/me/mailFolders/{id}/messages/delta?$select=receivedDateTime,isRead,flag,categories,isDraft` with `Prefer: odata.maxpagesize=200`; later calls GET the given link as is.
  - `summaries(&mut self, ids: &[String]) -> Result<Vec<GSummary>>`: chunks of 20 into `POST /$batch` (idempotent read). Sub-status rules as the spec: sub-404 drops that id; any sub-429/503 → whole call `Busy` (results discarded); sub-401 → the 401 rule for the whole call; other → that status's error. If T2 found `$batch` unusable, this is one GET per id instead (same signature).
  - `message_mime(&mut self, id: &str) -> Result<Option<Vec<u8>>>`: `/me/messages/{id}/$value`; 404 → `None`.
  - `get_categories(&mut self, id: &str) -> Result<Option<Vec<String>>>`.
  - `patch(&mut self, id: &str, change: &Patch) -> Result<()>` with `pub struct Patch { pub read: Option<bool>, pub flagged: Option<bool>, pub categories: Option<Vec<String>> }`; 404 → `Ok(())`.
  - `move_to(&mut self, id: &str, folder_id: &str) -> Result<Option<String>>` (the new immutable id is the same; return it for checking); no keep-alive retry; 404 → `Ok(None)`.
  - `permanent_delete(&mut self, id: &str) -> Result<()>`: `POST /me/messages/{id}/permanentDelete`; 404 → `Ok(())`.
  - `create_mime(&mut self, folder_id: &str, eml: &[u8]) -> Result<String>`: base64 `text/plain` body, `fresh = true`, no in-call status retry.
  - `send_mime(&mut self, eml: &[u8]) -> Result<()>`: `POST /me/sendMail`, base64 `text/plain`, `fresh = true`. Errors per the spec's stricter list; `Sent::After` → `NetError::Unconfirmed`.
  - `search(&mut self, folder_id: &str, text: &str) -> Result<Vec<GMeta>>`: `$search="…"` (quotes escaped), all pages.
  - `find_message_id(&mut self, folder_id: &str, mid: &str) -> Result<Vec<GMeta>>`: `/me/mailFolders/{id}/messages?$filter=internetMessageId eq '<…>'` with `'` doubled (Decision 13: always one folder).

**401 rule:** on a 401, call `bearer(true)` once and repeat the request; a second 401 → `SignIn`. For `send_mime` and `create_mime` the repeat is safe because a 401 means not accepted.

**In-call retry:** 429/500/502/503/504 with `Retry-After` ≤ 10 s are retried once after sleeping, except: `send_mime` retries only 429 and 503; `create_mime` never retries a status. Over 10 s or no header → `Busy`.

**Tests:**
- 401 then a fresh token: the second request carries the new token; two 401s → `SignIn`; `bearer(true)` called exactly once.
- Delta paging through fixtures; 410 → `Expired`.
- `$batch` with a sub-429 → `Busy`; with a sub-404 → that id missing; out-of-order replies matched.
- `send_mime` on 504 → `Unconfirmed`, not retried (mock counts requests = 1); on 503 with `Retry-After: 1` → retried once; on a connection drop after the body → `Unconfirmed`; always a fresh connection.
- `create_mime` on 503 with `Retry-After: 1` → not retried, `Busy`.
- `move_to` on a dead reused connection → `Busy`, no retry.
- `find_message_id` builds the filter with the angle brackets and a doubled `'`.

---

### T6: Migration 0006 and `graph_map` (store) — P3

**Tier:** trunk (a migration). **Testing:** unit plus `crates/envio-store/tests/graph_map.rs` on a temp-file DB.

**Files:** new `crates/envio-store/migrations/0006_graph_map.sql` (the spec's two tables with `STRICT`, like every existing table, plus `CREATE UNIQUE INDEX graph_folders_graph_id ON graph_folders(graph_id)`); `src/migrate.rs` (entry and the previous-schema test); new `src/graph_map.rs` (private shared functions), new `src/graph_map/open.rs` (`GraphMap`), `src/lib.rs` exports.

**Signatures:**
- Private, taking `&rusqlite::Connection`, opening no transaction: `reset(conn, folder_row_id) -> Result<u32 /*generation*/>`, `upsert_folder(conn, folder_row_id, graph_id) -> Result<Upserted>` with `pub enum Upserted { New(u32), Same, Changed(u32) }` (Changed resets), `folder(conn, folder_row_id) -> Result<Option<GraphFolder>>`, `allocate(conn, folder_row_id, ids: &[(String, i64)]) -> Result<Allocation>`, `allocate_low(conn, folder_row_id, id: &str, received_ms: i64) -> Result<Option<u32>>` (`None` = low range exhausted), `apply(conn, folder_row_id, changes: &[MetaChange], removed: &[String]) -> Result<()>`, `commit(conn, folder_row_id, delta_link: &str, modseq: u64) -> Result<()>`.
- `pub struct GraphFolder { pub graph_id: String, pub generation: u32, pub uid_next: u32, pub low_next: u32, pub modseq: u64, pub delta_link: String }`
- `pub enum Allocation { Done(Vec<(String, u32)>), Exhausted }` (high range; `Exhausted` when `uid_next` would reach `2^32 - 1`).
- `pub struct MetaChange { pub graph_id: String, pub flags: u8, pub categories: Vec<String> }`. `apply` computes one new modseq per call and gives it **only to rows whose flags or categories actually changed**; unchanged rows keep theirs (a first delta page lists every message, and `fetch_flags(changed_since)` must not return the whole folder). A `removed` id with no row (a message we moved out) is ignored. GraphServer allocates before it applies, so every `MetaChange` id is mapped.
- `pub struct MapRow { pub uid: u32, pub graph_id: String, pub received_ms: i64, pub flags: u8, pub categories: Vec<String>, pub modseq: u64 }`
- `pub struct GraphMap { conn: rusqlite::Connection }`: `open(path: &Path) -> Result<Self>` (WAL, `foreign_keys=ON`, 5 s busy_timeout, no migrations; `user_version` < 6 → a new `StoreError::SchemaTooOld`; `Db::open` always migrates first, so this only guards misuse).
- `GraphMap::folder_row_of(&self, graph_id: &str) -> Result<Option<FolderId>>` (Graph folder ids are immutable and global; `graph_folders.graph_id` gets a UNIQUE index in 0006). Methods, each in its own transaction: `folder`, `reset`, `allocate`, `allocate_low`, `apply`, `rows_since(folder_row_id, received_after_ms) -> Vec<MapRow>`, `rows_changed(folder_row_id, modseq) -> Vec<MapRow>`, `uid_of(folder_row_id, graph_id) -> Option<u32>`, `ids_of(folder_row_id, uids: &[u32]) -> Vec<(u32, String)>`, `row(folder_row_id, uid) -> Option<MapRow>`.
- `Db` methods for in-step use: `graph_reset(&self, folder_row_id) -> Result<u32>`, `graph_upsert_folder(&self, folder_row_id, graph_id) -> Result<Upserted>`, `graph_commit(&self, folder_row_id, delta_link, modseq) -> Result<()>`.
- `FolderId` → `folder_row_id` is `FolderId`'s inner `i64` (use the type, not a raw `i64`, in public signatures).

**Invariants:**
- `reset`: new generation non-zero and ≠ the old one (random `u32`, retry on 0 or equal); deletes the folder's message rows; `uid_next = 2^31`, `low_next = 2^31 - 1`, `modseq = 0`, `delta_link = ''`.
- `allocate` sorts by `received_ms` ascending and writes every row before returning uids; an id already mapped keeps its uid and is not re-allocated; uids are never reused.
- `allocate_low` counts down and returns `None` (not `Err`) when `low_next` would reach 0; an id already mapped returns its existing uid. GraphServer turns `None` (and `Allocation::Exhausted`) into a folder `reset` (Decision 10).
- Deleting the folder row cascades both tables.

**Tests:**
- Migration from the 0005 schema with rows: rows survive, `user_version` 6, tables exist.
- `reset` twice gives two different non-zero generations and an empty map.
- `allocate` is ascending by received time, idempotent per graph_id, and survives a reopen (crash safety).
- `allocate` near `2^32 - 1` → `Exhausted`; `allocate_low` near 0 → exhausted.
- `upsert_folder`: new → `New(gen)`; same id → `Same`; changed id → `Changed(gen')` with rows wiped.
- `GraphMap` and `Db` on the same file: a `graph_reset` through `Db` inside `db.step` is seen by the next `GraphMap::folder` (no cache).
- Cascade on folder delete.
- `apply` with one changed and one unchanged row: only the changed row's modseq moves; `rows_changed(old)` returns one row.
- `folder_row_of` finds a folder by graph_id after a reopen.

---

### T7: `GraphServer` folders, refresh, select and the trait hooks (sync) — P4

**Tier:** trunk. **Testing:** `crates/envio-sync/tests/graph_server.rs` on envio-net's `test-mock` server (dev-dependency `envio-net = { path = …, features = ["test-mock"] }`) and a temp-file DB.

**Files:** new `crates/envio-sync/src/graph/{mod,folders,refresh,map}.rs`; `src/server.rs` (trait hooks); `src/driver/folders.rs` (call sites); envio-net `imap/mod.rs` (`Caps.no_partial`).

**Trait additions** (defaults are no-ops; IMAP and `FakeServer` unchanged):
- `async fn refresh(&mut self, path: &str) -> Result<()> { Ok(()) }`
- `fn commit_folders(&mut self, db: &Db, account: &str) -> Result<()> { Ok(()) }`
- `fn commit_folder(&mut self, db: &Db, id: FolderId) -> Result<()> { Ok(()) }`
- These return `envio_net::Result`; store errors convert through `NetError::Storage { kind, msg }` built in envio-sync (`fn storage(e: &StoreError) -> NetError`: `is_damage` → Damaged, `is_disk_full` → DiskFull, else Other).
- `Caps { .., pub no_partial: bool }` (default false).

**Call sites:**
- `sync_folders` (`driver/folders.rs:22`): inside the existing `local.db.step`, after the upserts and removes, `server.commit_folders(db, account)`.
- `sync_folder_as` (`:126`): `server.refresh(&folder.path).await?` before `select`.
- The step at `:228` (`set_sync_state(id, &plan.finish)`): `server.commit_folder(db, id)` in the same closure.

**`GraphServer`:**
- `pub struct GraphServer { client: GraphClient, map: GraphMap, paths: BTreeMap<String, (String /*graph_id*/, Option<FolderId>)>, pending: BTreeMap<FolderId, Pending> }`; `GraphServer::new(client: GraphClient, map: GraphMap)`.
- `list_folders`: `client.folders()`, filter hidden and non-mail (`class` absent or `IPF.Note`), build paths per the spec (Inbox → `INBOX`, children `INBOX/<name>`, `/` → U+2215 and `.` → U+2024 inside a segment), roles from `wellKnownName`. Fills `paths`.
- Every lookup of a path's `FolderId` (in `refresh`, `select`, the message calls) that finds `None` fills it from `map.folder_row_of(graph_id)`, so a fresh `GraphServer` after a reconnect works for `Step::Body`/`Step::File` with no `Step::Folders` first. If `paths` is empty, `select`/`refresh` first do one `client.folders()` walk.
- `refresh(path)`: the spec's six steps. Unknown path after the walk → `Refused`. A path whose graph_id has no `graph_folders` row yet (created since the last commit) → `NetError::Busy("folder not listed yet")`, not `Refused`, so it never reaches `give_up`; the next `Step::Folders` commits it. Test: that case returns `Busy` and the step is retried, not given up.
- `select(path)`: no network; generation as `uid_validity`, `uid_next`, `exists` (count of high+low rows), `highest_modseq = Some(modseq)`.
- `commit_folders(db, account)`: for each listed path, look up the row id through `db.folders(account)`, `db.graph_upsert_folder`; `Changed` drops `pending` for it.
- `commit_folder(db, id)`: `db.graph_commit(id, link, modseq)` from `pending`, then clear it.
- Caps: the spec's list (`condstore`, `r#move`, `special_use`, `no_partial` on; the rest off). `pub const GRAPH_CAPS: Caps`.

**Tests:**
- Folder paths: Inbox, `INBOX/child`, a name with `/` and `.`, hidden and a contacts folder filtered out.
- A multi-call first sync (`sync_folder_as` twice with `more`) drains delta once (mock counts delta requests).
- Crash replay: drain, drop the server before `commit_folder`, rebuild, drain the same delta again → same uids, no duplicates.
- 410 on refresh → new generation and empty map; `select` reports the new `uid_validity`.
- A changed graph_id for a path in `commit_folders` → reset.
- Newest-first: high uids ascend with `receivedDateTime`.
- After a reconnect: a new `GraphServer` on the same DB, no `list_folders`/`commit_folders` call, `select("INBOX")` then `fetch_body(uid)` succeed.

---

### T8: `GraphServer` messages, flags, search, drafts (sync) — P4

**Tier:** trunk. **Testing:** `tests/graph_server.rs` (continued).

**Files:** new `crates/envio-sync/src/graph/{messages,flags,search}.rs`.

**Behaviour** (the spec's "Messages" section, item by item):
- `search_since(unix_secs)` / `search_all` from `GraphMap::rows_since`.
- `fetch_summaries(uids)`: `ids_of` → `client.summaries`; `Summary.header` rebuilt from `internetMessageHeaders` as `Name: value\r\n` lines plus a final `\r\n`; `internal_date` from `receivedDateTime`; `size` from 0x0E08; flags from the map row; `modseq` from the row; gone messages left out.
- `fetch_flags(uids, changed_since)`: `rows_changed`.
- `fetch_body(uid, _)`: `message_mime`. `fetch_partial` / `fetch_section` return `Err(Protocol("not on Graph"))` (never reached with `no_partial`).
- Flags mapping and categories read-merge-write exactly as the spec; `\Answered` dropped on write and never reported; `\Draft` read-only (a write is ignored).
- `store_flags`, `move_to(uids, path)`, `delete(uids)` resolve through the map and `paths`; delete is `permanent_delete`. After `move_to`, the moved rows are removed from the source folder's map (the target maps them at its next refresh).
- `append(folder, flags, eml)`: only for the Drafts role → `create_mime(drafts_id, eml)`; other folders → `Refused` (Sent filing is never needed on Graph; T11 sets `files_itself`).
- `search_message_id`, `search_message_id_deep`, `search_text`: server filter/search **in the last selected folder only** (Decision 13; with no folder selected → `Refused`, a driver bug); hits without a mapping get `allocate_low`.

**Tests:**
- Summaries rebuild headers that `envio_mime` parses back (From, Subject, Message-ID, Date).
- Categories: the server has `[A, B]`, we add `C` → PATCH `[A, B, C]`; remove `B` → `[A, C]` (mock records bodies).
- `\Answered`: `store_flags(add, [\Answered])` sends no PATCH and replay of the same action does not loop (run replay twice; one plan, no repeat).
- Search hits get low uids; a later refresh does not re-map them; `select().uid_next` is not moved by low allocations.
- `delete` on a gone message → `Ok`.
- A Sent copy with the same Message-ID as a draft survives the drafts driver deleting the draft; `search_message_id_deep` with Drafts selected queries only Drafts (mock records the path).
- The spec's "newmail ignoring low uids": a folder with one new high-uid row and one new low-uid row gives a new-mail count of 1 (use the sync crate's newmail count).

---

### T9: `GraphServer` send with Bcc (sync) — P4

**Tier:** trunk (a send path). **Testing:** unit for the pure Bcc function, mock for the send.

**Files:** new `crates/envio-sync/src/graph/send.rs`; new pure `crates/envio-sync/src/plan/bcc.rs`.

**Signatures:**
- `pub fn with_bcc(eml: &[u8], envelope: &[String]) -> Vec<u8>` (pure): Bcc = envelope recipients whose bare address (case-insensitive) is not in To or Cc; drops any existing `Bcc:` header; appends one folded `Bcc:` at the end of the header block; empty Bcc list → eml unchanged.
- `impl MailSender for GraphServer`: `send(from, to, eml)` → `client.send_mime(&with_bcc(eml, to))`. `NetError::Unconfirmed` → `SendError { error, unconfirmed: true }` (the existing struct at `smtp/send.rs:27`, which the Outbox driver already treats as may-have-been-sent); every other error → `unconfirmed: false`. No change to `driver/outbox.rs`.

**Tests (full code here is warranted; this is a never-resend path):**
- `with_bcc`: To/Cc/Bcc mixes; case-insensitive match; display-name forms (`"Bob" <bob@example.org>`); an existing Bcc replaced; folded long list; headers-only eml; the Outbox eml bytes are unchanged (the function takes `&[u8]` and the test compares).
- Send on 504 → item `MayHaveBeenSent`, never requeued; on 400 → `Failed`; on 503 + long Retry-After → stays `Queued` (Busy).

---

### T10: `ErrorKind::Damaged` everywhere (core, sync) — P6

**Tier:** trunk. **Files:** `crates/envio-sync/src/error.rs`, `plan/replay.rs:121`, `plan/outbox.rs:70`, `driver/account.rs:61-63`; `crates/envio-core/src/problem.rs`, `cause.rs`.

**Changes:** replaces T4's stub arms (between P4 and P6 a damaged Graph map surfaces as `Local`; harmless, no Graph account exists before P7). Exactly the spec's "Errors" list: `ErrorKind::Damaged`; `net_kind` for `Storage` and `Unconfirmed`; `SyncError::kind()` for a damaged store error → Damaged; `from_kind(Damaged)` → `Problem::Damaged`; `from_net` for `Storage` (like `from_store`) and `Unconfirmed` (→ `Temporary`; the Outbox handles it before this); Damaged is a Stop in replay and outbox planning; `cause::net_line` arms; `cause` and `from_sync` say "Outlook.com" instead of "IMAP" for a Graph account (pass the backend label in; it is a `&'static str` already used for the stage).

**Tests:** replay with a store that reports damage → `Problem::Damaged` → `Response::Rebuild` (existing rebuild test pattern); every `ErrorKind` round-trips through `from_kind`; `net_kind` for each new variant.

---

### T11: `Backend::Graph`, caps, `big`, `files_itself`, rebuild (core) — P6

**Tier:** trunk. **Files:** `worker/backend.rs`, `worker/conn.rs`, `worker/run/{mod,mail,prefetch}.rs`, `worker/read.rs`, `worker/run/sync.rs`, `crates/envio-sync/src/driver/body.rs`; new `worker/graph_bearer.rs`.

**Signatures:**
- `Backend::Graph(GraphServer)`; `MailServer` forwarding extends; `MailSender` for `Backend::Graph` goes to the GraphServer.
- `pub(crate) fn static_caps(kind: BackendKind) -> Caps`, with `envio_store::BackendKind` from `AccountSettings.backend` (T15, merged in P5). Callers of `big` take caps from here when no session is open.
- `pub fn big(caps: Caps, size: u32) -> bool` in `driver/body.rs` (Decision 6).
- `files_itself`: `run/mail.rs:65` becomes `kind == Graph || files_itself(smtp_host)`.
- `struct CoreBearer { handle: TokenHandle, last: RefCell<Option<Granted>> }` (the handle from `TokenCells::handle(account, ProviderKind::MicrosoftGraph)`, T16) implementing `envio_net::graph::Bearer` (spec "Token": problem → net error mapping; `force` with a held `Granted` → `renew`, without → `access(true)`).
- `Worker::backend` opens a `GraphServer` for a Graph account: `GraphClient::new(roots, Box::new(CoreBearer{..}))`, `GraphMap::open(&paths.db())`.
- `Step::Send` on Graph uses the same `GraphServer` from `live.conn` (take it out like `backend()` does; `send_mime` uses its own fresh HTTP connection). It is put back after every outcome, including an error and `Unconfirmed`. Test: after an unconfirmed send, `live.conn` holds the server again.
- `run/mod.rs::rebuild`: first set `live.conn = Conn::Closed` for every Graph account, then `worker.store.rebuild()`.
- Graph accounts start with `live.idle_capable = Some(false)`.

**Tests:** `big` for both caps; prefetch on Graph skips > 10 MiB; `files_itself` true for Graph; rebuild closes Graph conns and leaves IMAP ones (worker test with a fake Graph account); `CoreBearer` problem mapping and the `force` path (unit, with the existing token test clock/refresher).

---

### T12: Background poll (core) — P6

**Tier:** trunk. **Files:** `worker/plan.rs` (`Timing.poll_ms`, `next`, `BACKGROUND_POLL_MS`), `worker/turn.rs` (`account_wake`, where `Timing` is built at `:119`, the Run branch).

**Changes:** the spec's "Background poll" list verbatim. As the spec says, `poll_ms` is `Some(BACKGROUND_POLL_MS)` for every notifying account with `idle_capable == Some(false)`: Graph accounts and IMAP servers without IDLE alike (the spec calls the IMAP case intended). IMAP accounts with IDLE, or with IDLE support still unknown, keep `None`. `Timing { .., pub poll_ms: Option<i64> }`; `account_wake(.., poll_ms: Option<i64>, ..)` gains the background arm.

**Tests (pure, in `plan.rs`):** background + `poll_ms` + `now >= next_full_ms` → `Run`; before that → `Sleep { until_ms: Some(next_full_ms) }`; foreground unaffected; `poll_ms: None` unchanged. Worker test: an IMAP account without IDLE gets `poll_ms: Some(BACKGROUND_POLL_MS)`; one with IDLE gets `None` and the same `Next` as before. Worker test: after a background Run, `next_full_ms = now + BACKGROUND_POLL_MS` even when the Inbox step fails (no spin: two `wake_at` calls in a row give the same future time).

---

### T13: Resolve retries for unconfirmed Graph sends (store, sync, core) — P6

**Tier:** trunk (a never-resend path; full test code warranted).

**Files:** `crates/envio-store/src/outbox.rs` (`OutboxMeta.resolve_tries`, `resolve_after_ms`, `set_resolve`, `set_state` zeroing); `crates/envio-sync/src/driver/outbox.rs` (`file_sent(.., now_ms: i64, retries: bool)`, `Filed.retry`); `crates/envio-core/src/worker/run/mail.rs`, `worker/turn.rs` (`next_resolve`, `handled_resolve_ms`, `on_timers`).

**Signatures:**
- `pub fn set_resolve(&self, id: &str, tries: u32, after_ms: i64) -> Result<()>` (read-modify-write the meta; never touches `state`; leaves `unwritten` alone).
- `pub const RESOLVE_TRIES: u32 = 3; pub const RESOLVE_GAP_MS: i64 = 10_000;` in `plan/outbox.rs`.
- `Filed { .., pub retry: Vec<String> }`.
- `pub fn next_resolve(metas: &[OutboxMeta], after: i64) -> Option<i64>`.

**Behaviour:** the spec's "Resolve" list. `retries` is `settings.backend == BackendKind::Graph`; with `false`, a miss asks at once as today.

**Tests:** three misses 10 s apart then `ask`; an early `file` run (before `resolve_after_ms`) burns no try; tries survive an `Outbox::open` (restart); "Send again" (`set_state(Queued)`) zeroes the count; IMAP still asks on the first miss; a Graph account without Sent asks at once; old meta JSON without the fields loads (serde default).

---

### T14: "Move to Sent" gated on the backend (core, ui, qml) — P6

**Tier:** trunk (envio-core, envio-ui) plus leaf QML. **Files:** `crates/envio-core/src/api/view.rs` (`OutboxStatus::MayHaveBeenSent { asked, can_file }`), `outbox.rs` (`status(state, asked, can_file)`, `decide(.., graph: bool)`), `worker/outbox.rs:~48`, `text/outbox.rs`, `crates/envio-ui/src/rows.rs` (`OutboxRowUi.can_file`, `OUTBOX` + `"canFile"`), `envio-ui/src/convert.rs`, `qml/pages/inbox/OutboxRow.qml`, any QML test mock of the outbox model.

**Changes:** the spec's "Move to Sent" section. New status string when `!can_file`: "May have been sent: not found in Sent. Send again, or delete it?" (matches the mockup). `cargo make pot`.

**Tests:** `decide(MoveToSent, graph = true)` → `NotNow`; status text both ways; `outbox_row` fills `can_file`; QML test: "Move to Sent" hidden when `canFile` is false. Mockup: the Graph Outbox row frame in `carbon-provider.html`.

---

### T15: Accounts file v4, `backend`, optional servers (store, core) — P5

**Tier:** trunk. No Graph account can exist yet in P5 (the wizard is P7); this PR changes the data model and keeps IMAP behaviour. **Files:** `crates/envio-store/src/accounts.rs`; every `.imap` / `.smtp` use in core (`pin_problem`, `unreadable`, `stand_in`, `conn.rs`, `trust.rs`, `read.rs` sender check, `setup.rs`, `setup_thread.rs`, `oauth.rs`).

**Signatures:**
- `pub enum BackendKind { Imap, Graph }` (serde lowercase, default Imap), exported from envio-store and used by core and sync (Decision 11).
- `AccountSettings { .., #[serde(default, skip_serializing_if = "BackendKind::is_imap")] pub backend: BackendKind, pub imap: Option<Server>, pub smtp: Option<Server>, #[serde(default, skip_serializing_if = "Option::is_none")] pub migration: Option<Migration> }` with `pub struct Migration { pub step: u8, pub failures: u32 }` (T19 uses it).
- Version on save: 4 if any account has `backend: graph` or a `migration`; else today's 3/2 rule.

**Tests:** v2 and v3 files load with `backend = Imap` and `Some` servers; a v3 file saves back as v3 byte-equal; a Graph account saves as v4 with no `imap`/`smtp`; a v5 file is refused; every touched core path compiles without unwraps on `None` (Graph accounts never reach IMAP/SMTP code: `login_for` returns `Problem::Local` with a log line if they do).

---

### T16: Tokens keyed by provider kind, oauth.json v2, Graph provider (net, core) — P5

**Tier:** trunk (auth). **Files:** `crates/envio-net/src/oauth/provider.rs` (`ProviderKind::MicrosoftGraph`, `Provider::microsoft_graph()`); `crates/envio-core/src/oauth_tokens.rs`; `worker/token.rs` (`TokenCells`, `Cell.key`); `worker/remove.rs`; `worker/oauth.rs` (`judge`, `identity_of` callers).

**Signatures:**
- `OAuthTokens::{get, set, remove}(&mut self, account: &str, kind: ProviderKind, ..)`; `TokenCells::handle(account, kind)`, `forget(account)` drops both kinds, `set_paused(account, kind, problem)`.
- oauth.json v2 as the spec; written as v2 only if any account holds a Graph token; v1 loads as `imap`. A v1 file stays v1 byte-equal when no Graph token is added.
- `Provider::microsoft_graph()`: same client id, scopes `Mail.ReadWrite Mail.Send offline_access openid email profile`, `kind: MicrosoftGraph`; `imap`/`smtp` endpoints unused (keep the field type; fill with the Microsoft endpoints).
- `CONSUMER_TENANT` already exists (T3); T2's example switches to `microsoft_graph()`.
- `set_paused` from `Worker::report` and `run/mod.rs:266` passes the account's kind (`Microsoft` until P6 wires Graph).
- `Config.oauth` (core `Worker.providers`, `worker/mod.rs:141,289`) gains `Provider::microsoft_graph()` wherever `Provider::microsoft()` is added (grep `Provider::microsoft()`).

**Tests:** v1 → v2 upgrade on first Graph token; a file whose accounts hold only `imap` tokens is written as v1 (downgrade-safe); a v3 file → `Newer`; `forget` removes both; `judge` accepts a token with `tid` and no email when the typed address is known; the paused state of one kind does not pause the other.

---

### T17: Wizard "Choose a provider" step and app-password help (qml, core) — P7

**Tier:** trunk (core) plus leaf QML. **Design:** `carbon-provider.html` frames "Choose a provider", consent refused (user / admin), Outlook.com sign-in and waiting, mismatch, iCloud app password, Other. Design review of the built screens against those frames at the end of P7.

**Files:** new `qml/pages/accounts/ProviderStep.qml`, new `AppPasswordStep.qml`; `WizardPage.qml` (`step: "provider"` first, step counter per Decision 8); `worker/oauth.rs` (`start_oauth(email, account, kind)`, routing on `tid`, the one second sign-in); `worker/setup.rs` (`save_oauth_account` with `BackendKind`); `text/oauth.rs`, `text/setup.rs`; `api/command.rs` / `api/view.rs` for the new wizard events; qml tests under `qml/tests/`.

**Behaviour:** the spec's Wizard table and rules. App-password pages for iCloud, Gmail and Yahoo: the provider's settings link (opened with `Qt.openUrlExternally`), the steps as short text, then the existing `SignInStep` with the provider's known servers prefilled. "Other" goes to `EmailStep` (today's flow).

**Tests:** core: consumer `tid` → Graph account saved with no servers and a Graph token; work `tid` after the Outlook.com pick → mismatch event, one second sign-in with IMAP scopes, success saves an IMAP OAuth account; a second mismatch or different email/`tid` → refusal, nothing saved; consent refused → back to provider, nothing saved. QML: provider list items and their routes; Back from step 2 returns to the provider step.

---

### T18: "Update sign-in" for existing Outlook.com accounts (core, qml) — P8

**Tier:** trunk (core) plus leaf QML. **Design:** frames "Accounts list with Update sign-in", the explanation page, the three Update sign-in outcomes.

**Files:** new `crates/envio-core/src/migrate_graph/eligible.rs` (pure); `worker/oauth.rs` (Graph sign-in for an existing account); `api/view.rs` (`AccountRow.update_sign_in: bool`); `qml/pages/accounts/AccountsPage.qml`; new `qml/pages/accounts/UpdateSignInPage.qml`; reuse `SignInAgainPage.qml` for the IMAP re-sign-in shown in the stuck frame.

T18 reuses T17's `start_oauth(email, account, kind)` and adds no second kind parameter.

**Signatures:** `pub fn eligible(settings: &AccountSettings, identity_tid: Option<&str>) -> bool` (the spec's eligibility: consumer `tid`, or the narrower domain list with `auth == "microsoft"`; never `office365.com` or `*.onmicrosoft.com`).

**Tests:** each domain in and out of the list; a custom domain without `tid` → false; Update sign-in with a work `tid` drops the Graph token and leaves the account on IMAP; success stores the Graph token beside the IMAP one and starts T19.

---

### T19: The #342 migration state machine (core) — P8

**Tier:** trunk (user data). **Design:** frames "Moving", "Stuck", "Stuck: IMAP refuses the sign-in", "Done", "Done after Switch anyway".

**Files:** new `crates/envio-core/src/migrate_graph/{mod,plan}.rs` (pure planner: `fn next(m: &Migration, journal_empty: bool, accepted_left: bool, replay_failed: bool) -> MigrationStep`); worker driver in new `worker/migrate_graph.rs`; `crates/envio-store` (`Db::clear_account_cache(account)`: deletes the account's folder rows in one step, cascading messages and the Graph map; Outbox and drafts files live outside SQLite and are untouched). Bodies live on disk: step 2 first removes each folder's bodies with `Bodies::remove_folder` (as `sync_folders` does), then the rows. Both are idempotent, so a crash between them resumes by redoing step 2.

**Behaviour:** the spec's four migration steps, resumable from the step saved in `AccountSettings.migration`. "Switch anyway" after 3 failed replay runs drops the remaining actions with the notice in the mockup. Actions queued after step 1 ends are dropped with the same notice. Step 3 removes the IMAP token and `imap`/`smtp` entries and sets `backend = Graph`. Step 4 resets `Live` as `Live::new` does plus `idle_capable = Some(false)`, `wants.full`.

**Tests (full code warranted for the resume cases):** a crash after the bodies are removed and before the rows are cleared resumes and finishes; crash at each step (persist step, drop the worker, reopen) resumes at that step and finishes; Outbox items and drafts survive; an `Accepted` Outbox item blocks step 1 until filed; IMAP refusing sign-in waits at step 1; three failures enable Switch anyway; a pending action after step 1 is dropped with the notice.

---

### T20: Manual gate and release check — P9

Not code. On the test account, with the owner signing in (ask first; the phone is shared, ask before taking it):
- the spec's manual gate list (big-mailbox first sync timed plus a restart in the middle; Bcc to another mailbox; a Bcc-only mail; delivery above 12 MB; migrating an existing IMAP account; a background notification; time until the Sent copy appears → tune `RESOLVE_TRIES` / `RESOLVE_GAP_MS`);
- design review of every built screen against `carbon-provider.html` (screenshots via `ute2e shot`, separate agent);
- `auditing-before-launch` before the 0.4.0 tag.

If Bcc delivery fails, Graph send is held back (spec); log the result on #382.
