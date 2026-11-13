# Envio Plan 9a: notify new mail while Envio runs

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A system notification and launcher badge for new Inbox mail
while Envio runs, in the foreground or exempted in the background (#144).

**Architecture:**
- A pure rule in `envio-sync` spots new mail during an Inbox sync, and the
  worker emits `Event::NewMail`.
- A pure planner in `envio-core` turns that event into a `Notice`, using
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

### Task 1: Spike on the phone (no merge)

Throwaway branch. Add the `push` hook, a shell push helper that copies
infile to outfile, and the `push-notification-client` group. Then:
- `gdbus call` `Post` with a final card JSON, and check that the card
  shows;
- `SetCounter`, and check the badge shows;
- from inside the app's confinement, try reading
  `com.canonical.qtmir lifecycle-exempt-appids`.

Record the results in `.superpowers/sdd/09a/spike.md` and in a comment on
#144. If `Post` doesn't show a card, stop and rule (fallback:
`org.freedesktop.Notifications` through `Lomiri.Notifications`).

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

### Task 3: `Event::NewMail` and the notice planner (`envio-core`)

**Files:** `api/event.rs` (new variant), `worker/` (emit after an Inbox
sync with `count > 0`); create `notice/plan.rs` and `notice/mod.rs`
(`Notice`, `Poster` trait); add settings keys `notify.<account>` (default
true) and `notify.show_text` (default false).

**Produces:**
- `notice::plan(ev: &NewMail, account_name: &str, s: &NoticeSettings,
  focused: bool) -> Option<Notice>`;
- `Notice { tag: String, summary: String, body: String, account: AccountKey }`;
- `trait Poster: Send { fn post(&self, n: &Notice); fn badge(&self, total_unread: u32); }`;
- the core calls `badge` on every `AccountUnread` change, with the sum.

**Tests:**
- `switch_off_means_no_notice`;
- `focused_means_no_notice`;
- `many_shows_account_and_count` (uses `text::count::new_mail`);
- `one_with_text_off_shows_account_and_count`;
- `one_with_text_on_shows_sender_and_subject`;
- `the_tag_is_per_account`;
- e2e in `envio-e2e`: with Stalwart, the app in the background with IDLE
  up, deliver 3 messages within 1 s, and check the recording poster gets
  one notice per wake with the right count and no text.

### Task 4: Keep Inbox IDLE in the background

**Files:** `crates/envio-core/src/worker/conn.rs` (the background branch
of `idle_done`) and its tests.

**Invariant:** in the background, an account with `notify.<account>` on
keeps IDLE on Inbox and reconnects on resume as today. Other folders, and
accounts with notifications off, close as today.

**Tests:**
- `background_keeps_inbox_idle_when_notify_is_on`;
- `background_closes_when_notify_is_off`;
- `resume_after_a_dead_socket_reconnects` (existing, unchanged).

### Task 5: Push helper crate and click hooks

**Files:**
- create `crates/envio-push-helper/` (bin, files under 150 lines);
- create `push/push-helper.json` and `push/push-apparmor.json`;
- modify `manifest.json` (the `push` hook), `envio.apparmor` (add
  `push-notification-client`), `clickable.yaml` (`install_root_data`), and
  `CMakeLists.txt` (install the helper binary);
- modify `.forgejo/` only if the click job needs the new bin.

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
shape comes from a Rust `Notice::to_postal_json()` in core (unit-tested).

**Tests:**
- `to_postal_json` unit test (tag, card summary, body, popup, persist);
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
  If Task 1 found that the exemption list is readable, it also shows the
  current state.

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
4. Tap the card, and check that the Inbox opens.
5. Check the badge equals the Inbox unread count.
6. Without the exemption, check that no card shows while suspended and a
   card shows on resume only if Envio is not focused (expected: none).

Check every shot for private data before it goes in the PR.
