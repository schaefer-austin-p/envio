//! Reading iMIP invites: the corpus mails, the line grammar and the caps.

use envio_mime::{InviteMethod, NaiveParts, PartStat, When, invite_of, parse_invite};
use proptest::prelude::*;

fn corpus(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus")
        .join(name);
    std::fs::read(path).unwrap()
}

/// A calendar with `lines` inside one VEVENT.
fn ics(method: &str, lines: &str) -> Vec<u8> {
    format!("BEGIN:VCALENDAR\r\nMETHOD:{method}\r\nBEGIN:VEVENT\r\n{lines}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n")
        .into_bytes()
}

fn request(lines: &str) -> envio_mime::Invite {
    parse_invite(&ics("REQUEST", lines)).unwrap()
}

#[test]
fn google_invite_reads_summary_time_organizer() {
    let (invite, index) = invite_of(&corpus("invite-google.eml")).unwrap();
    assert_eq!(index, Some(0));
    assert_eq!(invite.method, InviteMethod::Request);
    assert_eq!(invite.uid, "google-1@example.org");
    assert_eq!(invite.summary, "Quarterly planning");
    assert_eq!(invite.location, "Room 4, Example HQ");
    // 2026-11-16 10:00 UTC to 11:00.
    assert_eq!(
        invite.when,
        When::Timed {
            start: 1_794_823_200,
            end: 1_794_826_800,
            zone_unknown: false
        }
    );
    let organizer = invite.organizer.unwrap();
    assert_eq!(
        (organizer.email.as_str(), organizer.name.as_str()),
        ("alice@example.org", "Alice Example")
    );
    assert_eq!(invite.attendees.len(), 2);
    assert_eq!(invite.attendees[0].email, "bob@example.org");
    assert_eq!(invite.attendees[0].name, "Bob Example");
    assert_eq!(invite.attendees[0].partstat, Some(PartStat::NeedsAction));
    assert_eq!(invite.attendees[1].name, "Carol Example");
    assert_eq!(invite.attendees[1].partstat, Some(PartStat::Accepted));
    assert!(invite.raw.starts_with("BEGIN:VCALENDAR"));
}

#[test]
fn folded_lines_and_escapes_unfold() {
    let invite = request(
        "UID:u1\r\nDTSTART:20261116T100000Z\r\nSUMMARY:Plan\\, build\r\n \\nand ship\r\n\t(three lines)",
    );
    assert_eq!(invite.summary, "Plan, build\nand ship(three lines)");
}

#[test]
fn lf_only_folds_unfold_too() {
    let raw = "BEGIN:VCALENDAR\nMETHOD:REQUEST\nBEGIN:VEVENT\nUID:u1\nDTSTART:20261116T100000Z\nSUMMARY:One\n two\nEND:VEVENT\nEND:VCALENDAR\n";
    assert_eq!(parse_invite(raw.as_bytes()).unwrap().summary, "Onetwo");
}

#[test]
fn quoted_cn_with_colon_and_semicolon() {
    let invite = request(
        "UID:u1\r\nDTSTART:20261116T100000Z\r\nATTENDEE;CN=\"Doe; Jane: PM\";PARTSTAT=TENTATIVE:mailto:jane@example.org",
    );
    assert_eq!(invite.attendees[0].name, "Doe; Jane: PM");
    assert_eq!(invite.attendees[0].email, "jane@example.org");
    assert_eq!(invite.attendees[0].partstat, Some(PartStat::Tentative));
}

#[test]
fn names_are_case_insensitive_and_non_mailto_organizer_is_ignored() {
    let invite = request(
        "uid:u1\r\ndtstart:20261116T100000Z\r\norganizer;cn=Eve:https://example.org/eve\r\nattendee;partstat=declined:MAILTO:bob@example.org",
    );
    assert_eq!(invite.organizer, None);
    assert_eq!(invite.attendees[0].partstat, Some(PartStat::Declined));
}

#[test]
fn only_first_vevent_and_no_alarm_leaks() {
    let raw = "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\nUID:one\r\nDTSTART:20261116T100000Z\r\nSUMMARY:First\r\nBEGIN:VALARM\r\nSUMMARY:Alarm\r\nEND:VALARM\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:two\r\nDTSTART:20261117T100000Z\r\nSUMMARY:Second\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    let invite = parse_invite(raw.as_bytes()).unwrap();
    assert_eq!(
        (invite.uid.as_str(), invite.summary.as_str()),
        ("one", "First")
    );
}

#[test]
fn cancel_method_marks_cancelled() {
    let (invite, _) = invite_of(&corpus("invite-cancel.eml")).unwrap();
    assert_eq!(invite.method, InviteMethod::Cancel);
    assert!(invite.cancelled);
    assert_eq!(invite.sequence, 1);
    let bare = parse_invite(&ics("CANCEL", "UID:u1\r\nDTSTART:20261116T100000Z")).unwrap();
    assert!(bare.cancelled);
}

#[test]
fn status_cancelled_marks_cancelled() {
    let invite = request("UID:u1\r\nDTSTART:20261116T100000Z\r\nSTATUS:cancelled");
    assert!(invite.cancelled);
    assert!(!request("UID:u1\r\nDTSTART:20261116T100000Z").cancelled);
}

#[test]
fn reply_reads_attendee_partstat() {
    let (invite, _) = invite_of(&corpus("invite-reply.eml")).unwrap();
    assert_eq!(invite.method, InviteMethod::Reply);
    assert_eq!(invite.attendees.len(), 1);
    assert_eq!(invite.attendees[0].partstat, Some(PartStat::Accepted));
}

#[test]
fn all_day_uses_dates_and_default_end() {
    let (invite, _) = invite_of(&corpus("invite-allday.eml")).unwrap();
    assert_eq!(
        invite.when,
        When::AllDay {
            start: (2026, 12, 24),
            end: (2026, 12, 25)
        }
    );
    let year_end = request("UID:u1\r\nDTSTART;VALUE=DATE:20261231");
    assert_eq!(
        year_end.when,
        When::AllDay {
            start: (2026, 12, 31),
            end: (2027, 1, 1)
        }
    );
    let leap = request("UID:u1\r\nDTSTART;VALUE=DATE:20280228\r\nDTEND;VALUE=DATE:20280301");
    assert_eq!(
        leap.when,
        When::AllDay {
            start: (2028, 2, 28),
            end: (2028, 3, 1)
        }
    );
}

#[test]
fn duration_sets_end() {
    let invite = request("UID:u1\r\nDTSTART:20261116T100000Z\r\nDURATION:PT1H30M");
    assert_eq!(
        invite.when,
        When::Timed {
            start: 1_794_823_200,
            end: 1_794_823_200 + 5400,
            zone_unknown: false
        }
    );
}

#[test]
fn no_end_means_end_equals_start() {
    let invite = request("UID:u1\r\nDTSTART:20261116T100000Z");
    assert_eq!(
        invite.when,
        When::Timed {
            start: 1_794_823_200,
            end: 1_794_823_200,
            zone_unknown: false
        }
    );
}

#[test]
fn floating_time_stays_wall_time() {
    let invite = request("UID:u1\r\nDTSTART:20261116T100000\r\nDTEND:20261116T113000");
    let wall = |h, mi| NaiveParts {
        year: 2026,
        month: 11,
        day: 16,
        hour: h,
        minute: mi,
        second: 0,
    };
    assert_eq!(
        invite.when,
        When::Floating {
            start: wall(10, 0),
            end: wall(11, 30)
        }
    );
}

fn timed(invite: &envio_mime::Invite) -> (i64, i64, bool) {
    match invite.when {
        When::Timed {
            start,
            end,
            zone_unknown,
        } => (start, end, zone_unknown),
        ref other => panic!("not timed: {other:?}"),
    }
}

/// A calendar with a VTIMEZONE block (`TZID`, standard offset) and one event.
fn with_vtimezone(tzid: &str, offset: &str, event: &str) -> Vec<u8> {
    format!(
        "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nBEGIN:VTIMEZONE\r\nTZID:{tzid}\r\n\
         BEGIN:STANDARD\r\nDTSTART:16010101T000000\r\nTZOFFSETFROM:+0000\r\n\
         TZOFFSETTO:{offset}\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\n\
         BEGIN:VEVENT\r\n{event}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
    )
    .into_bytes()
}

#[test]
fn iana_tzid_resolves_with_dst() {
    // Berlin in July is UTC+2: 10:00 local is 08:00 UTC.
    let invite = request("UID:u1\r\nDTSTART;TZID=Europe/Berlin:20260721T100000");
    assert_eq!(timed(&invite), (1_784_620_800, 1_784_620_800, false));
    // In November it is UTC+1.
    let invite = request("UID:u1\r\nDTSTART;TZID=Europe/Berlin:20261116T100000");
    assert_eq!(timed(&invite), (1_794_819_600, 1_794_819_600, false));
}

#[test]
fn bundled_tzdb_used_without_system_zoneinfo() {
    if std::env::var_os("ENVIO_ZONES_CHILD").is_some() {
        let invite = request("UID:u1\r\nDTSTART;TZID=Europe/Berlin:20260721T100000");
        assert_eq!(timed(&invite), (1_784_620_800, 1_784_620_800, false));
        return;
    }
    // jiff reads TZDIR once per process: rerun this test in a child whose
    // TZDIR is an empty directory.
    let empty = std::env::temp_dir().join(format!("envio-empty-tzdir-{}", std::process::id()));
    std::fs::create_dir_all(&empty).unwrap();
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "bundled_tzdb_used_without_system_zoneinfo"])
        .env("TZDIR", &empty)
        .env("ENVIO_ZONES_CHILD", "1")
        .output()
        .unwrap();
    let _ = std::fs::remove_dir(&empty);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn windows_tzid_resolves() {
    let invite = request("UID:u1\r\nDTSTART;TZID=W. Europe Standard Time:20260721T100000");
    assert_eq!(timed(&invite), (1_784_620_800, 1_784_620_800, false));
}

#[test]
fn mozilla_prefixed_tzid_resolves() {
    // London in November is UTC+0; a leading slash alone also resolves.
    let invite =
        request("UID:u1\r\nDTSTART;TZID=/mozilla.org/20050126_1/Europe/London:20261119T150000");
    assert_eq!(timed(&invite), (1_795_100_400, 1_795_100_400, false));
    let invite = request("UID:u1\r\nDTSTART;TZID=/Europe/Berlin:20261116T100000");
    assert_eq!(timed(&invite), (1_794_819_600, 1_794_819_600, false));
}

#[test]
fn unknown_tzid_uses_vtimezone_offset() {
    let bytes = with_vtimezone(
        "Custom Zone",
        "+0530",
        "UID:u1\r\nDTSTART;TZID=Custom Zone:20261116T100000",
    );
    let invite = parse_invite(&bytes).unwrap();
    // 10:00 at +05:30 is 04:30 UTC.
    assert_eq!(timed(&invite), (1_794_803_400, 1_794_803_400, false));
}

#[test]
fn unknown_tzid_without_vtimezone_is_zone_unknown() {
    let invite = request("UID:u1\r\nDTSTART;TZID=Nowhere/Land:20261116T100000");
    assert_eq!(timed(&invite), (1_794_823_200, 1_794_823_200, true));
}

#[test]
fn outlook_corpus_reads_local_time() {
    let (invite, _) = invite_of(&corpus("invite-outlook.eml")).unwrap();
    // 2026-07-21 14:00 to 15:00 in W. Europe (UTC+2): 12:00 to 13:00 UTC.
    assert_eq!(timed(&invite), (1_784_635_200, 1_784_638_800, false));
}

#[test]
fn rrule_sets_repeats() {
    let (apple, _) = invite_of(&corpus("invite-apple.eml")).unwrap();
    assert!(apple.repeats);
    assert_eq!(apple.sequence, 2);
    assert!(!request("UID:u1\r\nDTSTART:20261116T100000Z").repeats);
}

#[test]
fn no_uid_or_no_dtstart_gives_none() {
    assert!(parse_invite(&ics("REQUEST", "DTSTART:20261116T100000Z")).is_none());
    assert!(parse_invite(&ics("REQUEST", "UID:u1")).is_none());
    assert!(parse_invite(&ics("REQUEST", "UID:u1\r\nDTSTART:2026-11-16")).is_none());
    assert!(parse_invite(&ics("REQUEST", "UID:u1\r\nDTSTART:20261316T100000Z")).is_none());
}

#[test]
fn other_or_missing_method_gives_none() {
    assert!(parse_invite(&ics("PUBLISH", "UID:u1\r\nDTSTART:20261116T100000Z")).is_none());
    let no_method = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:u1\r\nDTSTART:20261116T100000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    assert!(parse_invite(no_method.as_bytes()).is_none());
}

#[test]
fn oversized_part_gives_none() {
    let mut big = ics("REQUEST", "UID:u1\r\nDTSTART:20261116T100000Z");
    big.extend(std::iter::repeat_n(b'x', 256 * 1024));
    assert!(parse_invite(&big).is_none());
    let many_lines = ics(
        "REQUEST",
        &format!(
            "UID:u1\r\nDTSTART:20261116T100000Z{}",
            "\r\nX-A:b".repeat(10_001)
        ),
    );
    assert!(parse_invite(&many_lines).is_none());
}

#[test]
fn text_is_capped_and_stripped() {
    let invite = request(&format!(
        "UID:u1\r\nDTSTART:20261116T100000Z\r\nSUMMARY:a\u{7}b\u{202e}{}",
        "x".repeat(900)
    ));
    assert_eq!(invite.summary.chars().count(), 500);
    assert!(invite.summary.starts_with("ab"));
}

#[test]
fn attendees_are_capped_at_500() {
    let many: String = (0..600)
        .map(|i| format!("ATTENDEE:mailto:p{i}@example.org\r\n"))
        .collect();
    let invite = request(&format!("UID:u1\r\n{many}DTSTART:20261116T100000Z"));
    assert_eq!(invite.attendees.len(), 500);
}

#[test]
fn invite_of_picks_first_calendar_part() {
    let (invite, index) = invite_of(&corpus("calendar-invite.eml")).unwrap();
    assert_eq!(invite.uid, "invite-1@example.com");
    assert_eq!(index, Some(0));
}

#[test]
fn invite_of_reads_attached_ics() {
    let (invite, index) = invite_of(&corpus("invite-fastmail.eml")).unwrap();
    assert_eq!(invite.summary, "Design review, round 2");
    assert_eq!(index, Some(0));
}

#[test]
fn invite_of_is_none_for_plain_mail() {
    assert!(invite_of(&corpus("gmail-alternative.eml")).is_none());
    assert!(invite_of(&corpus("garbage.eml")).is_none());
}

#[test]
fn every_corpus_invite_parses() {
    for name in [
        "google",
        "outlook",
        "apple",
        "fastmail",
        "thunderbird",
        "cancel",
        "reply",
        "allday",
    ] {
        assert!(
            invite_of(&corpus(&format!("invite-{name}.eml"))).is_some(),
            "{name}"
        );
    }
}

proptest! {
    #[test]
    fn parse_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..2000)) {
        let _ = parse_invite(&bytes);
        let _ = invite_of(&bytes);
    }

    #[test]
    fn mutated_corpus_never_panics(
        name in prop::sample::select(vec!["google", "outlook", "apple", "fastmail", "thunderbird", "cancel", "reply", "allday"]),
        cuts in proptest::collection::vec((any::<usize>(), any::<u8>()), 0..8),
    ) {
        let mut bytes = corpus(&format!("invite-{name}.eml"));
        for (at, byte) in cuts {
            let i = at % bytes.len();
            match byte % 3 {
                0 => bytes[i] = byte,
                1 => { bytes.remove(i); }
                _ => bytes.insert(i, byte),
            }
        }
        let _ = invite_of(&bytes);
        let _ = parse_invite(&bytes);
    }
}

#[test]
fn second_vevent_never_leaks_into_the_first() {
    let raw = "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\nUID:one\r\nDTSTART:20261116T100000Z\r\nATTENDEE:mailto:bob@example.org\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nLOCATION:Elsewhere\r\nRRULE:FREQ=DAILY\r\nSTATUS:CANCELLED\r\nORGANIZER:mailto:eve@example.org\r\nATTENDEE:mailto:extra@example.org\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    let invite = parse_invite(raw.as_bytes()).unwrap();
    assert_eq!(invite.location, "");
    assert!(!invite.repeats);
    assert!(!invite.cancelled);
    assert_eq!(invite.organizer, None);
    assert_eq!(invite.attendees.len(), 1);
    assert_eq!(invite.attendees[0].email, "bob@example.org");
}

#[test]
fn huge_durations_do_not_overflow() {
    for dur in [
        "PT9223372036854775807S",
        "P99999999999D",
        "P99999999999999999999W",
    ] {
        let timed = request(&format!(
            "UID:u1\r\nDTSTART:20261116T100000Z\r\nDURATION:{dur}"
        ));
        assert_eq!(
            timed.when,
            When::Timed {
                start: 1_794_823_200,
                end: 1_794_823_200,
                zone_unknown: false
            }
        );
        assert!(matches!(
            request(&format!(
                "UID:u1\r\nDTSTART;VALUE=DATE:20261116\r\nDURATION:{dur}"
            ))
            .when,
            When::AllDay {
                end: (2026, 11, 17),
                ..
            }
        ));
        assert!(matches!(
            request(&format!(
                "UID:u1\r\nDTSTART:20261116T100000\r\nDURATION:{dur}"
            ))
            .when,
            When::Floating { .. }
        ));
        let _ = request(&format!(
            "UID:u1\r\nDTSTART;TZID=Europe/Berlin:20261116T100000\r\nDURATION:{dur}"
        ));
    }
}

#[test]
fn end_before_start_is_clamped_and_bidi_stripped() {
    let invite = request(
        "UID:u1\r\nDTSTART:20261116T100000Z\r\nDTEND:20261116T090000Z\r\nSUMMARY:a\u{202e}b",
    );
    assert_eq!(
        invite.when,
        When::Timed {
            start: 1_794_823_200,
            end: 1_794_823_200,
            zone_unknown: false
        }
    );
    assert_eq!(invite.summary, "ab");
}

#[test]
fn exactly_ten_thousand_lines_are_accepted() {
    let head = "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\nUID:u1\r\nDTSTART:20261116T100000Z\r\n";
    let tail = "END:VEVENT\r\nEND:VCALENDAR\r\n";
    // head 5 lines + tail 2 lines + fill.
    let fill = "X-A:b\r\n".repeat(10_000 - 7);
    assert!(parse_invite(format!("{head}{fill}{tail}").as_bytes()).is_some());
    let over = "X-A:b\r\n".repeat(10_000 - 6);
    assert!(parse_invite(format!("{head}{over}{tail}").as_bytes()).is_none());
}
