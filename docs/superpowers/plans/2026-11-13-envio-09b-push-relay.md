# Envio Plan 9b: self-hosted push relay

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A single-owner relay that runs IDLE on its owner's Inboxes and
wakes the phone through push.ubports.com while Envio is suspended or
closed. The push carries no sender, subject or address (#54).

**Architecture:**
- `crates/envio-relay` is a bin built on `envio-net`. It has a pure
  wake → push planner, a thin IMAP and HTTP driver, a small HTTPS pairing
  API (behind the owner's reverse proxy), and one SQLite file with
  passwords encrypted at rest.
- The app side has a PushClient token in `qml/platform/ut/`, a pairing
  flow in Settings → Notifications, and the 9a push helper's generic card.

**Tech Stack:** Rust (tokio, envio-net IMAP, an HTTP server crate and an
HTTP client with rustls, rusqlite, chacha20poly1305), a Docker image
pushed by Forgejo Actions, QML `Lomiri.PushNotifications`, Stalwart in
Docker, and a fake push endpoint for tests.

**Spec:** `docs/superpowers/specs/2026-11-13-envio-notifications-design.md`
(section "9b"). Issue: #54. Depends on 9a Task 5 (push helper).

## Global Constraints

- The relay depends on `envio-net` only, and nothing in the app depends on the relay.
- The relay never FETCHes; it only issues IDLE and `STATUS INBOX (UIDNEXT UNSEEN)`. The push `data` holds only `{a: account key, n: new count, u: unseen}`.
- Single owner: pairing needs a one-time code made on the server. There is no sign-up endpoint.
- Passwords are encrypted at rest with a key from `ENVIO_RELAY_KEY_FILE`, and the relay refuses to start without it. Unpair deletes credentials at once.
- Logs carry device ids and account keys only.
- The app talks to the relay over HTTPS only. A self-signed relay goes through the fingerprint prompt.
- Testing tier is elevated (it sends outside the process) and maximum (crypto and new dependencies), so it gets a trunk review and an audit.
- Each task runs the proving commands it names, and `cargo make ci` passes at every commit.

## Plan series

9a. Local notifications. **9b. Self-hosted push relay** (this plan).

---

### Task 1: Spike on the phone (no merge)

- A QML `PushClient` in a throwaway build: check that a token appears
  without a login.
- POST one push by hand from the host to push.ubports.com with that
  token, and check the 9a helper shows the generic card.
- Check whether the helper profile can read
  `~/.local/share/envio.aps/notify-names.json`.

Record the results in `.superpowers/sdd/09b/spike.md` and on #54. If no
token appears, stop: the relay can't work on this image.

### Task 2: Wake → push planner (pure)

**Files:** create `crates/envio-relay/src/plan.rs`.

**Produces:** `plan::on_status(prev: Option<Status>, now: Status,
last_push: Option<Instant>, at: Instant) -> Action { Push { n, u },
Defer(until), Nothing }`, with a 30 s debounce. `plan::on_push_error(e)
-> { DropDevice, RetryClearPending, Backoff }`.

**Tests:**
- `first_status_never_pushes`;
- `uidnext_rise_pushes_the_delta`;
- `a_second_rise_within_30s_is_deferred_and_folded`;
- `unknown_token_drops_the_device`;
- `too_many_pending_retries_once_with_clear_pending`.

### Task 3: Store and crypto

**Files:** create `crates/envio-relay/src/store.rs` (SQLite: devices,
accounts, pair codes) and `crates/envio-relay/src/seal.rs`
(XChaCha20-Poly1305, with a random nonce per record).

**Tests:**
- `a_password_round_trips`;
- `the_file_holds_no_plaintext_password` (grep the db bytes);
- `a_wrong_key_fails_to_open`;
- `unpair_deletes_accounts_and_token`;
- `a_pair_code_works_once_and_expires_after_10_minutes`.

### Task 4: IMAP watcher and push sender (drivers)

**Files:** create `crates/envio-relay/src/watch.rs` (one task per
account: connect with envio-net, IDLE on INBOX re-issued ≤ 29 min, STATUS
on wake, backoff on errors) and `crates/envio-relay/src/push.rs` (POST to
the configured push URL, default `https://push.ubports.com/notify`).

**Tests:**
- Integration with Stalwart and a fake push server (in `envio-relay/tests/`):
  - deliver mail, and check one POST arrives whose JSON has the exact
    shape;
  - `no_address_or_subject_in_any_post` (search the captured bodies for
    the test sender, recipient and subject);
  - a burst of 5 gives one POST plus one folded POST;
  - after the server restarts, the watcher reconnects.
- Unit: `the_watcher_never_sends_fetch`, a scripted IMAP transcript in
  which any FETCH fails the test.

### Task 5: Pairing API and CLI

**Files:** create `crates/envio-relay/src/api.rs` (`POST /pair`,
`PUT /device`, `DELETE /device`; device-secret auth with a constant-time
compare) and `src/main.rs` (`serve`, `pair`, `list`, `unpair <device>`).

**Tests:**
- `pair_with_a_bad_code_is_refused`;
- `pair_returns_a_secret_once`;
- `put_device_with_a_wrong_secret_is_401`;
- `delete_device_wipes_it`;
- `no_endpoint_returns_a_password`.

### Task 6: Container image and docs

**Files:**
- create `relay/Dockerfile` (a static build on a distroless base);
- create `.forgejo/workflows/relay.yml` (build on PRs, push to the
  registry on master and on tags);
- create `docs/relay.md` (run, reverse proxy with HTTPS, key file,
  backup, `pair`, "This relay is for you alone").

**Proof:** the image builds in CI, `docker run … pair` prints a code, and
a run against Stalwart passes the Task 4 test inside the image.

### Task 7: App side

**Files:**
- create `qml/platform/ut/PushToken.qml` (`PushClient`; token changes go
  to core);
- core: `relay::client` (pair, put token, unpair) over HTTPS with the
  existing pin rules; the device secret is stored in the passwords file;
- in `NotificationsPage.qml`, a "Push relay" section with URL, code, the
  per-account choice, the password warning, and an optional relay
  password field;
- if Task 1 found the helper can read it, create `notify-names.json`
  (account key → name).

**Tests:**
- core unit tests against a mock relay: `pair_sends_the_token_and_chosen_accounts_only`,
  `a_token_change_is_sent_with_the_secret`, `unpair_forgets_the_secret`;
- QML: `the_password_warning_shows_before_pairing`.

### Task 8: End-to-end on the phone

1. Run the relay on the host with Stalwart and a synthetic account, and
   expose it through adb reverse behind a self-signed certificate (the
   prompt flow).
2. Pair, close Envio, deliver mail, and check that a card arrives through
   push.ubports.com within 30 s. Take a shot.
3. Unpair, and check the relay DB has no rows for the device.
4. Run `auditing-before-launch` before the owner deploys a relay on the
   Scaleway box. That deploy is the owner's call.
