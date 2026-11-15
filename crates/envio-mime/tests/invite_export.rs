//! The invite as a calendar file: passed through when it defines its zones,
//! rebuilt in UTC when it does not (issue #206).

use envio_mime::{calendar_file, parse_invite};

const STAMP: i64 = 1_794_000_000;

fn export(ics: &str) -> String {
    calendar_file(&parse_invite(ics.as_bytes()).unwrap(), STAMP)
}

/// Unfolded content lines.
fn lines(ics: &str) -> Vec<String> {
    assert!(ics.ends_with("\r\n"));
    let mut out: Vec<String> = Vec::new();
    for l in ics.trim_end_matches("\r\n").split("\r\n") {
        match (l.strip_prefix(' '), out.last_mut()) {
            (Some(rest), Some(last)) => last.push_str(rest),
            _ => out.push(l.to_owned()),
        }
    }
    out
}

fn event(body: &str) -> String {
    format!(
        "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\nUID:evt-1@example.org\r\n\
         SUMMARY:Planning\\, Q1\r\n{body}\
         ORGANIZER;CN=Olga:mailto:olga@example.org\r\n\
         ATTENDEE;CN=Bob:mailto:bob@example.org\r\n\
         END:VEVENT\r\nEND:VCALENDAR\r\n"
    )
}

const BERLIN: &str = "BEGIN:VTIMEZONE\r\nTZID:Europe/Berlin\r\nBEGIN:STANDARD\r\n\
     DTSTART:19701025T030000\r\nTZOFFSETFROM:+0200\r\nTZOFFSETTO:+0100\r\n\
     RRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\n";

#[test]
fn an_invite_that_defines_its_zone_is_passed_through() {
    let ics = event("DTSTART;TZID=Europe/Berlin:20261201T100000\r\n").replacen(
        "BEGIN:VEVENT",
        &format!("{BERLIN}BEGIN:VEVENT"),
        1,
    );
    assert_eq!(export(&ics), ics);
}

#[test]
fn an_invite_in_utc_is_passed_through() {
    let ics = event("DTSTART:20261201T090000Z\r\nDTEND:20261201T100000Z\r\n");
    assert_eq!(export(&ics), ics);
}

#[test]
fn a_zone_without_its_vtimezone_is_rewritten_in_utc() {
    let ics = event(
        "DTSTART;TZID=Europe/Berlin:20261201T100000\r\n\
         DTEND;TZID=Europe/Berlin:20261201T113000\r\n\
         LOCATION:Room 3\\, second floor\r\nRRULE:FREQ=WEEKLY\r\n",
    );
    let out = lines(&export(&ics));
    assert_eq!(
        out,
        [
            "BEGIN:VCALENDAR",
            "PRODID:-//Envio//Envio 0.2//EN",
            "VERSION:2.0",
            "BEGIN:VEVENT",
            "UID:evt-1@example.org",
            "DTSTAMP:20261106T212000Z",
            "DTSTART:20261201T090000Z",
            "DTEND:20261201T103000Z",
            "SUMMARY:Planning\\, Q1",
            "LOCATION:Room 3\\, second floor",
            "ORGANIZER;CN=Olga:mailto:olga@example.org",
            "END:VEVENT",
            "END:VCALENDAR",
        ]
    );
}

#[test]
fn a_windows_zone_name_is_rewritten_in_utc() {
    let ics = event("DTSTART;TZID=Tokyo Standard Time:20261121T090000\r\n");
    let out = lines(&export(&ics));
    assert!(
        out.contains(&"DTSTART:20261121T000000Z".to_owned()),
        "{out:?}"
    );
    assert!(!out.iter().any(|l| l.starts_with("DTEND")), "{out:?}");
}

#[test]
fn a_vtimezone_for_another_zone_does_not_count() {
    let ics = event("DTSTART;TZID=America/New_York:20261120T100000\r\n").replacen(
        "BEGIN:VEVENT",
        &format!("{BERLIN}BEGIN:VEVENT"),
        1,
    );
    let out = lines(&export(&ics));
    assert!(
        out.contains(&"DTSTART:20261120T150000Z".to_owned()),
        "{out:?}"
    );
    assert!(!out.iter().any(|l| l.contains("TZID")), "{out:?}");
}

#[test]
fn an_unknown_zone_goes_out_as_floating_wall_time() {
    let ics = event(
        "DTSTART;TZID=Nowhere/Atlantis:20261201T100000\r\n\
         DTEND;TZID=Nowhere/Atlantis:20261201T110000\r\n",
    );
    let out = lines(&export(&ics));
    assert!(
        out.contains(&"DTSTART:20261201T100000".to_owned()),
        "{out:?}"
    );
    assert!(out.contains(&"DTEND:20261201T110000".to_owned()), "{out:?}");
}

#[test]
fn an_all_day_event_keeps_its_dates() {
    // The TZID on another property makes it not self-contained.
    let ics = event(
        "DTSTART;VALUE=DATE:20261201\r\nDTEND;VALUE=DATE:20261203\r\n\
         X-WHEN;TZID=Europe/Berlin:20261201T000000\r\n",
    );
    let out = lines(&export(&ics));
    assert!(
        out.contains(&"DTSTART;VALUE=DATE:20261201".to_owned()),
        "{out:?}"
    );
    assert!(
        out.contains(&"DTEND;VALUE=DATE:20261203".to_owned()),
        "{out:?}"
    );
}

#[test]
fn text_cannot_inject_lines() {
    let ics = event("DTSTART;TZID=Europe/Berlin:20261201T100000\r\n").replace(
        "SUMMARY:Planning\\, Q1",
        "SUMMARY:Hi\\nEND:VEVENT\\nBEGIN:VEVENT\\;x",
    );
    let out = lines(&export(&ics));
    assert_eq!(out.iter().filter(|l| *l == "BEGIN:VEVENT").count(), 1);
    assert!(
        out.contains(&"SUMMARY:Hi\\nEND:VEVENT\\nBEGIN:VEVENT\\;x".to_owned()),
        "{out:?}"
    );
}

#[test]
fn an_empty_summary_and_location_are_left_out() {
    let ics = event("DTSTART;TZID=Europe/Berlin:20261201T100000\r\n")
        .replace("SUMMARY:Planning\\, Q1\r\n", "");
    let out = lines(&export(&ics));
    assert!(
        !out.iter()
            .any(|l| l.starts_with("SUMMARY") || l.starts_with("LOCATION"))
    );
}

#[test]
fn a_changed_occurrence_keeps_its_recurrence_id() {
    let ics = event(
        "RECURRENCE-ID;TZID=Europe/Berlin:20261208T100000\r\n\
         DTSTART;TZID=Europe/Berlin:20261208T140000\r\n",
    );
    let out = lines(&export(&ics));
    let at = |prefix: &str| out.iter().position(|l| l.starts_with(prefix));
    assert!(
        out.contains(&"RECURRENCE-ID:20261208T090000Z".to_owned()),
        "{out:?}"
    );
    assert!(
        out.contains(&"DTSTART:20261208T130000Z".to_owned()),
        "{out:?}"
    );
    assert!(at("UID") < at("RECURRENCE-ID"), "{out:?}");
}
