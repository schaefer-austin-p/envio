//! The invite as a calendar file: passed through when it defines its zones,
//! rebuilt in UTC when it does not (issue #206).

use envio_mime::{calendar_file, parse_invite};

const STAMP: i64 = 1_794_000_000;

/// The PRODID line for the crate's current major.minor.
fn prodid() -> String {
    format!(
        "PRODID:-//Envio//Envio {}.{}//EN",
        env!("CARGO_PKG_VERSION_MAJOR"),
        env!("CARGO_PKG_VERSION_MINOR")
    )
}

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
            prodid().as_str(),
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

#[test]
fn a_cancellation_with_an_undefined_zone_stays_cancelled() {
    let ics = event("DTSTART;TZID=W. Europe Standard Time:20261201T100000\r\n")
        .replace("METHOD:REQUEST", "METHOD:CANCEL");
    let out = lines(&export(&ics));
    assert!(out.contains(&"STATUS:CANCELLED".to_owned()), "{out:?}");
}

#[test]
fn a_status_cancelled_request_with_an_undefined_zone_stays_cancelled() {
    let ics = event("DTSTART;TZID=Tokyo Standard Time:20261121T090000\r\nSTATUS:CANCELLED\r\n");
    assert!(lines(&export(&ics)).contains(&"STATUS:CANCELLED".to_owned()));
}

#[test]
fn a_live_request_is_not_marked_cancelled() {
    let ics = event("DTSTART;TZID=Tokyo Standard Time:20261121T090000\r\n");
    assert!(!export(&ics).contains("STATUS"));
}

#[test]
fn a_rebuilt_event_names_the_current_version_in_prodid() {
    let ics = event("DTSTART;TZID=Europe/Berlin:20261201T100000\r\n");
    assert!(lines(&export(&ics)).contains(&prodid()));
}

#[test]
fn a_rebuilt_event_keeps_its_description_escaped() {
    let ics = event(
        "DTSTART;TZID=Europe/Berlin:20261201T100000\r\n\
         DESCRIPTION:Agenda: a\\, b\\; c\\nSecond line\\\\ok\r\n",
    );
    let out = lines(&export(&ics));
    assert!(
        out.contains(&"DESCRIPTION:Agenda: a\\, b\\; c\\nSecond line\\\\ok".to_owned()),
        "{out:?}"
    );
    let again = reparse(&export(&ics));
    assert_eq!(again.description, "Agenda: a, b; c\nSecond line\\ok");
}

#[test]
fn a_long_description_is_folded_and_survives_a_rebuild() {
    let text = "word, ".repeat(120) + "é end";
    let body = format!(
        "DTSTART;TZID=Europe/Berlin:20261201T100000\r\nDESCRIPTION:{}\r\n",
        text.replace(',', "\\,")
    );
    let out = export(&event(&body));
    assert!(out.split("\r\n").all(|l| l.len() <= 75), "{out}");
    assert!(out.contains("\r\n "), "folded");
    assert_eq!(reparse(&out).description, text);
}

#[test]
fn an_event_without_description_gets_none() {
    let ics = event("DTSTART;TZID=Europe/Berlin:20261201T100000\r\n");
    assert!(!export(&ics).contains("DESCRIPTION"));
}

/// The exported file read back (it carries no METHOD, so one is added).
fn reparse(ics: &str) -> envio_mime::Invite {
    let ics = ics.replacen(
        "BEGIN:VCALENDAR\r\n",
        "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\n",
        1,
    );
    parse_invite(ics.as_bytes()).unwrap()
}
