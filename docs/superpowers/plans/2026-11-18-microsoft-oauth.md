# Envio Plan: Microsoft accounts with OAuth 2.0 and XOAUTH2 (#219)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Outlook.com, Hotmail, Live, MSN and Microsoft 365 accounts by signing in with Microsoft in the system browser (authorization code with PKCE), then sync and send over IMAP and SMTP with XOAUTH2. A revoked token shows "Sign in again"; a send that cannot authenticate is held in the Outbox, never dropped.

**Design:** `docs/design/carbon-ms-oauth.html` (the notes under each frame are part of the spec). No new colour role. `docs/10-design.md` has the Views row.

**Owner decision (2026-11-18):** the Microsoft Entra app is not registered yet; the owner supplies the client ID later. Everything here (code, tests, UI) lands and is releasable without it. With no client ID the wizard behaves as today for Microsoft domains (#221's "not supported yet" message) and never shows a sign-in that cannot work. The last task is blocked on the owner.

**Architecture:** A new `envio-net::oauth` module holds the pure parts (PKCE, URLs, token request and reply parsing, the provider table) and a thin HTTPS client. `envio-core` owns the tokens (a 0600 file beside `passwords.json`), one refresh at a time per account, and the two commands that run the flow. `envio-ui`/`qt`/QML only show the steps. `envio-net` gets two new authenticators (IMAP and SMTP XOAUTH2) that take an access token.

**Tech Stack:** Rust (`envio-net`, `envio-store`, `envio-core`), cxx-qt in `qt/`, Qt 5.15 / QtQuick 2.15, Lomiri URL dispatcher.

## Global Constraints

From `CLAUDE.md`, verbatim where quoted:

- "Logic in Rust, Qt and QML only for UI and platform APIs. The core is Qt-free; `unsafe` only in `qt/`."
- "Dependencies point one way: ui → core → sync → net, store, mime. net, store and mime never depend on each other." (So the token file lives in `envio-core` like `secrets.rs`, not in the store, and the account's auth kind is a plain string in the store.)
- "One concept per module, files around 300 lines, logic kept out of I/O code (pure planners, thin drivers)."
- "Migrations are numbered and never edited after merge." (No SQLite change here. `accounts.json` gains one optional field.)
- "Unsent work (Outbox, drafts, pending actions) lives outside the SQLite cache and is saved within about 1 s."
- "User-facing text is translated: `qsTr()` in QML, gettext in Rust, domain `envio.aps`. Run `cargo make pot` after changing strings."
- "Single-crate integration tests go in `crates/<crate>/tests/`; anything that needs two crates goes in `envio-e2e`."
- Lomiri and Suru APIs only in `qml/platform/ut/`. Test accounts use `example.org` / `envio.test` / `outlook.example` only; no real Microsoft address appears in a test, a log or a screenshot.
- No token, authorization code, PKCE verifier or refresh token is ever logged, put in `Debug` output, an event text, a panic message, SQLite or the cache. Types holding them are `SecretString` and implement `Debug` by hand.
- `cargo make ci` passes at every commit; QML tasks also `cargo make qml-test`; tasks touching `qt/` or `qml/` also `cargo make app-docker`.
- Blast radius: everything except the QML pages and tests is trunk (full review): `envio-net`, `envio-core`, `envio-store`, `qt/`, `envio.apparmor`-adjacent files.

## Decisions

1. **Flow: authorization code with PKCE (S256) in the system browser, redirect through the URL dispatcher.** Redirect URI `enviooauth://callback`. Fallback: device code (decision 2).
   - Microsoft's redirect-URI page (checked 2026-11-18, learn.microsoft.com/entra/identity-platform/reply-url) documents only `https` and `http://localhost`/`127.0.0.1` (port ignored) as valid. It does not mention custom schemes. MSAL's mobile apps register `msauth.<bundle>://auth` and `msal<client_id>://auth` under the "Mobile and desktop applications" platform, and that platform's "Custom redirect URIs" box takes arbitrary schemes, but nothing documented promises it for the "personal accounts and organizations" audience. **This is unverified until the app is registered; T10 checks it first.** Query parameters in a redirect URI are not allowed for personal accounts; ours has none (the code and state arrive in the query of the response, which is fine).
   - `https://login.microsoftonline.com/common/oauth2/nativeclient` is supported but only shows the code in the browser's address bar; Envio cannot see it without an embedded web view (which the product avoids). Rejected.
   - Loopback (`http://localhost:<port>`) is the documented-safe redirect, but on Ubuntu Touch the browser is in front, so Lomiri suspends Envio and its listener cannot answer; the browser would hang on a dead page until the user switches back. The URL dispatcher launches or resumes the app and hands it the URL, which is the platform's own way. Loopback stays a possible desktop-only follow-up (needs a desktop scheme handler otherwise).
   - Ubuntu Touch 24.04 side (read from the repo, to be confirmed by T0 on a phone): `envio.url-dispatcher` already lists `mailto` and is installed by `clickable.yaml` (`install_root_data`) and wired by `manifest.json` (`hooks.envio.urls`); adding `{"protocol": "enviooauth"}` is the whole registration. `envio.apparmor` needs no change: it already has `networking` (TLS to Microsoft) and the reader already opens links through `Platform.Uri.open` (`Qt.openUrlExternally`). The cold-start path exists too: `Exec=envio %U` puts the link on the command line and `Platform.Uri` scans it.
2. **Fallback: OAuth 2.0 device code** (`/devicecode` then poll `/token`). Needs no redirect, no scheme and no inbound delivery, and it survives suspension (polling resumes with the app). Reasons it is not the primary: the user must copy a code onto another page, and many Microsoft 365 tenants block the device code flow by Conditional Access. The event that opens the browser carries an optional `user_code`, so adding it later changes no API. It becomes the primary if T0 or T10 shows the scheme cannot work.
3. **The client ID is one checked-in `const`**: `crates/envio-net/src/oauth/client_ids.rs`, `pub const MICROSOFT: &str = "";`. Why: a public client's ID is not a secret; one place, reviewed in a diff, identical in every build and in CI, no `clickable`/Docker/CMake environment plumbing, and the owner's step is a one-line PR. (A build-time env var was considered and rejected for the plumbing and for builds that silently differ.) Production code reaches it only through `Provider::microsoft() -> Option<Provider>`, which is `None` while the constant is empty. Tests and `envio-core::Config` inject a `Provider` with a fake client ID and fake endpoints, so no test depends on the constant.
4. **Empty constant = today's behaviour.** `Detect` reports `oauth: None`, the wizard never reaches the OAuth step, and Microsoft domains get #221's message (or today's password step if #221 has not merged; the wizard code takes whichever exists, see T7). The QML has no "Sign in with Microsoft" anywhere unless the model says `oauth` is offered.
5. **Refresh token: a new 0600 file `oauth.json` in the data dir**, beside `passwords.json`, written with `fsutil::write_atomic(.., 0o600)`, removed with the account. Why separate from `passwords.json`: that file's values are passwords and an older Envio would try to LOG IN with a refresh token; keeping them apart makes a downgrade fail closed. Never in SQLite, the cache, logs or `accounts.json`. The access token lives only in memory (`SecretString`), never on disk.
6. **Account kind:** `AccountSettings.auth: Option<String>` in `accounts.json` (`#[serde(default, skip_serializing_if = "Option::is_none")]`; `None` = password, `Some("microsoft")` = OAuth). A plain string keeps `envio-store` free of `envio-net` types. An older Envio ignores the field (its file reader does not use `deny_unknown_fields`) and finds no password: the account shows "Sign-in failed" and is paused, nothing is lost.
7. **Refresh:** the access token is refreshed when fewer than 5 minutes remain, before every IMAP or SMTP connect, one refresh at a time per account (an async mutex held across the request). A server's AUTHENTICATE refusal with a token older than 60 s forces one refresh and one retry; with a token younger than 60 s it is a sign-in problem at once. The token endpoint's `invalid_grant`, `interaction_required` or `consent_required` pause the account with `Problem::SignIn` ("Sign in again"); network errors, 5xx and 429 are `Problem::Temporary` (backoff as today). The new refresh token Microsoft returns replaces the old one on disk before the access token is used.
8. **Outbox:** no new Outbox rule is needed, and a test proves it. `after_send` already maps `SignIn` to "stay Queued, stop the account" and Temporary to "Queued, retry". A token that cannot be refreshed because of the network retries; one Microsoft refuses pauses the account with the item queued; after "Sign in again" `resume` runs the Outbox. Nothing marks it Failed or removes it.
9. **Identity: accept on a match, ask on a mismatch, never hard-reject.** The request adds `openid email profile`. `identity_of(id_token)` returns both the `email` and the `preferred_username` claims (either may be absent). **Rule:** the typed address (trimmed, case-insensitive) equal to the `email` claim or to `preferred_username` is a match and the account is saved at once. Anything else (a UPN that differs from the mail address, an outlook.com alias, a vanity address on a Microsoft 365 mailbox, no claim at all) is not an error: nothing is stored, the tokens stay in the worker's attempt (memory only), and the wizard asks "You signed in as X. Use this account for Y?" with **Use this account** and **Cancel** (X is the `email` claim, else `preferred_username`; with neither the text is "You signed in to a Microsoft account. Use it for Y?"). `Command::ConfirmOAuth { accept: bool }` answers: accept saves the account under the typed address, decline drops the tokens and the attempt. Why not reject: a UPN or alias is a legitimate sign-in for that mailbox, and a typo is caught by the question. The same rule runs for "Sign in again" (compared with the account's address; a mismatch asks, because the new token may belong to another mailbox). **XOAUTH2 `user=` is the typed mailbox address** (stored as the account's `email` and `username`): Exchange accepts the primary SMTP address or the UPN, so no claim is ever sent as the user; whether an alias also works is a T10 manual-test item (if not, the follow-up is to store the UPN as `username`).
10. **Hosts for a Microsoft OAuth account come from the provider table, not autoconfig:** IMAP `outlook.office365.com:993` TLS, SMTP `smtp.office365.com:587` STARTTLS, both for personal and Microsoft 365 (Microsoft's own client guidance uses these for both). Scopes: `https://outlook.office.com/IMAP.AccessAsUser.All https://outlook.office.com/SMTP.Send offline_access openid email profile`. Authority: `https://login.microsoftonline.com/common/oauth2/v2.0/` (the app registration is "any organization and personal accounts"). Microsoft 365 tenants can switch off "Authenticated SMTP" per mailbox; Exchange then answers `535 5.7.139` ("SmtpClientAuthentication is disabled"). Today `reply::error_for(.., signing_in = true)` maps every 5xx during sign-in to `SignIn`, and a login failure in `connect_smtp` pauses the account, so the user would loop on "Sign in again". **Rule for OAuth accounts:** a `5.7.139` reply, and any SMTP sign-in refusal that arrives with a token under 60 s old while this account's IMAP login with a bearer token has already succeeded (the token is good, SMTP is the problem), is a **send refusal with a hint**: the due Outbox items become `Failed` with the server's text plus "Ask your administrator to allow Authenticated SMTP for this mailbox, then send again.", the account is not paused and mail keeps syncing. Any other SMTP sign-in refusal stays `SignIn`. Detail in T5 and T6. (The login happens in `connect_smtp`, before `send_outbox`, so `after_send` is never reached by it; the change is in the connect path, not in `after_send`.)
11. **Sign-in state is in memory only.** The PKCE verifier and `state` live in the worker for one attempt (5 minutes, one at a time, replaced by a new `StartOAuth`). If Envio is killed meanwhile the redirect arrives with nothing to match: the core answers with a Notice ("That sign-in link is no longer valid. Start again.") and changes nothing. No verifier is ever written to disk.
12. **Shared with Google (#220):** `Provider` (endpoints, scopes, client ID, hosts, redirect), PKCE, the token client, XOAUTH2 in IMAP and SMTP, `oauth.json`, the refresh single-flight, the commands and events, the wizard step and the "Sign in again" page. Microsoft-only: the provider values, the id_token identity check (Google's `userinfo` differs), and the domain table. #220 adds a `Provider::google()` and a table row.

## Tasks

Each task lands green on its own. Order: T0 (phone, no code, can run any time before T8), T1, T2, T3, T4, T5, T6, T7, T8, T9, T10. T1 to T3 are `envio-net` only; T4 is store plus core; T5 and T6 are core (T5 and T6 also touch `envio-sync`); T7 is core, `envio-ui` and `qt/`; T8 is QML; T9 is end-to-end and docs; T10 is blocked on the owner.

---

### T0: Phone spike for the redirect (no code merged)

Needs the phone: ask first (see "Phone shared, ask first"). A throwaway click, or this branch's build with the protocol added locally.

- [ ] Add `{"protocol": "enviooauth"}` to `envio.url-dispatcher`; install.
- [ ] In the browser open a page with a link `enviooauth://callback?code=abc&state=xyz`. Record: does Morph hand it to the dispatcher, does it ask first, is the whole query kept, does Envio come to the front when it is running, when it is in the background, and when it was closed (cold start: the link must be on the command line).
- [ ] Record the result as a short "Spike result" section in this file, with the date, in the PR that lands T1 (docs only).

**Outcome rules:** works in all three states: proceed. Works only when running: add the restart notice (decision 11 already covers it) and proceed. Does not work: switch the primary to device code (decision 2); T5 to T7 change only in what the browser opens and a poll loop replacing the redirect step.

---

### T1: XOAUTH2 in IMAP and SMTP (envio-net)

**Files:** `crates/envio-net/src/imap/sign_in.rs` (new `Method::Xoauth2`, `xoauth2_response`, authenticator), `crates/envio-net/src/server.rs` (`Credentials`), `crates/envio-net/src/smtp/mod.rs` (`login`), tests: `crates/envio-net/tests/support/fake_imap.rs`, `fake_smtp.rs`, new `crates/envio-net/tests/xoauth2.rs`.

**Produces:**
```rust
// server.rs
pub enum Secret { Password(SecretString), Bearer(SecretString) }
pub struct Credentials { pub username: String, pub password: SecretString, bearer: bool }
impl Credentials {
    pub fn new(username: &str, password: &str) -> Self;           // unchanged
    pub fn bearer(username: &str, access_token: &str) -> Self;    // NEW
    pub fn is_bearer(&self) -> bool;
}
// imap/sign_in.rs
/// RFC-less de facto format: "user=" u "\x01auth=Bearer " t "\x01\x01" (not base64; async-imap encodes).
fn xoauth2_response(username: &str, token: &str) -> Vec<u8>;
```
(Use whichever of the two shapes is smaller; the point is that `Credentials::new` call sites in `envio-e2e` and the tests do not change, and a bearer credential cannot be mistaken for a password.)

**Behaviour:**
- IMAP: a bearer credential always uses `AUTHENTICATE XOAUTH2`, never LOGIN or PLAIN, and only if CAPABILITY lists `AUTH=XOAUTH2` (reuse `offers_plain`'s capability read, generalised to `offers(mech)`); otherwise `NetError::SignIn`. async-imap sends the initial response after the server's `+`. On refusal Microsoft's IMAP answers `NO AUTHENTICATE failed.` and some servers send a `+ <base64 JSON>` challenge first: the authenticator answers any later challenge with an empty response (RFC-style failure acknowledgement), then the `NO` maps to `NetError::SignIn`.
- SMTP: `Mechanism::Xoauth2` (lettre 0.11.24 has it). `login` picks `[Xoauth2]` for a bearer credential, `[Plain, Login]` as today otherwise. lettre answers a `334` error challenge with "This mechanism does not expect a challenge" (a client error): add it to `client_error` as a sign-in problem, next to "No compatible authentication mechanism". `535 5.7.3` already maps through `reply::error_for(.., signing_in = true)`.

**Invariants:** the token never appears in `Debug` (extend `debug_never_shows_the_password`); a bearer credential is never sent as PLAIN or LOGIN to a server without XOAUTH2.

**Tests (unit):** `xoauth2_response("a@outlook.example","tok")` equals the bytes with `\x01`; `choose`-style selection for bearer; the `client_error` text; `Debug` hides the token. **Tests (integration, fake servers):** `fake_imap` and `fake_smtp` gain a script field `xoauth2_accepts: &'static str` (the decoded string they accept), advertise `AUTH=XOAUTH2` / `AUTH XOAUTH2` in the signed-out capabilities and EHLO when it is set, and implement the Microsoft failure shapes: IMAP `NO AUTHENTICATE failed.`, IMAP `+ <base64 {"status":"401"}>` then `NO`, SMTP `334 <base64 json>` then `535 5.7.3`, SMTP bare `535 5.7.3`. Assert: success; wrong token gives `SignIn` (all four shapes); a server without XOAUTH2 gives `SignIn` without sending the token (check the transcript); the transcript never shows PLAIN or LOGIN.

**Stalwart:** `cargo make net-it`'s Stalwart (v1.0.3) has the XOAUTH2 and OAUTHBEARER mechanisms compiled in (strings present in the binary) but is configured through JMAP and the token would have to come from its own OAuth server; whether that can be scripted in `up.sh` is unverified. The fake servers are the baseline and enough for CI. As a timeboxed (1 h) extra in this task, probe the throwaway Stalwart's CAPABILITY for `AUTH=XOAUTH2`; if it is offered and a token can be had with a `curl` call, add one `#[ignore]`-free test in `tests/stalwart.rs`; if not, write the finding here and stop.

**Outcome (T1):** the Stalwart probe was skipped; the fake servers are the CI baseline. A bearer credential against a server without XOAUTH2 returns `NetError::Refused` (IMAP and SMTP), not `SignIn`, so T5/T6 must not read it as an expired token: only `SignIn` triggers the refresh-and-retry rule.

---

### T2: The pure OAuth parts (envio-net `oauth` module)

**Files:** new `crates/envio-net/src/oauth/mod.rs`, `pkce.rs`, `url.rs`, `token.rs` (reply parsing), `provider.rs`, `client_ids.rs`; `lib.rs` (`pub mod oauth`). `Cargo.toml` of `envio-net`: **SHA-256 and randomness come from `ring` (already a direct dependency, `ring = "0.17"`, locked at 0.17.14): `ring::digest::SHA256`, `ring::rand::SystemRandom`; no `sha2`, no `getrandom`.** New direct dependencies, all already in `Cargo.lock` (no new crate to download or audit): `base64` (the locked version, 0.23.1; base64url without padding and the JWT payload), `serde` with `derive` and `serde_json` (`"1"`, as `envio-core` and `envio-store` declare them; the token reply and the JWT claims). Check `cargo tree -d` shows no second copy afterwards.

**Produces:**
```rust
pub struct Provider {
    pub kind: ProviderKind,                 // Microsoft (Google in #220)
    pub client_id: String,
    pub authorize_url: String,              // https://login.microsoftonline.com/common/oauth2/v2.0/authorize
    pub token_url: String,
    pub scopes: &'static str,
    pub redirect_uri: String,               // "enviooauth://callback"
    pub imap: Endpoint, pub smtp: Endpoint, // the hosts of decision 10
}
impl Provider {
    pub fn microsoft() -> Option<Provider>;           // None while client_ids::MICROSOFT is empty
    pub fn with_client_id(self, id: &str) -> Self;    // tests
}
/// The provider that offers OAuth for this address's domain, if any (pure table, case-insensitive,
/// trailing dot ignored): outlook.com, hotmail.*, live.*, msn.com, office365 domains known to the
/// table. Custom domains hosted at Microsoft are #221's MX check; this table takes its result.
pub fn provider_kind_for(email: &str) -> Option<ProviderKind>;

pub struct Pkce { pub verifier: SecretString, pub challenge: String, pub state: String }
impl Pkce { pub fn new() -> Self; pub fn from_parts(verifier: &str, state: &str) -> Self; }
pub fn authorize_url(p: &Provider, pkce: &Pkce, login_hint: &str) -> String;

pub struct Redirect { pub code: SecretString, pub state: String }
pub enum RedirectError { Denied, Mismatch, Malformed, Server(String) }
pub fn parse_redirect(url: &str, expect_state: &str, redirect_uri: &str) -> Result<Redirect, RedirectError>;

pub struct Identity { pub email: Option<String>, pub preferred_username: Option<String> }
pub struct Tokens { pub access: SecretString, pub refresh: Option<SecretString>, pub expires_in: u64, pub identity: Option<Identity> }
pub enum TokenError { Revoked, Temporary, Refused(String) }   // `Revoked` = invalid_grant, interaction_required, consent_required, invalid_client
pub fn token_request_body(...) -> String;                      // form-urlencoded: authorization_code and refresh_token grants
pub fn parse_token_reply(status: u16, body: &[u8]) -> Result<Tokens, TokenError>;
pub fn identity_of(id_token: &str) -> Option<Identity>;        // both claims, from the unverified JWT payload
/// Decision 9: the typed address (trim, ASCII case-insensitive) equals `email` or `preferred_username`.
pub fn identity_matches(typed: &str, id: &Identity) -> bool;
```

**Invariants:** `code_verifier` is 64 URL-safe characters from `SystemRandom` (RFC 7636: 43 to 128), `code_challenge = base64url-nopad(SHA-256(verifier))`; `state` is 32 random bytes in base64url; every parameter in the authorize URL is percent-encoded; `parse_redirect` checks the scheme and host exactly (`enviooauth://callback`), `state` in constant time, treats `error=access_denied` as `Denied` and any other `error` as `Server` with the `error` code only (never `error_description`, which can echo the address); `parse_token_reply` never puts the body in an error; `identity_of` does not verify the signature (the token came straight from the token endpoint over TLS, which is the OIDC rule for the code flow) and says so in its doc comment; a 5xx or 429 is `Temporary`, a 400 with `invalid_grant`/`interaction_required`/`consent_required`/`invalid_client` is `Revoked`, other 4xx are `Refused`; a reply whose body is not JSON, at any status, is `Temporary` (a captive portal or proxy page; OAuth errors are always JSON); JSON without a token or with wrong types is `Refused`.

**Tests (unit):** the RFC 7636 appendix B vector (`dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk` → `E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM`); the authorize URL has every parameter (response_type=code, client_id, redirect_uri, scope, state, code_challenge, code_challenge_method=S256, login_hint) and the hint's `@` and `+` are encoded; redirect parsing (ok, wrong state, wrong host, `access_denied`, missing code, repeated parameters, a huge query); token replies (ok with and without a refresh token, each error class, a body with HTML, a negative `expires_in`); `identity_of` on three fixtures including garbage and a token with only `preferred_username`; `identity_matches` (equal to `email`; equal to `preferred_username` with different case and surrounding spaces; a UPN that differs from the typed mail address while `email` equals it; neither equal; both claims absent is no match; a prefix or substring is no match); the provider table (outlook.com, `Outlook.COM`, hotmail.co.uk, live.nl, msn.com, example.org, `outlook.com.example.org` → none); `Provider::microsoft()` is `None` for the empty constant (assert on the constant itself so the owner's PR must update the test, see T10).

---

### T3: The token client and a fake token endpoint (envio-net)

**Files:** lift `autoconfig/http.rs`'s reusable reply code to `crates/envio-net/src/http.rs` (move with `git mv`, re-export from `autoconfig` so its tests and callers stay green; do this as its own commit first), new `crates/envio-net/src/oauth/client.rs`, `crates/envio-net/tests/oauth_token.rs`, `crates/envio-net/tests/support/fake_token.rs`.

**Produces:**
```rust
/// POST application/x-www-form-urlencoded to `provider.token_url`; returns the parsed reply.
/// Host and path come only from the Provider (never from user input or a server reply),
/// no redirects are followed, the body cap is `http::MAX_BODY`, the whole call is bounded by
/// `COMMAND_TIMEOUT`.
pub async fn exchange_code(p: &Provider, roots: &Roots, code: &SecretString, verifier: &SecretString) -> Result<Tokens, TokenError>;
pub async fn refresh(p: &Provider, roots: &Roots, refresh_token: &SecretString) -> Result<Tokens, TokenError>;
```
No new HTTP crate (reqwest and friends are heavy for a phone and bring their own TLS roots): HTTP/1.0 over `tls::connect(&Endpoint::new(host, 443, Security::Tls), roots)`, the same way autoconfig fetches, with the reply parser of `http.rs`.

**Test support:** `fake_token::serve(cert, script) -> (port, Transcript)` is a TLS listener (same `certs.rs`, `rcgen`) that answers `POST /token` from a script (a queue of `(status, json)`), records the decoded form body, and can stall or cut the connection. `Provider::token_url` points at `https://127.0.0.1:<port>/token` and the test's `Roots` trusts the test CA.

**Tests (integration):** the code grant sends `grant_type=authorization_code`, `client_id`, `code`, `redirect_uri`, `code_verifier` and no `client_secret`; the refresh grant sends `grant_type=refresh_token`, `refresh_token`, `scope`; success parses; `invalid_grant` is `Revoked`; 503 and a cut connection and a stall are `Temporary`; an HTML 200 is not a token; the transcript of an error path contains no token (the logs are fed through a capturing logger and searched for the fake token strings).

---

### T4: Tokens at rest and the account's auth kind (envio-core, envio-store)

**Files:** new `crates/envio-core/src/oauth_tokens.rs` (the `oauth.json` file, modelled on `secrets.rs`: same permission narrowing, atomic write, `Newer(u32)` guard, hand-written `Debug`), `crates/envio-store/src/accounts.rs` (`AccountSettings.auth`; `git grep -n "AccountSettings {"` finds 9 struct literals that gain `auth: None`: `envio-store/src/accounts.rs:288` and `envio-store/tests/accounts.rs:8`, and in `envio-core` `worker/conn.rs:394`, `worker/testing.rs:56` and `:107`, `worker/setup.rs:235` and `:330`, `worker/trust.rs:162`, `tests/support/mod.rs:131`; the compiler lists any more), `crates/envio-core/src/worker/store.rs` (field `tokens: OAuthTokens` next to `secrets`), `worker/remove.rs` (remove the account's token with its password), `worker/store.rs` open path.

**Produces:**
```rust
pub const FILE_NAME: &str = "oauth.json";
pub struct OAuthTokens { /* path, map: BTreeMap<String, SecretString> (account key -> refresh token) */ }
impl OAuthTokens {
    pub fn open(data_dir: &Path) -> Result<Self, TokensError>;
    pub fn get(&self, account: &str) -> Option<&SecretString>;
    pub fn set(&mut self, account: &str, refresh: SecretString) -> Result<(), TokensError>;   // atomic, 0600, in-memory map changes only after the write succeeds
    pub fn remove(&mut self, account: &str) -> Result<(), TokensError>;
}
// accounts.rs
pub struct AccountSettings { /* ..., */ #[serde(default, skip_serializing_if = "Option::is_none")] pub auth: Option<String> }
impl AccountSettings { pub fn is_oauth(&self) -> bool }
```

**Invariants:** a failed write leaves the old token in memory and on disk; a damaged or newer file is reported like `passwords.json` (the account is stopped, not wiped); removing an account removes its token (and its password if any) even when one of the two removals fails; no token in `Debug`, in `Display` of the error, or in the account JSON. Existing account files load unchanged and round-trip byte-identical when `auth` is `None`.

**Tests:** modelled on `secrets.rs`'s: round trip, 0600 and widening narrowed, damaged, newer, failed write keeps the old value, `Debug` hides the value, remove is idempotent. `accounts.rs`: an old file loads with `auth: None` and saves without the field; a file with `"auth":"microsoft"` round-trips. A remove-account test in `crates/envio-core/tests/accounts.rs` checks both files.

---

### T5: Refresh and connect with a token (envio-core)

**Files:** new `crates/envio-core/src/worker/token.rs` (the single-flight refresher), `worker/conn.rs` (`login` becomes `Login`, `connect_imap`/`connect_smtp` take it), `worker/open.rs` (the background IDLE open resolves its login inside the future), `worker/run/mail.rs` (Outbox connect), `problem.rs`, `text/status.rs` and `health.rs` (wording), `core.rs` (`Config.oauth: Vec<Provider>`, default `Provider::microsoft()` if any).

**Lasting connection faults (from T3):** the token client maps every TLS and certificate fault to `TokenError::Temporary` (a wrong clock or a captive portal clears by itself), so a lasting one, for example roots that no longer vouch for the token host, retries as `Temporary` indefinitely; T5 must show a lasting offline or connection-problem state for it, not spin silently.

**Produces:**
```rust
/// What a connect needs, resolved without awaiting (clone-cheap handles, no &Worker borrow).
pub(crate) enum LoginSource { Password(envio_net::Credentials), Token(TokenHandle) }
pub(crate) struct Login { pub endpoint: envio_net::Endpoint, pub source: LoginSource }
pub(super) fn login(account: &Account, secrets: &Secrets, tokens: &TokenCells, service: Service) -> Result<Login, Problem>;

#[derive(Clone)] pub(crate) struct TokenHandle { /* Rc<TokenCell>: account key, provider, roots, Rc<RefCell<OAuthTokens>> */ }
impl TokenHandle {
    /// A usable access token: the cached one if it has more than REFRESH_MARGIN (5 min) left,
    /// else one refresh (single-flight: a second caller awaits the first's result).
    pub(crate) async fn access(&self, force: bool) -> Result<SecretString, Problem>;
    pub(crate) fn age_ms(&self) -> Option<i64>;
    pub(crate) fn imap_ok(&self) -> bool;   // set by a successful bearer IMAP login, cleared by a Revoked refresh
}
/// Revoked -> SignIn; Temporary -> Temporary; Refused -> SignIn. A refused refresh (blocked
/// client, policy, bad request) cannot be fixed by retrying, and `Problem::Refused` would not
/// pause the account, so it pauses with "Sign in again", the one action that can help.
pub(crate) fn token_problem(error: &TokenError) -> Problem;
/// SMTP over a bearer token (decision 10). `5.7.139` in the text, or a sign-in refusal with a
/// fresh token (< 60 s) while `imap_ok` (a bearer IMAP login succeeded for this account since
/// its last sign-in), becomes `NetError::Refused(text + hint)`; everything else is unchanged.
pub(crate) fn bearer_send_error(error: NetError, token_fresh: bool, imap_ok: bool) -> NetError;
```
The account's `AccountStatus::SignInFailed` text depends on the kind: `status(&AccountStatus, oauth: bool)` returns "Sign in again" for OAuth accounts and keeps "Sign-in failed, update password" for passwords.

**Behaviour:**
- `connect_imap` / `connect_smtp`: password accounts are untouched. Token accounts call `access(false)`, connect with `Credentials::bearer(username, token)`. On `NetError::SignIn` from the server: if the token is older than 60 s (`age_ms`), `access(true)` and retry exactly once; else return `Problem::SignIn`. A second refusal is `Problem::SignIn`.
- The refresh writes the new refresh token (when the reply has one) with `OAuthTokens::set` before returning the access token; if that write fails, log (no secret), keep the new token in memory, and carry on; the next start refreshes from the stored one (Microsoft keeps the previous refresh token valid for a while).
- `begin_open`'s IDLE open no longer calls `login` before spawning: `Job.login` becomes a `Login` and `open_inbox` awaits `access(false)` as its first step, so the worker never blocks on the network.
- `connect_smtp` (token accounts) passes a login failure through `bearer_send_error`; a resulting `Refused` is returned as `Problem::Refused(text)`. `worker/run/mail.rs::send` catches that one case from the connect (before `send_outbox`) and calls a new `envio_sync::driver::refuse_outbox(local, account, now_ms, reason: &str) -> Result<Sent>`, which sets every due `Send`-step item (same skip rules as `send_outbox`) to `SendState::Failed { reason }`, returns their ids in `Sent.failed` and clears their schedule entry; `send` then emits `OutboxChanged` as for any failed item and returns `Ok(())`, so the account is not paused and IMAP carries on. Item state changes stay in `envio-sync`, pure and tested there. A password account is never affected.
- `Problem::SignIn` pauses the account through `health.failed` exactly as today; `resume` after a new sign-in runs everything again.

**Invariants:** at most one refresh request per account in flight; a refresh never runs for a paused account; the refresh token is never read from anywhere but `OAuthTokens`; a token that expires while a long IDLE is open is not a problem (the next reconnect refreshes). Nothing here marks an Outbox item Failed.

**Tests (core, with the fake token endpoint from T3 and the fake IMAP/SMTP from T1, in `crates/envio-core/tests/oauth.rs`; the cross-crate fakes live in `envio-e2e` if `envio-core` cannot reach `envio-net`'s test support, then these go to T8):** unit tests for `token_problem`; a cached token is reused (the endpoint sees one request for ten connects); an expiring token refreshes once for two concurrent connects; a refresh returning a new refresh token persists it before the IMAP login; `invalid_grant` leaves the account `SignInFailed` and the text "Sign in again"; a 503 leaves it `Offline` with a retry time; IMAP refusal with an old token refreshes and succeeds on the second try; refusal with a fresh token is `SignIn` without a second refresh; no secret in the captured log; `bearer_send_error` as a table (`5.7.139` with an old token and `imap_ok` false is `Refused`; bare `535 5.7.3` with a fresh token and `imap_ok` true is `Refused`; the same with `imap_ok` false or an old token stays `SignIn`; a password-account error is not passed through it); `refuse_outbox` in `crates/envio-sync/tests/outbox.rs` (due items become `Failed` with the reason; a held or not-due item and a `MayHaveBeenSent` item are untouched).

---

### T6: Outbox holds a send whose token fails (envio-core, envio-sync tests)

**Files:** tests only, plus any one-line change they force: `crates/envio-sync/tests/outbox.rs`, `crates/envio-core/tests/oauth.rs`, `crates/envio-e2e/tests/oauth_outbox.rs` (if two crates are needed).

**Invariants proven:** (a) SMTP `535 5.7.3` with a fresh token while no bearer IMAP login has succeeded (or the refresh was Revoked): the item stays `Queued`, the account is paused with `SignInFailed`, the item is still in the Outbox directory after a restart; (b) refresh `Temporary` (token endpoint down): item `Queued`, `retry: true`, account `Offline`, retried by the normal backoff, then sent when the endpoint returns; (c) after "Sign in again" completes (a new refresh token stored with `OAuthTokens::set`, then `resume(account)`: the two calls T7's redirect path makes; the path through `OAuthRedirect` itself is tested in T7 and T9) the held item is sent; (d) an expired access token that refreshes fine sends on the first run with no user-visible state; (e) `MayHaveBeenSent` behaviour is unchanged (a crash after the token was accepted does not resend); (f) SMTP `535 5.7.139`: the due items become `Failed` with the server text and the hint, they are still Failed after a restart, the account is **not** paused, IMAP still syncs, and sending the item again works once the server accepts; (g) bare `535 5.7.3` with a fresh token after a successful bearer IMAP login gives the same result as (f); (h) the same `535 5.7.3` for a password account is still `SignIn` and pauses (regression).

---

### T7: Commands, events and the wizard flow (envio-core, envio-ui, qt)

**Files:** `crates/envio-core/src/api/command.rs`, `event.rs`, `worker/dispatch.rs`, `worker/setup.rs` (`save_oauth_account`), new `worker/oauth.rs` (the flow, one concept), `worker/setup.rs` (`Detect` result), `crates/envio-ui` (wizard state, signals), `qt/src/calls.rs`, `core_object.rs`, `convert.rs`, `envio.url-dispatcher`, `tests/qml/MockCore.qml`.

**Produces:**
```rust
// command.rs
Command::StartOAuth { email: String, account: Option<String> }, // account: Some(key) = "Sign in again"
Command::OAuthRedirect { url: String },                         // from Platform.Uri
Command::ConfirmOAuth { accept: bool },                        // answers the identity question (decision 9)
Command::CancelOAuth,
// event.rs
Event::Detected { /* existing */, oauth: Option<OAuthOffer> }   // OAuthOffer { provider: String ("Microsoft"), name_hint } ; None when no client ID
Event::OAuthBrowser { email: String, url: String, user_code: Option<String> } // QML opens `url`; user_code is for the device-code fallback
Event::OAuthConfirm { email: String, signed_in_as: Option<String>, text: String }  // decision 9: "You signed in as X. Use this account for Y?"
Event::OAuthDone { email: String, outcome: OAuthOutcome }       // Added | Resumed | Cancelled | Failed(OAuthFailure)
pub enum OAuthFailure { Timeout, Offline, Blocked, Other }      // each has translated text built in core
// On success for a new account the core emits the existing Event::AccountAdded { account } first, then OAuthDone { Added } (no payload).
```
`envio.url-dispatcher` becomes `[{"protocol": "mailto"}, {"protocol": "enviooauth"}]`.

**Behaviour:**
- `Detect`: when `provider_kind_for(email)` is Some and `Provider::microsoft()` is Some, the answer carries `oauth: Some(..)` and the servers of decision 10, `source: Provider`. When the kind is Some but there is no provider (empty constant), the answer is exactly what it is today for that address (or #221's "Microsoft accounts are not supported yet" unavailable result if #221 has merged; the table of #221 and `provider_kind_for` are the same table: whichever merges second imports the first's). No code path offers OAuth without a provider.
- `StartOAuth`: refuses (Notice) if there is no provider, if `account` names a non-OAuth or unknown account, or (for a new account) if the address is already added. Makes a `Pkce`, stores `{pkce, email, account, started_at}` in the worker (replacing any earlier attempt), emits `OAuthBrowser`, and arms a 5-minute timeout (`Timeout` failure). Opening the browser is QML's job (`Platform.Uri.open`); if it returns false QML shows "Open the browser again" as a link.
- `OAuthRedirect`: matches `state`; on `Denied` emits `Cancelled`; on `Mismatch`/`Malformed` or with no attempt, a Notice and nothing else; otherwise `exchange_code` on the setup thread's runtime (not the worker's), then the identity rule (decision 9): a match goes on; a mismatch keeps the tokens in the attempt and emits `OAuthConfirm`, and `ConfirmOAuth { accept: true }` goes on, `false` is `Cancelled`. Going on, for a new account: build `AccountSettings` from the provider (`auth = Some("microsoft")`, `email` and `username` = the typed address) and call `save_oauth_account`; for "Sign in again": `OAuthTokens::set` then `resume(account)`.
- `save_oauth_account`, beside `save_account` (which keeps its signature and its password path untouched; the `AccountInfo` building moves into one private helper both call):
  ```rust
  fn save_oauth_account(&mut self, settings: AccountSettings, refresh: SecretString) -> Result<AccountInfo, String>;
  ```
  It runs `self.store.accounts.add(settings, now_ms())` first (that creates the key), then `self.store.tokens.set(&key, refresh)`. If `set` fails it logs (no secret), calls `self.store.accounts.remove(&key)` (logging if that fails too) and returns `text::setup::not_saved()`. It never calls `secrets.set`, so `passwords.json` gets no entry (not even an empty password). The caller then does what the password path does after `save_account`: insert a `Live` with `wants.full = true` and emit `Event::AccountAdded`. A first IMAP connect is not done here: the first sync does it, and a failure there is the ordinary paused-account path.
- `CancelOAuth` drops the attempt and emits `Cancelled`.
- A new `Command::UpdatePassword` on an OAuth account is refused with a Notice (a token account has no password).

**Invariants:** one attempt at a time; the verifier and code never leave the worker or reach an event or a log; every exit (success, cancel, timeout, failure, new attempt) clears the attempt; events carry translated text but no token, no code, no Microsoft error text; a result for a replaced attempt is dropped (ticket, like `setup_thread`).

**Tests:** `crates/envio-core/tests/oauth_flow.rs` with the fake token endpoint: detect with and without a provider (the no-provider case equals today's `Detected` for an Outlook address); start, redirect, account added with `auth: Some("microsoft")`, `oauth.json` holds the refresh token and `passwords.json` has no entry; wrong state ignored with the attempt kept; `access_denied` is Cancelled; identity: the typed address equal to `email` saves, equal to `preferred_username` (a UPN) saves, a different address emits `OAuthConfirm` and stores nothing until `ConfirmOAuth { accept: true }` (then the account is saved under the typed address and the XOAUTH2 `user=` is the typed address), `accept: false` stores nothing and drops the tokens, a `ConfirmOAuth` with no pending attempt is a Notice; `save_oauth_account`: success writes `accounts.json` with `auth` and `oauth.json` with the token and no `passwords.json` entry, and a forced `OAuthTokens::set` failure (read-only data dir) removes the account again and leaves both files as they were; token endpoint down is `Offline` and the attempt can be retried; timeout fires (fake clock or short test timeout); a second `StartOAuth` replaces the first; "Sign in again" resumes a paused account and the Outbox drains; removal deletes the token. `qt` and `MockCore.qml` get the four invokables (`startOAuth`, `oauthRedirect`, `confirmOAuth`, `cancelOAuth`) and the signals (`oauthBrowser`, `oauthConfirm`, `oauthDone`).

---

### T8: QML: the wizard step, waiting and errors, "Sign in again" (qml)

**Files:** `qml/pages/accounts/SignInStep.qml` (a `microsoft` variant: no password field, the explanation and privacy card, the "Sign in with Microsoft" button), new `qml/components/NoticeCard.qml` (title, text and an optional button in one card: `qml/components/Banner.qml` has only `kind` and `text`, one line of text beside an icon, no title or button; the browser step, the identity question and the error cards use the new component, tint A or tint B as `Banner` does), new `qml/pages/accounts/BrowserStep.qml` (waiting, Cancel, "Open the browser again", the error banners), `WizardPage.qml` (step `"browser"`; `onDetected` goes to it when `oauth` is offered), `AccountsPage.qml` (first sheet row reads "Sign in again" for a Microsoft account in `SignInFailed`; the old row stays for passwords), new `qml/pages/accounts/SignInAgainPage.qml`, `qml/pages/AppShell.qml` (a second `Platform.Uri { schemes: ["enviooauth"] }` feeding `core.oauthRedirect`; push `SignInAgainPage`; the Outbox row's "Sign in again"), `qml/pages/inbox/OutboxRow.qml` (the held-send text and "Sign in again" action; `OutboxPage.qml` beside it), `tests/qml/`.

**Design:** frames 1 to 8 of `carbon-ms-oauth` in light; dark is checked on the four representative frames the board shows in dark, the rest use the same tokens. The wizard's `connecting` logic does not change for password steps. Errors sit on the step ("Errors sit with their item"); the held-send text sits on the Outbox item. Colours: tint A and tint B only; the primary button is the page's one primary action.

**Strings (all `qsTr`, then `cargo make pot`):** "Sign in with Microsoft", "Waiting for Microsoft", "Open the browser again", "This stops by itself after 5 minutes.", "Sign-in cancelled", "You signed in as %1", "Use this account for %1?", "Use this account", "Sign in again", "Microsoft needs you to sign in again", "Waiting for sign-in. It goes out after you sign in again.". Texts that come from the core (failure bodies) are gettext in Rust.

**Tests (QML, `MockCore.qml`):** with `oauth` absent in `detected` the wizard shows no Microsoft button and no step "browser" (a regression test for the empty client ID); with it present the step shows the button and no password field; the button calls `startOAuth(email, "")` and the page moves to "browser"; the browser step calls `Platform.Uri.open` once with the event's URL (opener replaced as in `tst_reader.qml`); Cancel and Back call `cancelOAuth`; each failure outcome shows its card; `OAuthConfirm` shows the question and its two buttons (Use calls `confirmOAuth(true)`, Cancel `confirmOAuth(false)`); success arrives as `AccountAdded`, the wizard's existing path (`onAccountAdded` sets `busy` false and moves to the `options` step), and `OAuthDone { Added }` only clears the browser step, it does not close the wizard; for "Sign in again", `Resumed` closes `SignInAgainPage`; a redirect link offered to the `Uri` item reaches `core.oauthRedirect`; the accounts sheet shows "Sign in again" only for a Microsoft account in `SignInFailed`; `SignInAgainPage` starts the flow with the account key and closes on `Resumed`; the Outbox row's text and action appear only for a held message of an account in `SignInFailed`. Run `cargo make qml-test` and `cargo make app-docker`; screenshots of each frame on the phone are T10's job with the real flow, and on a desktop `qmltestrunner` with the fake core for the layout check against the board.

---

### T9: End to end without Microsoft (envio-e2e) and the docs

**Files:** `crates/envio-e2e/tests/oauth_e2e.rs`, `tests/support/` (a helper that starts the fake token endpoint and fake IMAP/SMTP behind one `Provider`), `docs/superpowers/specs/2026-11-07-envio-design.md` (move "Microsoft OAuth" from Later into the current list under a new version heading when the owner picks one; note Google stays Later), `CHANGELOG.md` ("Unreleased: Microsoft accounts need the owner's client ID; until then they show the unsupported message"), `docs/design-conformance.md`, `docs/store/listing.md` (only after T10).

**Tests:** the whole path with a injected `Provider` pointing at the fakes: Detect, StartOAuth, redirect URL fed to `OAuthRedirect`, account added, first sync over XOAUTH2, a message sent through SMTP XOAUTH2, access token expiry mid-session (fake clock) refreshed with no UI event, refresh token revoked (`invalid_grant`) gives `SignInFailed` and the held Outbox message, the redirect of a "Sign in again" attempt (`OAuthRedirect`) resumes the account and delivers the held message; a mailbox with SMTP auth disabled (`535 5.7.139`) gives a Failed item with the hint and a live account. A test with the real `Provider::microsoft()` (empty constant) asserts the wizard path equals today's for `x@outlook.example`.

---

### T10 (blocked on the owner): register the app, put the client ID in, test on the phone

Blocked: needs the owner to register the app in Microsoft Entra and send the client ID. Do not start before.

- [ ] **Registration** (owner, about 10 minutes). Entra admin center, App registrations, New registration. Name "Envio". Supported account types: "Accounts in any organizational directory and personal Microsoft accounts". Platform "Mobile and desktop applications", custom redirect URI `enviooauth://callback`. If the portal rejects the custom scheme: that is the signal from decision 1; switch to the device-code primary (Authentication, "Allow public client flows" = Yes) and tell the planner before going on. Under Authentication also turn "Allow public client flows" to Yes (no secret is ever created). API permissions, Microsoft Graph is not needed; add "Office 365 Exchange Online" delegated `IMAP.AccessAsUser.All` and `SMTP.Send`, and Microsoft Graph delegated `offline_access`, `openid`, `email`, `profile`. Publisher verification is optional but without it organizations that restrict user consent show "unverified"; note it in the store listing text.
- [ ] **Client ID PR:** set `client_ids::MICROSOFT`, update every test that asserts it is empty (all three flip, `cargo make ci` fails until they do): the T2 test in envio-net (assert it is a UUID), `the_empty_client_id_offers_no_provider` in `crates/envio-core/src/core.rs` (assert `default_oauth_providers()` holds Microsoft), and `without_a_client_id_the_wizard_path_is_what_it_always_was` in `crates/envio-e2e/tests/oauth_e2e.rs` (its `Provider::microsoft().is_none()` and empty-list asserts). Also add an e2e Detect against the real `Provider::microsoft()` list (the e2e tests inject a provider with fake hosts, so nothing else checks the real one end to end): Detect offers Microsoft with `outlook.office365.com` and `smtp.office365.com`. `cargo make ci`, `cargo make app-docker`.
- [ ] **Manual phone test** with a synthetic, newly made Outlook.com test account (never the owner's own): add the account; send and receive between it and a second synthetic account; wait for the access token to expire (about an hour) with the app open and with the app in the background; revoke the app at account.live.com/consent/Manage and confirm "Sign in again" appears, a queued send waits, and sign-in again delivers it; cold-start redirect (close Envio while the browser is open) shows the "no longer valid" notice; cancel in the browser; wrong account; airplane mode during sign-in. Take the board's screenshots with `ute2e shot` (ask for the phone first).
- [ ] Ship note in `CHANGELOG.md`; the store listing line "Microsoft accounts" comes out of "Not yet".

## API check

Each item verified against the code at `origin/master` ae78094 on 2026-11-18.

| Relied on | Where | Checked |
| --- | --- | --- |
| `Credentials { username, password: SecretString }`, `Credentials::new`, `Debug` test `debug_never_shows_the_password` | `crates/envio-net/src/server.rs:94-` | struct has two public fields and no struct-literal uses outside `server.rs` (grep `Credentials {`), so adding a private field is source compatible |
| IMAP sign-in: `choose`, `Method`, `plain_response`, `Plain: Authenticator`, `run`, `offers_plain` (raw CAPABILITY read with tag `CAP1`) | `crates/envio-net/src/imap/sign_in.rs` | read in full |
| `async_imap::Client::authenticate(auth_type, Authenticator)`; `Authenticator::process(&[u8]) -> Response`; sends the response only after a `+` continuation and base64-encodes it; an empty `process` result answers a failure challenge | `async-imap-0.12.0/src/client.rs:287-335` | read; the failure `+ <base64 json>` path calls `process` again with the decoded challenge |
| `SmtpSession::connect`, `login(&Credentials)`, `Mechanism::{Plain, Login}`, `client_error`, `map_error(e, signing_in)` | `crates/envio-net/src/smtp/mod.rs` | read in full |
| `lettre::transport::smtp::authentication::Mechanism::Xoauth2` and its `user=..\x01auth=Bearer ..\x01\x01` response; it errors "This mechanism does not expect a challenge" on a challenge | `lettre-0.11.24/src/transport/smtp/authentication.rs:58-123` | read; workspace pins `lettre = "0.11.24"` in `envio-net/Cargo.toml` |
| `reply::error_for(code, text, signing_in)` maps 535 to `SignIn` | `crates/envio-net/src/smtp/reply.rs`, called at `smtp/mod.rs:map_error` | call site read; the 535 mapping is asserted in `tests/smtp.rs` (re-read when implementing T1) |
| `NetError::SignIn`, `is_temporary`, `ErrorKind::SignIn` from `net_kind` | `envio-net/src/error.rs`, `envio-sync/src/error.rs:44` | read |
| `tls::connect(&Endpoint, &Roots)`, `Roots` (clone, injectable), `Endpoint::new(host, port, Security)` | `envio-net/src/tls.rs:188`, `server.rs` | read signatures |
| `autoconfig::http` (`parse_url`, `request`, `reply_complete`, `parse_reply`, `finish_reply`, `MAX_BODY`) is private to `autoconfig` and GET only | `envio-net/src/autoconfig/http.rs`, `mod.rs` (`mod http;`) | read; hence the lift in T3 |
| Fake servers: `fake_imap::{Script, serve}` handles CAPABILITY, `AUTHENTICATE PLAIN` with `+ ` and `plain_accepts`; `fake_smtp::{Script, serve}` has `auth_reply`, EHLO `AUTH PLAIN LOGIN`, `Transcript` | `envio-net/tests/support/` | read the AUTHENTICATE and EHLO arms |
| Stalwart v1.0.3 image is local; `/usr/local/bin/stalwart` contains `XOAUTH2` and `OAUTHBEARER` strings | `docker run --rm --entrypoint grep stalwartlabs/stalwart:v1.0.3 -c -a XOAUTH2 /usr/local/bin/stalwart` gave 5 | only the strings; configuration unverified (T1 probe) |
| `Secrets` (`passwords.json`, `open/get/set/remove`, 0600, `write_atomic`, `Newer(u32)`, hand-written `Debug`) | `envio-core/src/secrets.rs`, `fsutil.rs:10` | read in full; `OAuthTokens` copies it |
| `Worker.store.secrets`, removal on account removal | `worker/store.rs:30,79`, `worker/remove.rs:69` | read |
| `login(account, secrets, service) -> Result<(Endpoint, Credentials), Problem>`, `connect_imap`, `connect_smtp`, `Worker::imap` | `worker/conn.rs:118-160, 218-240` | read |
| `begin_open` builds `login` before spawning the IDLE open future (`Job { login, .. }`) | `worker/open.rs:99-125` | read |
| `connect_smtp` use for the Outbox | `worker/run/mail.rs:25` | read the call |
| `Problem::{Temporary, SignIn, Certificate, Refused, DiskFull, Damaged, Local}`, `from_net` | `envio-core/src/problem.rs` | read |
| `Health::failed`: `Problem::SignIn` sets `AccountStatus::SignInFailed` and returns `Response::Pause`; `is_paused`, `resume` | `envio-core/src/health.rs:118-175` | read |
| Status text `AccountStatus::SignInFailed => "Sign-in failed, update password"` | `envio-core/src/text/status.rs:26` | read |
| `after_send`: `SignIn` or `Certificate` gives `Queued`, `retry: false`, `stop: Some(kind)`; Temporary gives `Queued`, `retry: true`; `MayHaveBeenSent` | `envio-sync/src/plan/outbox.rs:70-100` | read |
| `Worker::update_password`, `resume`, `accept_certificate`, `status_of`, `report`, `emit` | `worker/trust.rs`, `worker/mod.rs:369-390` | read |
| `Command::{UpdatePassword, Connect, Detect}`, `ConnectRequest { setup, password, servers_confirmed, accepted }`, `AccountSetup`, `Event::{Detected, SetupProblem, TrustPrompt, AccountAdded, Notice}` | `api/command.rs`, `api/event.rs` | read |
| `save_account`, `setup_from(found, email)`, `check_request` (duplicate address), `SetupThread`/tickets | `worker/setup.rs`, `setup_thread.rs` | names and roles read; the body of `save_account` re-read in T7 |
| `core::Config { cache_dir, data_dir, roots, log_level, foreground }` | `envio-core/src/core.rs:26` | read; `oauth` is added here |
| `AccountSettings { name, email, username, imap, smtp }`, `Account`, `Server { host, port, security, pin }`; `FileIn` reads accounts as `serde_json::Value` entries (a broken entry is kept raw) | `envio-store/src/accounts.rs:31-100` | read; no `deny_unknown_fields` anywhere in `envio-store/src` (grep) |
| `AccountInfo { key, name, label, email, status, unread, broken }` | `envio-core/src/api/view.rs:16` | read |
| QML: `WizardPage` steps (`email`, `signin`, `servers`, `options`), `onDetected(email, text, needsConfirm, json, serversText)`, `current(email)`, `forget()`, `SignInStep` (`appPassword`, `password`, `foundText`), `UpdatePasswordPage`, `AccountsPage` action sheet rows (`updatePasswordRow`), `AppShell` `onUpdatePasswordRequested: stack.push(updatePasswordPage, ..)` | `qml/pages/accounts/*.qml`, `qml/pages/AppShell.qml:257-262` | read |
| `Platform.Uri { schemes, onOpened(uri), open(url), opener }`; cold-start links come from `Qt.application.arguments`; running-app links from `ut/UriSource.qml` | `qml/bindle/Platform/Uri.qml`, `AppShell.qml:111` | read; AppShell already instantiates one with `["mailto"]` |
| `envio.url-dispatcher` is `[{"protocol": "mailto"}]`; `envio.desktop` has `Exec=envio %U`; `manifest.json` `hooks.envio.urls`; `clickable.yaml` installs `envio.url-dispatcher` | repo root | read |
| `envio.apparmor` policy groups: connectivity, content_exchange, content_exchange_source, networking, push-notification-client; no change needed | repo root | read |
| Microsoft: scopes `https://outlook.office.com/IMAP.AccessAsUser.All`, `https://outlook.office.com/SMTP.Send`, `offline_access`; the XOAUTH2 string; IMAP `NO AUTHENTICATE failed.` and SMTP `535 5.7.3` failure shapes; works for Outlook.com and Microsoft 365 | learn.microsoft.com, "Authenticate an IMAP, POP or SMTP connection using OAuth" (fetched 2026-11-18) | read; the `+ <json>` challenge shape is Google's documented behaviour, kept as a defensive path |
| Microsoft redirect rules (https, `http://localhost`, port ignored, no query for personal accounts, custom schemes not mentioned) | learn.microsoft.com, "Redirect URI best practices and limitations" (fetched 2026-11-18) | read; custom scheme acceptance is open (T0, T10) |
| Not verified here (no phone, no registration): the Lomiri behaviours of T0, Microsoft's hosts `outlook.office365.com:993` and `smtp.office365.com:587` for personal accounts, `login.microsoftonline.com/common` serving personal accounts, the refresh-token rotation grace period | n/a | each is a checkbox in T0 or T10 |

## Open questions for the owner

1. Register the app (T10) now rather than after T9? It turns the custom-scheme question from "unverified" into a fact in ten minutes and could change the primary flow, so earlier is cheaper.
2. Is a desktop build in scope for Microsoft sign-in? A scheme handler on desktop needs a `.desktop` `MimeType` entry; the alternative is the loopback redirect, which is only needed there.
3. Publisher verification (Microsoft Partner Network ID): wanted? Without it, organizations that restrict user consent show an "unverified" warning and some will block Envio.
4. Decided in the plan (decision 10): an Authenticated-SMTP-off tenant gives a Failed Outbox item with the server's text and a hint, and the account keeps syncing. Say so if you would rather have the bare server text.
