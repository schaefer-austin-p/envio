# Envio Plan 10: Meeting invites Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A message with an iMIP invite shows an invite card in the reader; Accept, Maybe or Decline sends a `METHOD:REPLY` through the Outbox with 5 s Undo and an optional note; the answer is remembered; updates, cancellations and received answers show on the card; "Add to calendar" hands the `.ics` to Calendar if a phone check shows it works (#200).

**Architecture:** Two PRs from two worktrees, one per CI runner. **A-core** (`plan10-core`: A1–A6) is all Rust plus the `qt/` glue: the parser and reply builder in `envio-mime`, a held Outbox item and `invites.json` in `envio-store`, the hold in the `envio-sync` planner, the commands and the card view in `envio-core`, the pass-through in `envio-ui` and `qt/`. **B-ui** (`plan10-ui`: B1–B4) is the design board, the QML card against `MockCore`, the reader toast and the Content Hub calendar mode; it codes against the JSON contract below and merges after A-core. B5 (phone) runs when both are merged.

**Tech Stack:** Rust (envio-mime, envio-store, envio-sync, envio-core, envio-ui, envio-e2e), `jiff` (new; features `tz-system`, `tzdb-bundle-platform`), cxx-qt in `qt/`, Qt 5.15 / QtQuick 2.15, qmltestrunner offscreen, Stalwart in Docker; `ute2e` on the phone.

**Spec:** `docs/superpowers/specs/2026-11-14-envio-invites-design.md` (and the parent spec's "0.2.0"). Issue #200.

## Global Constraints

From the spec and `CLAUDE.md`, verbatim where quoted:

- "Logic in Rust, Qt and QML only for UI and platform APIs. The core is Qt-free; `unsafe` only in `qt/`."
- "Dependencies point one way: ui → core → sync → net, store, mime. net, store and mime never depend on each other."
- "One concept per module, files around 300 lines, logic kept out of I/O code (pure planners, thin drivers)."
- "Unsent work (Outbox, drafts, pending actions) lives outside the SQLite cache and is saved within about 1 s." The reply is enqueued at once; only its send is held.
- "User-facing text is translated: `qsTr()` in QML, gettext in Rust, domain `envio.aps`. Run `cargo make pot` after changing strings." Rust picks and translates all state and subject text; QML only shows it.
- Invite input is untrusted: parts over 256 KiB or 10 000 lines give no card; nothing in the parser may panic (proptest).
- Times cross into QML as Unix seconds (timed) or `{y,m,d}` (all-day); QML formats them like `HeaderBlock.qml:15`.
- Answers live in `invites.json` (user data, atomic write), never only in SQLite.
- No migration in this plan.
- Every QML `Text` states `textFormat`; colours, fonts, sizes from `Theme` roles. No swipe actions.
- Test mail and screenshots use synthetic accounts and `example.org` / `envio.test` only.
- `cargo make ci` passes at every commit; B tasks also `cargo make qml-test`; A6/B tasks touching `qt/` or `qml/` also `cargo make app-docker`.
- A-core never touches `qml/` or `tests/qml/`; B-ui never touches `crates/`.

## The card contract (shared by A6 and B2)

`message_opened` map gets `invite` (absent when there is no card):

| key | type | meaning |
|---|---|---|
| `method` | string | `request`, `cancel`, `reply` |
| `state` | string | `open`, `answered`, `updated`, `cancelled`, `replied` |
| `stateText` | string | translated line, e.g. "You accepted", "Updated", "Cancelled", "Grace accepted"; empty for `open` |
| `answer` | string | `accepted`, `tentative`, `declined` or empty |
| `sending` | bool | our reply is still in the Outbox |
| `summary`, `location` | string | sanitized plain text (may be empty) |
| `allDay` | bool | |
| `start`, `end` | int | Unix s (timed); 0 when all-day |
| `startDate`, `endDate` | string | `YYYY-MM-DD` when all-day (end exclusive, as in iCalendar), else empty |
| `zoneUnknown` | bool | the TZID resolved nowhere; shown as a note |
| `repeats` | bool | |
| `organizer` | string | display name, else address |
| `canAnswer` | bool | `request` with a `mailto:` organizer |
| `icsIndex` | int | attachment index of the calendar part, -1 if it is inline-only |

Calls: `core.answerInvite(id, answer, note)` (answer is one of the three strings) and `core.undoAnswer(id)`. Signals: `message_opened` again with the new card after an answer or undo (simplest: the core re-emits `MessageOpened` for that id), and `undo_offer` reused with a new undo kind `answer`.

## Parallel lanes

| Lane | Branch / worktree | Tasks | Starts |
|---|---|---|---|
| 1 | `plan10-core` | A1 → A2 → A3 → A4 → A5 → A6 | after the plan PR merges |
| 2 | `plan10-ui` | B1 → B2 → B3 → B4 | same time |
| 1 | `plan10-phone` (docs only) | B5 | both merged |

---

## Part A: core (one PR, branch `plan10-core`)

### A1: Parse invites (envio-mime)

**Files:** create `crates/envio-mime/src/invite/{mod.rs,lines.rs,event.rs,when.rs}`; modify `crates/envio-mime/src/lib.rs`, `message.rs` (field `invite`); tests `crates/envio-mime/tests/invite.rs`, corpus files `tests/corpus/invite-{google,outlook,apple,fastmail,thunderbird,cancel,reply,allday}.eml` (synthetic, `example.org`).

**Interfaces (produces):**
```rust
pub struct Invite {
    pub method: InviteMethod,            // Request | Cancel | Reply
    pub uid: String,
    pub sequence: u32,                   // missing = 0
    pub summary: String, pub location: String,   // unescaped, control chars stripped, ≤ 500 chars
    pub when: When,
    pub organizer: Option<Person>,       // only a mailto: value counts
    pub attendees: Vec<Person>,          // ≤ 500 kept
    pub repeats: bool,
    pub cancelled: bool,                 // METHOD:CANCEL or STATUS:CANCELLED
    pub raw: String,                     // the part as received (for the reply and for export)
}
pub enum InviteMethod { Request, Cancel, Reply }
pub struct Person { pub email: String, pub name: String, pub partstat: Option<PartStat> }
pub enum PartStat { Accepted, Tentative, Declined, NeedsAction }
pub enum When {
    Timed { start: i64, end: i64, zone_unknown: bool },   // Unix s; end = start if neither DTEND nor DURATION
    AllDay { start: (i32, u8, u8), end: (i32, u8, u8) },  // end exclusive; default start + 1 day
    Floating { start: NaiveParts, end: NaiveParts },       // local wall time, no zone
}
pub fn parse_invite(bytes: &[u8]) -> Option<Invite>;
// Message gains: pub invite: Option<Invite>, from the first text/calendar part
// (inline or attached) whose METHOD is REQUEST, CANCEL or REPLY.
```
`When::Timed` resolution goes through A2's `zones::resolve(tzid, local, vtimezones) -> Option<i64>`; A1 lands with UTC and `Z` only and a stub returning `None` (→ `zone_unknown: true`).

**Invariants:** unfolding per RFC 5545 §3.1 (CRLF or LF followed by space/tab); property names and parameter names case-insensitive; quoted parameter values may hold `;` `:` `,`; text escapes `\n \N \, \; \\`; only the first `VEVENT` counts; anything over the caps → `None`; never panics.

**Tests (names → what they assert):**
- `google_invite_reads_summary_time_organizer` — corpus Google: UTC times, organizer name+email, 2 attendees.
- `folded_lines_and_escapes_unfold` — a summary folded over 3 lines with `\,` and `\n`.
- `quoted_cn_with_colon_and_semicolon` — `CN="Doe; Jane: PM"`.
- `cancel_method_marks_cancelled`, `status_cancelled_marks_cancelled`.
- `reply_reads_attendee_partstat`.
- `all_day_uses_dates_and_default_end` — `VALUE=DATE`, no DTEND → next day.
- `duration_sets_end` — `DURATION:PT1H30M`.
- `floating_time_stays_wall_time`.
- `rrule_sets_repeats`.
- `no_uid_or_no_dtstart_gives_none`, `oversized_part_gives_none`.
- `parse_never_panics` (proptest over arbitrary bytes and over mutated corpus lines).
- `message_picks_first_calendar_part` — the existing `calendar-invite.eml` gives `Some`.

- [ ] Write the tests and corpus; run `cargo test -p envio-mime --test invite` (fail).
- [ ] Implement; tests pass; `cargo make ci`.
- [ ] Commit `feat(mime): read iMIP invites from text/calendar parts`.

### A2: Time zones (envio-mime)

**Files:** create `crates/envio-mime/src/invite/zones.rs`, `zones/windows.rs` (generated table), `tools/gen-windows-zones.py` (reads CLDR `windowsZones.xml`, writes the table; the generated file says so in its first line); modify workspace `Cargo.toml` + `crates/envio-mime/Cargo.toml` (`jiff` with `default-features = false, features = ["std", "tz-system", "tzdb-bundle-platform"]`); tests in `tests/invite.rs`.

**Interfaces:** `pub(crate) fn resolve(tzid: &str, local: NaiveParts, vtz: &[VTimezone]) -> Option<i64>`; `VTimezone { tzid: String, standard_offset_s: i32 }` filled by A1's reader from `BEGIN:VTIMEZONE` blocks.

**Order:** IANA name via `jiff::tz::TimeZone::get` (strip a leading `/` and Thunderbird's `/mozilla.org/.../` prefix) → Windows name via the table → the part's `VTIMEZONE` `STANDARD` `TZOFFSETTO` → `None`. A gap or fold in local time takes jiff's "compatible" disambiguation.

**Tests:** `iana_tzid_resolves_with_dst` (Europe/Berlin in July = UTC+2), `windows_tzid_resolves` ("W. Europe Standard Time"), `mozilla_prefixed_tzid_resolves`, `unknown_tzid_uses_vtimezone_offset`, `unknown_tzid_without_vtimezone_is_zone_unknown`, `outlook_corpus_reads_local_time` (the Outlook corpus file end to end).

- [ ] Tests (fail) → implement → pass; `cargo make ci` (audit and licence check accept jiff).
- [ ] Commit `feat(mime): resolve invite time zones (IANA, Windows, VTIMEZONE)`.

### A3: Build the reply (envio-mime)

**Files:** create `crates/envio-mime/src/invite/reply.rs`; modify `crates/envio-mime/src/build.rs` (field `calendar`); tests `tests/invite_reply.rs`, snapshot.

**Interfaces:**
```rust
pub struct CalendarPart { pub method: &'static str, pub ics: String }   // in build.rs
// Outgoing gains: pub calendar: Option<CalendarPart>  (Default = None; no other caller changes)
pub fn reply_ics(invite: &Invite, me: &str, answer: PartStat, note: &str, dtstamp_unix: i64) -> String;
pub fn reply_outgoing(original: &Parsed, invite: &Invite, from: Address,
                      answer: PartStat, note: &str, subject: String, text: String,
                      dtstamp_unix: i64) -> Outgoing;  // To organizer, In-Reply-To/References like reply.rs
```
With `calendar: Some`, `build` makes `multipart/alternative` (text, then `text/calendar; method=REPLY; charset=utf-8`, base64), plus `multipart/mixed` around it only if there are attachments.

**Tricky part (full code expected in the implementer's work, check in review):** `reply_ics` writes CRLF lines folded at 75 octets on UTF-8 boundaries; escapes text values; copies `UID`, `SEQUENCE`, `DTSTART` (verbatim line, with its `TZID` param), `ORGANIZER` (verbatim), `SUMMARY`; writes exactly one `ATTENDEE;PARTSTAT=…;CN=…:mailto:me` (the matching attendee's CN if one matched); `COMMENT` only when the note is non-empty; `PRODID:-//Envio//Envio 0.2//EN`; `METHOD:REPLY` both in the body and the content type.

**Tests:** `reply_has_method_in_body_and_header`, `reply_answers_matching_attendee_only`, `reply_adds_me_when_not_listed`, `note_goes_into_comment_and_text`, `long_lines_fold_at_75_octets_on_char_boundaries`, `reply_threads_to_the_invite`, `reply_snapshot` (insta), `outgoing_without_calendar_is_unchanged` (existing build snapshots stay byte-identical).

- [ ] Tests (fail) → implement → pass; `cargo make ci`.
- [ ] Commit `feat(mime): build an iMIP REPLY`.

### A4: Held Outbox items and remembered answers (envio-store, envio-sync)

**Files:** modify `crates/envio-store/src/outbox.rs` (`OutboxMeta.send_after_ms: i64`, `#[serde(default)]`; `NewOutboxItem.send_after_ms`), create `crates/envio-store/src/invites.rs`, modify `paths.rs` (`invites()`), `crates/envio-sync/src/plan/outbox.rs` (`item_step(meta: &OutboxMeta, now_ms: i64)`; held → `ItemStep::Wait`), its two callers in `driver/account.rs:70` and `driver/outbox.rs:41`; next-wake must include the earliest `send_after_ms` (find where the driver computes its next deadline and add it).

**Interfaces:**
```rust
pub struct InviteAnswer { pub account: String, pub uid: String, pub sequence: u32,
    pub answer: String /* accepted|tentative|declined */, pub answered_at_ms: i64,
    pub event_end_unix: i64, pub reply_message_id: String }
pub fn load_invites(path: &Path) -> Result<Vec<InviteAnswer>>;
pub fn record_invite(path: &Path, a: InviteAnswer, now_ms: i64) -> Result<()>;   // replaces same (account, uid); prunes ended > 30 days
pub fn forget_invite(path: &Path, account: &str, uid: &str) -> Result<()>;
```
**Tests:** `old_meta_without_send_after_reads_as_zero`, `held_item_waits_until_send_after` (planner), `driver_wakes_at_send_after`, `invites_round_trip`, `record_replaces_same_uid`, `record_prunes_old_events`, `forget_removes_only_that_uid`, `invites_file_written_atomically` (copy the contacts test pattern).

- [ ] Tests (fail) → implement → pass; `cargo make ci`.
- [ ] Commit `feat(store): held Outbox items and remembered invite answers`.

### A5: Answer and Undo (envio-core)

**Files:** create `crates/envio-core/src/worker/invite.rs`, `crates/envio-core/src/invite_card.rs` (pure: `fn card(invite: &Invite, remembered: Option<&InviteAnswer>, sending: bool, me: &str) -> InviteCard`), `crates/envio-core/src/text/invite.rs`; modify `api/command.rs` (`AnswerInvite { id: MessageRowId, answer: PartStat, note: String }`, `UndoAnswer { id: MessageRowId }`), `api/view.rs` (`MessageView.invite: Option<InviteCard>`), `worker/read.rs` (`render_view` fills `invite` — keep the change to a call into `invite_card::card`), `worker/dispatch.rs`; tests `crates/envio-core/tests/invite.rs`.

**Behaviour:** `AnswerInvite` → load the cached raw body, parse, refuse unless `canAnswer`; subject `text::invite::subject(answer, summary)` ("Accepted: …", "Tentative: …", "Declined: …"); default text when the note is empty (`text::invite::reply_text(answer, name)`); `reply_outgoing` → `build` → `outbox.enqueue` with `send_after_ms = now + envio_sync::UNDO_MS` → `record_invite` (with the built message id) → `UndoOffer` (kind answer, `expires_at_ms` = send_after) → re-emit `MessageOpened`. `UndoAnswer` → delete the Outbox item if still `Queued` and held, `forget_invite`, `UndoClosed`, re-emit. Undo after the hold passed is a no-op with `UndoClosed { undone: false }`. `InviteCard.state`: per the spec's "What the card shows"; `sending` = the remembered `reply_message_id` is still in the Outbox.

**Tests:** `answer_enqueues_a_held_reply_to_the_organizer`, `answer_remembers_and_card_says_you_accepted`, `undo_within_hold_deletes_reply_and_answer`, `undo_after_hold_does_nothing`, `higher_sequence_shows_updated`, `cancel_card_has_no_buttons`, `reply_card_names_the_attendee`, `no_organizer_cannot_answer`, `card_unit_tests` in `invite_card.rs` for every state.

- [ ] Tests (fail) → implement → pass; `cargo make pot`; `cargo make ci`.
- [ ] Commit `feat(core): answer invites through the Outbox with Undo`.

### A6: Pass-through and the e2e reply (envio-ui, qt, envio-e2e)

**Files:** modify `crates/envio-ui/src/convert.rs`, `qt/src/json.rs` (`invite` map per the contract), `qt/src/core_object.rs` + `qt/src/calls.rs` (`answerInvite`, `undoAnswer`); create `crates/envio-e2e/tests/invite.rs`.

**Tests:** envio-ui `message_opened_carries_the_invite_card`; qt json unit test `invite_map_has_contract_keys` (every key in the contract table, types right); e2e `invite_reply_reaches_the_organizer` — grace sends an invite to heidi on Stalwart, heidi's core answers Accept, after the hold the organizer's INBOX has a message whose `text/calendar` part has `METHOD:REPLY` and `PARTSTAT=ACCEPTED`.

- [ ] Tests (fail) → implement → pass; `cargo make ci`; `cargo make app-docker`.
- [ ] Commit `feat(qt): invite card and answer calls reach QML`; open the A-core PR via `shipping-a-pr`.

---

## Part B: UI (one PR, branch `plan10-ui`)

### B1: The invite card board

**Files:** modify `docs/design/carbon-reading.html`, `docs/design/airmail-reading.html` (an invite card above the body: request open with note field, answered, cancelled), `docs/design/README.md` (drop "still to add"), `docs/10-design.md` (Reader row mentions the card).

Card in Carbon: `surface` card, `cardRadius`, eyebrow "INVITATION" (or "UPDATED", "CANCELLED"), summary in heading, date line, location, organizer; three equal buttons (Accept primary, Maybe, Decline); "Add a note" text link; "Add to calendar" text link. Airmail: the same content on paper with the airmail stripe treatment its other cards use. Light and dark.

- [ ] Draw; check both themes and dark in a browser screenshot (Playwright) at 360 px wide.
- [ ] Commit `docs(design): invite card board`.

### B2: InviteCard.qml

**Files:** create `qml/pages/reader/InviteCard.qml`; modify `qml/pages/reader/ReaderPage.qml` (card above `AttachmentList`, visible when `view.invite`), `qml/qml.qrc`, `tests/qml/MockCore.qml` (`answerInvite`, `undoAnswer` recorders); create `tests/qml/tst_invite_card.qml`.

**Behaviour:** dates: timed → `new Date(start*1000).toLocaleString(Qt.locale(), Locale.ShortFormat)` and the end time (date omitted when the same day); all-day → `Date.fromLocaleString(…, "yyyy-MM-dd")` shown with `Locale.ShortFormat` date only, end shown as end − 1 day when longer than one day. Buttons call `core.answerInvite(view.id, answer, note.text)` and clear the note. "Change" shows the buttons again. `sending` shows a small "Sending…" (`qsTr`). Strike through the summary when `state === "cancelled"`.

**Tests:** `test_open_request_shows_three_buttons`, `test_accept_calls_core_with_note`, `test_answered_shows_state_text_and_change`, `test_change_shows_buttons_again`, `test_cancelled_has_no_buttons`, `test_reply_has_no_buttons`, `test_all_day_shows_dates_only`, `test_no_invite_hides_card`.

- [ ] Tests (fail) → implement → pass with `cargo make qml-test`.
- [ ] Commit `feat(qml): invite card in the reader`.

### B3: Undo toast in the reader

**Files:** modify `qml/pages/reader/ReaderPage.qml` (a `Toast` like `InboxPage.qml:210-253`, reacting to `undo_offer`/`undo_closed` for kind `answer`); tests in `tst_reader.qml`: `test_answer_shows_undo_toast`, `test_undo_calls_core_undo_answer`.

- [ ] Tests (fail) → implement → pass; commit `feat(qml): Undo for an invite answer`.

### B4: Add to calendar

**Files:** modify `qml/platform/ut/ContentExport.qml` (mode `calendar`: `ContentType.Events`, `ContentHandler.Destination`), `qml/platform/Content.qml` (passes the mode), `InviteCard.qml` ("Add to calendar" → `core.saveAttachment(id, icsIndex, attachmentsDir)` then export with mode `calendar`; hidden when `icsIndex < 0` or on desktop where `Content` reports no hub); test `test_add_to_calendar_saves_the_ics` in `tst_invite_card.qml`.

- [ ] Tests (fail) → implement → pass; `cargo make app-docker`.
- [ ] Commit `feat(qml): add an invite to Calendar through Content Hub`; open the B-ui PR via `shipping-a-pr` after A-core merges (merge `origin/master` in first).

---

## B5: Phone check (after both PRs merge, branch `plan10-phone`)

1. `cargo make click-install`. On the synthetic account, send heidi invites from the corpus (Google, Outlook with a Windows zone, all-day, cancel) through Stalwart.
2. Open each; shoot Carbon and Airmail, light and dark (`ute2e shot`), into `docs/phone/0.2.0/` (shrunk).
3. Accept one; check the Undo toast; let it go; check the organizer received the reply (Stalwart).
4. **Calendar import:** tap "Add to calendar", pick Calendar. If the event appears in Calendar, keep the button and write the answer into the parent spec's "Answered on the phone" (replace the "likely yes, untested" line). If Calendar does not import it, remove the link in a follow-up commit on this branch (keep `ContentExport`'s mode out too) and record the "no" there; the attachment's Share stays the way.
5. Any defect found → a Forgejo issue (`bug`, moscow label, milestone Release 0.2.0).
6. Commit `docs: invite phone check for 0.2.0`; PR via `shipping-a-pr` (leaf).
