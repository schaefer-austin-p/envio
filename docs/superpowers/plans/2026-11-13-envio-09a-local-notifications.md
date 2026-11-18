# Envio Plan 9a: notify new mail while Envio runs

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A system notification and launcher badge for new Inbox mail
while Envio runs, in the foreground or exempted in the background (#144).

**Architecture:**
- A pure rule in `envio-sync` spots new mail during an Inbox sync, and the
  worker emits `Event::NewMail`.
- A pure planner in `envio-core` turns that event into an `Alert`, using
  the settings and focus.
- A thin `Poster` in `qt/` sends it to `com.lomiri.Postal`, and a tiny
  Rust push helper passes local posts through.
- IDLE on Inbox stays up in the background for accounts that have
  notifications on.

**Tech Stack:** Rust (envio-sync, envio-core, envio-ui, a new
`envio-push-helper` bin crate), cxx-qt and QtDBus in `qt/`, QML, and
clickable hooks with AppArmor on Ubuntu Touch 24.04, tested against
Stalwart in Docker.

**Spec:** `docs/superpowers/specs/2026-11-13-envio-notifications-design.md`
(section "9a"). Issue: #144.

## Global Constraints

- Logic in Rust, Qt and QML only for UI and platform APIs. The core is Qt-free; `unsafe` only in `qt/`.
- Dependencies point one way: ui → core → sync → net, store, mime. The push helper depends on no workspace crate.
- One concept per module, files around 300 lines, logic kept out of I/O code (pure planners, thin drivers).
- User-facing text is translated: `qsTr()` in QML, gettext in Rust, domain `envio.aps`. Run `cargo make pot` after changing strings. Card text is chosen and translated in Rust.
- Privacy: the card shows the account name and a count unless "Show sender and subject" is on (default off). Logs never carry names, addresses or subjects (`mask::*`).
- The first sync of an account and a UIDVALIDITY reset never notify.
- Each task runs the proving commands it names, and `cargo make ci` passes at every commit.

## Plan series

**9a. Local notifications** (this plan). 9b. Self-hosted push relay.

---

### Task 1: Spike on the phone (done 2026-11-18, no merge)

The released 0.2.0 click was repacked with a `push` hook, a shell helper
and `push-notification-client`, installed on the phone (UT 24.04.5,
lomiri-push-service 0.100.1), and driven with `gdbus` run under the app's
AppArmor profile (`aa-exec -p envio.aps_envio_<version>`). Results, also
on #144:
- `Post("envio.aps_envio", json)` on `/com/lomiri/Postal/envio_2eaps`
  works from inside confinement, and the card shows in the notification
  drawer with Envio's icon. While Envio is focused no card is shown.
- `SetCounter("envio.aps_envio", 7, true)` shows a 7 badge on Envio's
  launcher icon.
- The helper json's `exec` is resolved **relative to the helper json's
  own directory** (`push/push-helper.json` with `exec:
  "push/push-helper.sh"` looked for `push/push/push-helper.sh`).
- The `ubuntu-push-helper` profile denies executing `/bin/cp`, so the
  helper must not shell out; a native binary (Task 5) is the way.
- On a failing helper the service logs "unable to get helper output;
  putting payload into message" and shows nothing.
- Reading `lifecycle-exempt-appids` under confinement returns a wrong
  value (`['music.ubports']` while the real list has 7 entries, with a
  dconf permission error), so the exemption state cannot be read: the
  Settings page gives instructions only.
- A card action `appid://envio.aps/envio/current-user-version` dispatched
  through `com.lomiri.URLDispatcher` brings Envio to the front. Tapping
  the card through ute2e's synthetic touch only closed the drawer, so the
  real tap is checked by hand in Task 8.
- The AppArmor profile is only regenerated when the click version
  changes; reinstalling the same version keeps the old policy.

### Task 2: New-mail rule (`envio-sync`)

**Files:** create `crates/envio-sync/src/newmail.rs`; wire it into the
Inbox sync step where new UIDs are stored.

**Produces:**
`newmail::count(prev_uidnext: Option<u32>, uidvalidity_changed: bool,
stored: &[NewRow], own: &[Address]) -> NewMail { count: u32, first:
Option<(String, String)> }`, where `NewRow { uid, seen, from: Address,
from_name, subject }`. `first` is set only when `count == 1`.

**Tests:**
- `no_previous_uidnext_never_notifies`;
- `a_uidvalidity_reset_never_notifies`;
- `seen_mail_is_not_new`;
- `mail_from_my_own_address_is_not_new`;
- `uids_below_the_old_uidnext_are_not_new`;
- `one_new_message_carries_sender_and_subject`;
- `three_new_messages_carry_no_text`.

### Task 3: `Event::NewMail` and the alert planner (`envio-core`)

**Files:** `api/event.rs` (new variant), `worker/` (emit after an Inbox
sync with `count > 0`); create `alert/plan.rs` and `alert/mod.rs`
(`Alert`, `Poster` trait; not `notice`, since `Event::Notice` already
means an in-app message); `crates/envio-store/src/userdata.rs` (trunk)
for the settings.

**Settings:** `envio_store::userdata::Settings` is one serde struct saved
whole by `Command::SaveSettings`, with `#[serde(default)]` on the struct.
Add two fields, no `SETTINGS_VERSION` bump (old files read with the
defaults):
- `notify_off: BTreeSet<String>`: account keys with notifications off
  (empty by default, so every account notifies);
- `notify_show_text: bool` (default false).

**Produces:**
- `alert::plan(ev: &NewMail, account_name: &str, s: &Settings,
  focused: bool) -> Option<Alert>`;
- `Alert { tag: String, summary: String, body: String, account: String }`
  (account key, as everywhere else in core);
- `trait Poster: Send { fn post(&self, a: &Alert); fn badge(&self, total_unread: u32); }`;
- the core calls `badge` on every `AccountUnread` change, with the sum.

**Tests (store):** `old_settings_files_read_with_notifications_on`,
`notify_fields_round_trip`.

**Tests:**
- `switch_off_means_no_alert`;
- `focused_means_no_alert`;
- `many_shows_account_and_count` (uses `text::count::new_mail`);
- `one_with_text_off_shows_account_and_count`;
- `one_with_text_on_shows_sender_and_subject`;
- `the_tag_is_per_account`;
- e2e in `envio-e2e`: with Stalwart, the app in the background with IDLE
  up, deliver 3 messages within 1 s, and check the recording poster gets
  one alert per wake with the right count and no text.

### Task 4: Keep Inbox IDLE in the background

**Files:** in `crates/envio-core/src/worker/`, every place that stops
work in the background today:
- `plan.rs`: `next` returns `Sleep` whenever `!timing.foreground`, so
  `begin_idle` is never reached. In the background it must still return
  an Inbox sync, then IDLE, for a notifying account (the `Timing` input
  gains which accounts notify);
- `conn.rs`: `app_active(false)` wakes every `Conn::Idling` and closes
  every `Conn::Ready`. Notifying accounts keep their Inbox IDLE;
- `run/mod.rs`: `keep_open` closes the session after any step but a Body
  one in the background. Keep it open for a notifying account's Inbox
  sync, so IDLE can follow;
- `turn.rs`: `needs_timer` counts only foreground accounts. Check whether
  IDLE renewal (`begin_idle`'s `until_ms`) still fires in the background,
  and make it.

**Invariants:**
- in the background, an account not in `notify_off` keeps IDLE on Inbox;
  a wake runs an Inbox-only sync (no other folders, no bodies) and goes
  back to IDLE;
- other folders, and accounts in `notify_off`, stop in the background as
  today;
- on resume, every connection is treated as possibly dead and reconnects
  (as today).

**Tests:**
- `plan.rs`: `background_plans_inbox_idle_for_a_notifying_account`,
  `background_sleeps_for_an_account_with_notify_off`,
  `background_wake_plans_an_inbox_only_sync`;
- `conn.rs`: `background_keeps_inbox_idle_when_notify_is_on`,
  `background_closes_when_notify_is_off`;
- `resume_after_a_dead_socket_reconnects` (existing, unchanged);
- e2e (`envio-e2e`, Stalwart): app in the background, notify on, deliver
  one message, and the recording poster gets one alert without any
  foreground switch.

### Task 5: Push helper crate and click hooks

**Files:**
- create `crates/envio-push-helper/` (bin, files under 150 lines);
- create `push/push-helper.json` and `push/push-apparmor.json`;
- modify `manifest.json` (the `push` hook), `envio.apparmor` (add
  `push-notification-client`), `clickable.yaml` (`install_root_data`), and
  `CMakeLists.txt` (install the helper binary);
- modify `.forgejo/` only if the click job needs the new bin.

The helper json's `exec` is relative to the json's own directory, and
the helper profile can't run other programs (Task 1).

**Produces:** `envio-push-helper <in> <out>`. If the input has
`notification`, it copies it to the output. Otherwise it writes the
generic card ("Envio" / "New mail", gettext) with `emblem-counter` from
`message.u` when present. It always exits within 5 s, and on bad input it
writes `{}`.

**Tests:**
- unit: `passes_a_local_notification_through`;
- `builds_the_generic_card_from_a_message`;
- `bad_input_writes_an_empty_object`;
- `cargo make click`, then check the helper json shows up in
  `~/.local/share/lomiri-push-service/helpers/` on the phone.

### Task 6: UT poster in `qt/`

**Files:** `qt/src/` (a QtDBus call to `com.lomiri.Postal` at
`/com/lomiri/Postal/envio_2eaps`: `Post("envio.aps_envio", json)` and
`SetCounter`), registered as the core's `Poster`. Glue only: the JSON
shape comes from a Rust `Alert::to_postal_json()` in core (unit-tested).
The card carries `actions: ["appid://envio.aps/envio/current-user-version"]`,
so a tap brings Envio to the front on its usual unified Inbox (Task 1
checked the URL). Opening one account's Inbox from the card is out of
scope: it needs a custom URL scheme in `envio.url-dispatcher`; it is #230.

**Tests:**
- `to_postal_json` unit test (tag, card summary, body, popup, persist,
  the appid action);
- the phone check is in Task 8.

### Task 7: Settings → Notifications page

**Files:**
- create `qml/pages/settings/NotificationsPage.qml`;
- add an entry in `SettingsPage.qml`;
- add a list model in `envio-ui` (accounts with switches);
- create `tests/qml/tst_notifications.qml`.

The page shows:
- per-account switches;
- "Show sender and subject";
- the "Keep checking in the background" explanation, with the Terminal
  command written out (keep existing entries) and the UT Tweak Tool route.
  Instructions only: Task 1 found the exemption list unreadable under
  confinement.

**Tests:**
- `toggling_an_account_writes_its_setting`;
- `the_text_switch_defaults_off`;
- `the_background_help_names_the_package_envio_aps`.

### Task 8: Phone run

On synthetic accounts, with Envio exempted (UT Tweak Tool):
1. Background Envio, deliver one message from the host to grace, and take
   a shot of the card (summary = account name).
2. Deliver three messages, and check one card that says 3 new messages.
3. Turn the text switch on, deliver one, and check sender and subject.
4. Tap the card by hand (ute2e's synthetic tap doesn't trigger card
   actions), and check that Envio comes to the front (where the user left it).
5. Check the badge equals the Inbox unread count.
6. Without the exemption, check that no card shows while suspended and a
   card shows on resume only if Envio is not focused (expected: none).

Check every shot for private data before it goes in the PR.

Run 2026-11-18: see "Notifications run (Plan 9a)" in `docs/phone-checklist.md`.
The exemption entry is `'envio.aps'` (package name), confirmed.
