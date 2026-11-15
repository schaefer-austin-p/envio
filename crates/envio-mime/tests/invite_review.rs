//! Invite reading, review round: plain organizer addresses, VTIMEZONEs with
//! daylight time, zone names that are paths, DST gaps and folds, RRULE UNTIL.

use envio_mime::{Invite, When, parse_invite};

fn request(lines: &str) -> Invite {
    let ics = format!(
        "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\n{lines}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
    );
    parse_invite(ics.as_bytes()).unwrap()
}

fn timed(invite: &Invite) -> (i64, i64, bool) {
    match invite.when {
        When::Timed {
            start,
            end,
            zone_unknown,
        } => (start, end, zone_unknown),
        ref other => panic!("not timed: {other:?}"),
    }
}

#[test]
fn organizers_that_are_not_plain_addresses_are_refused() {
    for bad in [
        "mailto:a@b.example>,c@d.example",
        "mailto:nobody",
        "mailto:a@b@c.example",
        "mailto:@b.example",
        "mailto:a@",
        "mailto:a(b)@c.example",
        "mailto:\"a\"@c.example",
    ] {
        let invite = request(&format!(
            "UID:u1\r\nDTSTART:20261116T100000Z\r\nORGANIZER:{bad}"
        ));
        assert_eq!(invite.organizer, None, "{bad}");
        assert_eq!(invite.organizer_line, None, "{bad}");
    }
    let ok = request("UID:u1\r\nDTSTART:20261116T100000Z\r\nORGANIZER:mailto:ada@example.org");
    assert_eq!(ok.organizer.unwrap().email, "ada@example.org");
}

#[test]
fn a_vtimezone_with_daylight_time_is_zone_unknown() {
    let with = |daylight: &str| {
        format!(
            "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nBEGIN:VTIMEZONE\r\nTZID:Corp Time\r\n\
             BEGIN:STANDARD\r\nDTSTART:16011028T030000\r\nTZOFFSETFROM:+0200\r\n\
             TZOFFSETTO:+0100\r\nEND:STANDARD\r\n{daylight}END:VTIMEZONE\r\n\
             BEGIN:VEVENT\r\nUID:u1\r\nDTSTART;TZID=Corp Time:20260721T100000\r\n\
             END:VEVENT\r\nEND:VCALENDAR\r\n"
        )
        .into_bytes()
    };
    let dst = "BEGIN:DAYLIGHT\r\nDTSTART:16010325T020000\r\nTZOFFSETFROM:+0100\r\n\
               TZOFFSETTO:+0200\r\nEND:DAYLIGHT\r\n";
    let (_, _, unknown) = timed(&parse_invite(&with(dst)).unwrap());
    assert!(unknown, "a DAYLIGHT block makes one offset wrong");
    let (start, _, unknown) = timed(&parse_invite(&with("")).unwrap());
    assert!(!unknown, "standard-only stays resolved");
    assert_eq!(start, 1_784_628_000 - 3600);
}

#[test]
fn path_like_tzids_are_never_looked_up() {
    // The first would be Europe/Berlin if the path were followed.
    for tzid in [
        "Europe/../Europe/Berlin",
        "../zoneinfo/Europe/Berlin",
        "/usr/share/zoneinfo/Europe/Berlin",
        "a/../../b",
        "/etc/x",
        "../x",
    ] {
        let invite = request(&format!("UID:u1\r\nDTSTART;TZID={tzid}:20260721T100000"));
        let (start, _, unknown) = timed(&invite);
        assert!(unknown, "{tzid}");
        assert_eq!(start, 1_784_628_000, "{tzid} reads as UTC");
    }
}

#[test]
fn a_gap_reads_after_the_jump_and_a_fold_the_earlier_instant() {
    // Berlin 2026-03-29: 02:00 CET jumps to 03:00 CEST, so 02:30 is not a
    // real time; the compatible reading is 03:30 CEST = 01:30 UTC.
    let gap = request("UID:u1\r\nDTSTART;TZID=Europe/Berlin:20260329T023000");
    assert_eq!(timed(&gap), (1_774_747_800, 1_774_747_800, false));
    // 2026-10-25: 03:00 CEST falls back to 02:00 CET, so 02:30 happens
    // twice; the earlier is CEST = 00:30 UTC.
    let fold = request("UID:u1\r\nDTSTART;TZID=Europe/Berlin:20261025T023000");
    assert_eq!(timed(&fold), (1_792_888_200, 1_792_888_200, false));
}

#[test]
fn an_rrule_until_is_read() {
    let until = |rule: &str| {
        request(&format!(
            "UID:u1\r\nDTSTART:20261116T100000Z\r\nRRULE:{rule}"
        ))
        .repeats_until
    };
    assert_eq!(until("FREQ=WEEKLY"), None);
    assert_eq!(
        until("FREQ=WEEKLY;UNTIL=20270101T000000Z"),
        Some(1_798_761_600)
    );
    assert_eq!(until("FREQ=WEEKLY;UNTIL=20270101"), Some(1_798_761_600));
    assert_eq!(until("FREQ=WEEKLY;UNTIL=garbage"), None);
}
