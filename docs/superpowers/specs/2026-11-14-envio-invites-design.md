# Envio meeting invites (iMIP)

Date: 2026-11-14. Status: approved in brainstorming (owner: "go with your
recommendations"). Release: 0.2.0. Issue: #200. Parent spec:
`2026-11-07-envio-design.md` ("0.2.0").

## Goal

A message that carries an invite shows a card at the top of the reader: what,
when (in the phone's time), where, who organizes it, and Accept, Maybe and
Decline. One tap answers the organizer with a standard iMIP reply (RFC 6047,
`METHOD:REPLY`) through the Outbox, with Undo for 5 s and an optional note.
Envio remembers the answer, shows updates and cancellations on the card, and
shows answers that come back to an organizer.

## Scope

In:
- Reading `text/calendar` parts with `METHOD` `REQUEST`, `CANCEL` and `REPLY`
  (also a `.ics` attachment with a `METHOD` line).
- The first `VEVENT` of the part: `UID`, `SEQUENCE`, `SUMMARY`, `LOCATION`,
  `DTSTART`, `DTEND` or `DURATION`, `ORGANIZER`, `ATTENDEE` (address, `CN`,
  `PARTSTAT`), `STATUS`, and whether `RRULE` is present.
- Answering a `REQUEST` with Accept, Maybe or Decline; an optional note.
- "Add to calendar": hands the `.ics` to the Calendar app through Content Hub
  (`ContentType.Events`, destination). Kept only if the phone check in the
  plan shows Calendar 1.3.1 imports it; otherwise dropped, and the
  attachment's Share stays the way to do it.
- An invite card board for Carbon and Airmail in `docs/design/`.

Out (later or never): working out the dates of a repeating event (the card
says "Repeats"), `COUNTER`, `REFRESH`, `DECLINECOUNTER`, `ADD`, `PUBLISH`,
invites that Envio sends, more than one `VEVENT`, editing the event, reading
or writing the system calendar directly (needs a reserved policy group).

## Decisions

1. **Parser in envio-mime, written for this.** A module `invite/` reads the
   lines (unfolding, `;PARAM=` values, quoted values, text escapes) and keeps
   only the fields above. A general iCalendar crate brings far more than the
   card needs. Input is untrusted: a size cap (256 KiB), a line count cap, no
   panics; a broken part gives no card, never an error.
2. **Times are instants in Rust, words in QML.** Rust turns `DTSTART`/`DTEND`
   into Unix seconds (floating times travel as wall-time strings). QML formats them with `toLocaleString` the way
   `HeaderBlock.qml` formats the message date, so the phone's zone and
   locale apply. All-day events (`VALUE=DATE`) travel as a date (y, m, d) and
   are shown as dates, never shifted by a zone.
3. **Zones.** `Z` is UTC. `TZID` with an IANA name resolves through `jiff`
   (system zoneinfo, `tz-system`; a bundled copy as fallback,
   `tzdb-bundle-always`, so confinement or a missing tzdata never breaks it). A Windows zone name (Outlook: "W. Europe Standard
   Time") maps to IANA through a small table generated from CLDR
   `windowsZones.xml`. A `TZID` that resolves nowhere uses the part's own
   `VTIMEZONE` `STANDARD` offset when present; failing that the time is
   treated as UTC and the card marks it "time zone unknown". A floating time
   (no zone) is taken as the phone's local time (QML formats it as local).
4. **Who answers.** The account's own address (`AccountSettings.email`). If
   an `ATTENDEE` matches it (case-insensitive), that line is answered. If none
   matches (invited through a list or an alias), the reply adds the account's
   address as the attendee, as Thunderbird does.
5. **The reply.** `To:` the organizer, `Subject:` "Accepted: <summary>"
   (translated: Accepted, Tentative, Declined), `In-Reply-To`/`References`
   from the invite. The body is `multipart/alternative`: the note (or a
   one-line default text) as `text/plain`, then `text/calendar;
   method=REPLY; charset=utf-8` with `METHOD:REPLY`, `PRODID`, and one
   `VEVENT` holding `UID`, `SEQUENCE`, `DTSTAMP`, `DTSTART`, `ORGANIZER`,
   `SUMMARY`, one `ATTENDEE;PARTSTAT=…` line, `COMMENT` with the note.
   `envio_mime::build` gains a calendar part (an `Option` on `Outgoing`); no
   other caller changes.
6. **Undo through a held Outbox item.** There is no Undo send today. The
   reply is written to the Outbox at once (unsent work on disk within 1 s, as
   the spec demands), with a new `send_after_ms` in `OutboxMeta`
   (`#[serde(default)]`, so old items read as 0). The sender skips an item
   until then. Undo within 5 s deletes the item and puts back the answer
   remembered before (or none). The existing Undo book carries the offer;
   offers get a scope (list or reader) so each screen's toast shows only its
   own.
   The reader gets a toast (it has none today); the inbox's `Toast` component
   is reused.
7. **Remembering the answer outside SQLite.** The database is a rebuildable
   cache, so answers live in a JSON file next to `contacts.json`:
   `invites.json`, keyed by account and `UID`, holding `sequence`,
   `partstat`, `answered_at_ms`, `event_end` (Unix s). Entries whose event
   ended more than 30 days ago are dropped on write. Saved like the other
   user data files (atomic write).
8. **What the card shows.**
   - `REQUEST`, no answer: buttons Accept, Maybe, Decline; "Add a note" opens
     a one-line field under them.
   - `REQUEST`, answered with the same or a newer `SEQUENCE`: "You accepted"
     (or maybe/declined) and "Change", which shows the buttons again.
   - `REQUEST` with a higher `SEQUENCE` than the answered one: "Updated" and
     the buttons.
   - `CANCEL`: "Cancelled", the event struck through, no buttons.
   - `REPLY`: "<name> accepted" (from the `ATTENDEE` line), no buttons.
   - Our own reply still in the Outbox: "Sending…" until it leaves, or
     "Not sent, see Outbox" beside the answer once it failed.
   - Past events still show; buttons stay (a late answer is allowed).

## Architecture

- `envio-mime/src/invite/`: `lines.rs` (lines, properties), `event.rs`
  (`Invite`, `InviteMethod`, `Person`, `PartStat`), `when.rs` (`When`), `zones.rs`
  (TZID to offset, the CLDR table in `zones/windows.rs`), `reply.rs`
  (`reply_ics(&Invite, attendee, PartStat, note, dtstamp) -> String`).
  `invite_of(raw)` finds the first calendar part.
- `envio-store`: `OutboxMeta.send_after_ms`; `invites.rs` for
  `invites.json` (load, record, forget, prune).
- `envio-core`: `Command::AnswerInvite { id, answer: Answer, note }` (Undo goes through the
  existing `Command::Undo`); `worker/invite.rs` builds the reply with
  `envio_mime::reply` headers plus the calendar part, enqueues it held for
  `UNDO_MS`, records the answer, emits an undo offer and sends
  `MessageOpened` again with the new card. `MessageView` gets `invite: Option<InviteCard>` (a view
  struct: method, state, summary, location, when, organizer, repeats,
  attendee answers for `REPLY`, can_answer). Strings for states and subjects
  in `text/invite.rs`.
- `envio-ui` and `qt/`: pass the card through (`json.rs` keys `invite.*`);
  a new call `answerInvite(id, answer, note)`; `undoOffer` gains a scope.
- `qml/pages/reader/InviteCard.qml`: the card, above the attachments; the
  reader gets a `Toast` for Undo. "Add to calendar" uses
  `Content.exportFile(path, "calendar")`, a new mode in
  `ut/ContentExport.qml` (`ContentType.Events`, destination).

## Error handling

- A part that does not parse, has no `VEVENT`, no `UID` or no `DTSTART`:
  no card; the attachment row stays.
- No `ORGANIZER` with a `mailto:`: the card shows, without buttons.
- Answer while offline: the item waits in the Outbox like any send; the card
  says "Sending…".
- The send fails for good: the Outbox shows it as failed as today; the
  remembered answer stays, so the card says "You accepted" with the Outbox
  state beside it.

## Testing

- envio-mime unit and corpus tests: Google, Outlook (Windows TZID,
  `VTIMEZONE`), Apple, Fastmail and Thunderbird style invites; folded lines,
  escapes, all-day, `DURATION`, floating, unknown zone, `CANCEL`, `REPLY`,
  broken input (proptest: never panics). Reply: snapshot of the built
  message, `METHOD:REPLY` in both the content type and the body, one
  attendee, note in `COMMENT`, CRLF lines folded at 75 octets.
- envio-store: `send_after_ms` default on old JSON; `invites.json` round
  trip and pruning.
- envio-core: answer enqueues a held item; the sender skips it until
  `send_after_ms`; Undo deletes it and forgets the answer; reopening shows
  "You accepted"; a higher `SEQUENCE` shows "Updated".
- envio-e2e: the reply reaches the organizer's mailbox on Stalwart with the
  calendar part intact.
- QML: `tst_invite_card.qml` for each state; reader toast.
- Phone: the Calendar import check; a card in Carbon and Airmail, light and
  dark, shot with `ute2e`.
