# Envio new-mail notifications design

Plan 9. Issues: #144 (notify while Envio runs), #54 (push relay while Envio
is closed). Builds on the main spec,
`docs/superpowers/specs/2026-11-07-envio-design.md`, whose privacy rules
still apply. Research notes behind this design (phone checks and upstream
sources) are summarised under "Platform facts".

## Goals and non-goals

Goals:

- A system notification and a launcher badge when new mail reaches an
  Inbox, for anyone, with no server: Envio notifies while it runs, and
  keeps running in the background if the user exempts it from suspension
  (9a).
- An optional relay a user runs for themselves. It wakes the phone through
  the UBports push service while Envio is suspended or closed (9b).
- No sender, subject or preview leaves the phone, and none passes through
  the relay or the push service.

Non-goals:

- **A relay we run for other people.** A shared relay would hold
  strangers' mail passwords: it becomes a breach target, makes the
  operator a data controller under the GDPR, and costs money with no way
  to recover it. The relay is single-owner. It has no sign-up, and it
  serves only devices its owner paired by hand. The protocol leaves a
  hosted relay possible later, but nothing in Plan 9 builds or plans one.
- Notifications for folders other than Inbox, VIP or rules, quiet hours
  (the system's Do Not Disturb applies), and notification actions
  (archive or reply from the card).

## Platform facts

Each fact was checked on the phone (Ubuntu Touch 24.04.4,
lomiri-push-service 0.100.1) unless it is marked as coming from source.

- `lomiri-push-service` owns `com.lomiri.Postal` and
  `com.lomiri.PushNotifications` on the session bus. An app may only use
  its own path, `/com/lomiri/Postal/envio_2eaps`, and needs the
  `push-notification-client` policy group.
- `Post(appid, json)` hands the JSON to the app's **push helper**, which
  writes the card. `SetCounter(appid, count, visible)` sets the launcher
  badge. Both method signatures come from source.
- The push helper is a click hook (`"push": {"apparmor": …, "push-helper":
  …}`, helper json `{"exec": "<path>"}`, AppArmor template
  `ubuntu-push-helper`; `exec` is relative to the helper json's own
  directory, and the profile can't run other programs such as `cp`). It
  runs with argv `infile outfile` and must finish
  within 5 s (from source), under its own AppArmor profile, so it cannot
  read Envio's data. TELEports ships a native helper on this phone.
- The service drops a card while the app is focused. It honours the
  user's per-app notification, sound and vibration settings.
- Suspension: an app stays alive in the background only if its package
  name is in `com.canonical.qtmir lifecycle-exempt-appids`. The user sets
  this, either with UT Tweak Tool ("Prevent app suspension") or with
  `gsettings` in the Terminal. A confined app cannot set it itself.
  Confirmed on the phone 2026-11-18: the entry is `'envio.aps'` (package
  name, like `'music.ubports'`), and the exemption takes effect at once.
  UT Tweak Tool path: Apps + Scopes, Envio, Details, General, "Prevent app
  suspension" (read from the tool's QML; it asks for the passcode).
- Remote push: `POST https://push.ubports.com/notify` with `appid`
  (`envio.aps_envio`), `token`, `expire_on`, and the optional
  `replace_tag`, `clear_pending` and `data`. `data` may be at most 4 KiB
  (from source). The token comes from QML `PushClient` and needs no
  account login (from source). A `400 unknown-token` means the device
  re-registered.

## 9a: notify while Envio runs (#144)

### What the user sees

- When new mail arrives in an account's Inbox while Envio is not on
  screen, a card appears: summary "‹account name›", body "3 new messages"
  (gettext plural, `text::count::new_mail`). Bursts collapse: the card
  uses tag `new-‹account key›`, so a later card replaces the earlier one.
  Postal does not replace by tag on a local `Post` (phone run, 2026-11-18),
  so the poster calls `ClearPersistent(app, tag)` first and posts when that
  call has finished. Tapping the card brings Envio to the front (an
  `appid://` card action); it opens where the user left it.
- The launcher badge shows the total Inbox unread count across accounts.
  It is cleared when the count is 0.
- Settings → Notifications:
  - one switch per account (default on);
  - "Show sender and subject" (default **off**). When it is on, the card
    for a single new message shows "‹sender name›" and "‹subject›". This
    stays on the phone, so it doesn't break the privacy rule, but the lock
    screen would show it, so it is opt-in;
  - "Keep checking in the background", which explains suspension: Envio is
    paused when not on screen unless it is exempted, with the two ways to
    exempt it (UT Tweak Tool, or the Terminal command with the existing
    list kept). Envio can't read the exemption list under confinement, so
    this is instructions only (9a.T1 confirmed: under confinement the read
    returns a wrong value).

### How it works

- **New-mail detection lives in the sync step, not the UI.** When an Inbox
  sync stores messages whose UIDs are at or above the folder's previous
  `UIDNEXT`, and they are not `\Seen` and not sent by one of the user's
  own addresses, the worker emits
  `Event::NewMail { account, count, first: Option<NoticeText> }`.
  `first` is filled only when `count == 1` and only with the sender
  display name and subject. A rise in `AccountUnread` is *not* used: it
  also rises when the user marks mail unread on another device. The rule
  is a pure function in `envio-sync`, unit-tested without I/O.
- **The first sync after adding an account never notifies** (no previous
  `UIDNEXT`), and neither does a UIDVALIDITY reset.
- **A `Notifier` in core decides, and a platform poster shows.**
  `envio-core` gets `alert::plan(event, settings, focused) ->
  Option<Alert>` (pure: account switch, text choice, tag) and a trait
  `Poster { fn post(&self, a: &Alert); fn badge(&self, count: u32); }`.
  The Ubuntu Touch poster lives in `qt/` and calls `com.lomiri.Postal`
  through QtDBus. It is platform glue only, with no decisions in it. The
  core stays Qt-free; tests use a recording poster.
- **The push helper** is a small Rust binary,
  `crates/envio-push-helper` (bin, depends on nothing in the workspace
  except a shared `notice` wire type it defines itself). It reads the input
  JSON. If the input already has a `notification`, which is how a local
  `Post` arrives, it copies it to the output unchanged. Otherwise it builds
  the generic card (9b). It does no I/O beyond the two files and finishes
  in milliseconds.
- **Background IDLE.** Today the worker closes the connection after IDLE
  when Envio is in the background (`worker/conn.rs`). In 9a an account
  with notifications on keeps IDLE on Inbox in the background. If Envio is
  suspended, the socket dies and the existing reconnect brings it back on
  resume; if Envio is exempted, IDLE keeps working. Other folders still
  stop in the background.
- On resume, Envio runs the normal Inbox sync. New mail found then is
  notified only if Envio is still not focused, so opening the app never
  pops a card for mail you are looking at.

### Privacy

- The card text is built on the phone. Nothing about the notification
  leaves the device.
- Logs record "notified account ‹key› count N" only, never names or
  subjects (`mask::*`).

## 9b: a self-hosted relay (#54)

### Shape

- `crates/envio-relay`: a Rust binary that depends on `envio-net` only,
  for IMAP with the same TLS and pin rules. It is shipped as a container
  image built in CI and pushed to the Forgejo registry
  (`code.austinschaefer.com/schaefera/envio-relay`), with a
  `docs/relay.md` that covers Docker, a reverse proxy for HTTPS, and
  backups.
- One relay serves its owner's devices only. The owner creates a pairing
  code on the server (`envio-relay pair` prints a one-time code, valid 10
  min). There is no account system and no public sign-up.

### Pairing (Envio → relay, HTTPS only)

1. In Settings → Notifications → "Push relay", the user enters the relay
   URL and the pairing code. Envio checks the relay's certificate against
   the system roots. A self-signed relay goes through the same
   fingerprint prompt as IMAP.
2. Per account, the user ticks which accounts the relay watches, and sees
   in plain words: "The relay will store this account's password. Use an
   app password if your provider offers one." Envio offers a separate
   password field for the relay, prefilled with nothing.
3. Envio POSTs `{code, push_token, accounts: [{key, host, port, user,
   password}]}`. The relay answers `{device_id, device_secret}`. Envio
   keeps the secret next to its passwords (same file, same 0600 rule).
4. Whenever `PushClient.token` changes, Envio sends `PUT /device` with the
   new token, authenticated by the secret. "Unpair" sends `DELETE /device`,
   and the relay deletes that device's credentials and token at once.

### What the relay does

- For each account, it keeps one IMAP connection that runs IDLE on INBOX,
  re-issued every ≤ 29 min (RFC 2177). It reconnects with backoff.
- On a wake, it runs **only `STATUS INBOX (UIDNEXT UNSEEN)`**. It never
  FETCHes headers or bodies, so it never sees a sender or subject. If
  `UIDNEXT` rose, it POSTs to push.ubports.com:
  `{"appid":"envio.aps_envio","token":…,"expire_on":now+6h,
  "replace_tag":"new-‹key›","data":{"message":{"a":"‹key›","n":‹UIDNEXT
  delta›,"u":‹UNSEEN›}}}`. The account key is Envio's opaque local key,
  not an address.
- It sends at most one POST per account per 30 s, and folds bursts into
  the next one. On `unknown-token` it drops the device. On
  `too-many-pending` it retries once with `clear_pending: true`.
- Storage is one SQLite file. Passwords are encrypted with a key from
  `ENVIO_RELAY_KEY_FILE` (XChaCha20-Poly1305), and the relay refuses to
  start without it. Logs carry device ids and account keys only.

### On the phone

- The push helper gets `data.message` without a `notification`, and
  writes the card from fixed gettext text: summary "Envio", body "New
  mail" plus the count. It sets `emblem-counter` to `u` and `popup: true`.
  It can't read Envio's data to name the account, and 9b.T1 checks whether
  the helper profile can read one small file Envio writes (account key →
  name). If it can, the summary becomes the account name.
- Tapping the card opens Envio, which syncs as usual.

### Threats this design accepts

- The relay holds IMAP passwords. Mitigations: single owner, app
  passwords encouraged, encryption at rest, deletion on unpair, no other
  data stored.
- UBports' push service learns that the device gets pushes from Envio and
  when. It learns no content.
- A stolen device secret lets someone change that device's push token. It
  does not reveal passwords, because the relay never returns them.

## Testing

- 9a:
  - pure-rule unit tests for new-mail detection (UID ≥ old UIDNEXT, seen,
    own address, first sync, UIDVALIDITY reset) and for `alert::plan`
    (switch off, focused, one versus many, privacy switch);
  - an e2e test with Stalwart: deliver mail during background IDLE, and
    check a recording poster gets one alert per burst;
  - QML tests for the settings page;
  - a phone check: a card shows while Envio is in the background and
    exempted, and the badge count is right.
- 9b:
  - relay unit tests (debounce, token drop, STATUS diff);
  - an integration test with Stalwart and a fake push endpoint, which
    checks the exact POST body and that no address or subject is in it;
  - the pairing and unpair API against a test relay;
  - a phone check with a real push through push.ubports.com and a
    disposable synthetic account.
- Testing tier: 9a is standard (local only). 9b is **elevated**: it sends
  outside the process, so its gate names push.ubports.com and reads the
  delivered card back on the phone. It is also **maximum** for its new
  crypto and dependency set, so it gets a trunk review and an audit.

## Open questions (each is the first task of its plan)

- 9a.T1 (answered 2026-11-18, see the 9a plan): a local `Post` shows a
  card and `SetCounter` a badge from inside confinement; the exemption
  list can't be read.
- 9b.T1: can the push helper read a file in Envio's data directory? Does
  `PushClient` from `Lomiri.PushNotifications 0.1` give a token without a
  login on this image?
