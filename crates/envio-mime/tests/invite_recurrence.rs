//! RECURRENCE-ID: one occurrence of a repeating meeting, and organizer
//! addresses that can never be answered.

use envio_mime::{Invite, PartStat, parse_invite, reply_ics};

const STAMP: i64 = 1_794_000_000;

fn request(lines: &str) -> Invite {
    let ics = format!(
        "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\n{lines}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
    );
    parse_invite(ics.as_bytes()).unwrap()
}

#[test]
fn recurrence_id_is_read_for_one_occurrence_only() {
    let series = request("UID:u1\r\nDTSTART:20261118T100000Z");
    assert_eq!(series.recurrence_id, "");
    assert_eq!(series.recurrence_id_line, None);
    let one = request(
        "UID:u1\r\nDTSTART:20261118T110000Z\r\nRECURRENCE-ID;TZID=Europe/Berlin:20261118T100000",
    );
    assert_eq!(one.recurrence_id, "20261118T100000");
    assert_eq!(
        one.recurrence_id_line.as_deref(),
        Some("RECURRENCE-ID;TZID=Europe/Berlin:20261118T100000")
    );
}

#[test]
fn reply_to_one_occurrence_copies_recurrence_id_after_uid() {
    let one = request(
        "UID:evt-1@example.org\r\nRECURRENCE-ID;TZID=Europe/Berlin:20261118T100000\r\n\
         DTSTART;TZID=Europe/Berlin:20261118T110000\r\nORGANIZER:mailto:olga@example.org",
    );
    let text = reply_ics(&one, "bob@example.org", PartStat::Declined, "", STAMP);
    let lines: Vec<&str> = text.trim_end().split("\r\n").collect();
    let uid = lines.iter().position(|x| x.starts_with("UID:")).unwrap();
    assert_eq!(
        lines[uid + 1],
        "RECURRENCE-ID;TZID=Europe/Berlin:20261118T100000"
    );
    let series = request("UID:evt-1@example.org\r\nDTSTART:20261118T100000Z");
    let text = reply_ics(&series, "bob@example.org", PartStat::Declined, "", STAMP);
    assert!(!text.contains("RECURRENCE-ID"));
}

#[test]
fn organizer_with_controls_or_only_bidi_controls_is_not_an_organizer() {
    for value in [
        "mailto:a@example.org\rx",
        "mailto:a@example.org\u{1}",
        "mailto:\u{202e}",
        "mailto:a b@example.org",
    ] {
        let invite = request(&format!(
            "UID:u1\r\nDTSTART:20261118T100000Z\r\nORGANIZER:{value}"
        ));
        assert!(invite.organizer.is_none(), "{value:?}");
        assert!(invite.organizer_line.is_none(), "{value:?}");
    }
    let ok = request("UID:u1\r\nDTSTART:20261118T100000Z\r\nORGANIZER:mailto:a@example.org");
    assert!(ok.organizer.is_some());
}
