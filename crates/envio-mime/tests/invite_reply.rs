//! The iMIP REPLY: calendar text, answer rules, folding, threading, mail shape.

use envio_mime::{
    Address, CalendarPart, Envelope, Invite, Outgoing, OutgoingAttachment, PartStat, build,
    parse_invite, reply_ics, reply_outgoing,
};

const STAMP: i64 = 1_794_000_000;

fn invite() -> Invite {
    let ics = "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\nUID:evt-1@example.org\r\n\
         SEQUENCE:2\r\nSUMMARY:Planning\\, Q1\r\n\
         DTSTART;TZID=Europe/Berlin:20261201T100000\r\n\
         ORGANIZER;CN=Olga Organizer:mailto:olga@example.org\r\n\
         ATTENDEE;CN=Bob Example;PARTSTAT=NEEDS-ACTION:mailto:bob@example.org\r\n\
         ATTENDEE;CN=Carol:mailto:carol@example.org\r\n\
         END:VEVENT\r\nEND:VCALENDAR\r\n";
    parse_invite(ics.as_bytes()).unwrap()
}

/// Unfolded content lines of an ICS text.
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

fn outgoing(original: &Envelope, note: &str) -> Outgoing {
    reply_outgoing(
        original,
        &invite(),
        Address::new(Some("Bob Example"), "bob@example.org"),
        PartStat::Accepted,
        note,
        "Accepted: Planning, Q1".to_owned(),
        "Bob Example accepted.".to_owned(),
        STAMP,
    )
}

#[test]
fn reply_has_method_in_body_and_header() {
    let ics = reply_ics(&invite(), "bob@example.org", PartStat::Accepted, "", STAMP);
    let l = lines(&ics);
    assert_eq!(l.first().unwrap(), "BEGIN:VCALENDAR");
    assert_eq!(l.last().unwrap(), "END:VCALENDAR");
    assert!(l.contains(&format!(
        "PRODID:-//Envio//Envio {}.{}//EN",
        env!("CARGO_PKG_VERSION_MAJOR"),
        env!("CARGO_PKG_VERSION_MINOR")
    )));
    for want in [
        "METHOD:REPLY",
        "VERSION:2.0",
        "UID:evt-1@example.org",
        "SEQUENCE:2",
        "DTSTART;TZID=Europe/Berlin:20261201T100000",
        "ORGANIZER;CN=Olga Organizer:mailto:olga@example.org",
        "SUMMARY:Planning\\, Q1",
        "DTSTAMP:20261106T212000Z",
    ] {
        assert!(l.contains(&want.to_owned()), "{want} missing in {l:?}");
    }
    assert!(!l.iter().any(|x| x.starts_with("COMMENT")));

    let built = build(&outgoing(&Envelope::default(), ""), STAMP, "m1@example.org");
    let mail = String::from_utf8(built.bytes).unwrap();
    assert!(mail.contains("multipart/alternative"), "{mail}");
    assert!(
        mail.contains("text/calendar; method=\"REPLY\"; charset=\"utf-8\""),
        "{mail}"
    );
    assert!(!mail.contains("multipart/mixed"));
}

#[test]
fn reply_answers_matching_attendee_only() {
    let ics = reply_ics(&invite(), "BOB@example.org", PartStat::Declined, "", STAMP);
    let att: Vec<_> = lines(&ics)
        .into_iter()
        .filter(|l| l.starts_with("ATTENDEE"))
        .collect();
    assert_eq!(
        att,
        ["ATTENDEE;PARTSTAT=DECLINED;CN=Bob Example:mailto:BOB@example.org"]
    );
}

#[test]
fn reply_adds_me_when_not_listed() {
    let ics = reply_ics(
        &invite(),
        "dave@example.org",
        PartStat::Tentative,
        "",
        STAMP,
    );
    let att: Vec<_> = lines(&ics)
        .into_iter()
        .filter(|l| l.starts_with("ATTENDEE"))
        .collect();
    assert_eq!(att, ["ATTENDEE;PARTSTAT=TENTATIVE:mailto:dave@example.org"]);
}

#[test]
fn note_goes_into_comment_and_text() {
    let note = "Running late; see you,\nBob";
    let ics = reply_ics(
        &invite(),
        "bob@example.org",
        PartStat::Accepted,
        note,
        STAMP,
    );
    assert!(lines(&ics).contains(&"COMMENT:Running late\\; see you\\,\\nBob".to_owned()));
    let out = outgoing(&Envelope::default(), "Running late");
    assert_eq!(out.text, "Bob Example accepted.\n\nRunning late");
    assert_eq!(out.calendar.as_ref().unwrap().method, "REPLY");
    assert_eq!(
        outgoing(&Envelope::default(), "").text,
        "Bob Example accepted."
    );
}

#[test]
fn long_lines_fold_at_75_octets_on_char_boundaries() {
    let note = "é".repeat(200) + &"日本".repeat(40);
    let ics = reply_ics(
        &invite(),
        "bob@example.org",
        PartStat::Accepted,
        &note,
        STAMP,
    );
    for raw in ics.trim_end_matches("\r\n").split("\r\n") {
        assert!(raw.len() <= 75, "{} octets: {raw}", raw.len());
    }
    let comment = lines(&ics)
        .into_iter()
        .find(|l| l.starts_with("COMMENT:"))
        .unwrap();
    assert_eq!(comment, format!("COMMENT:{note}"));
    assert!(ics.contains("\r\n "));
}

#[test]
fn reply_threads_to_the_invite() {
    let original = Envelope {
        message_id: Some("inv@example.org".to_owned()),
        from: vec![Address::new(Some("Someone"), "assistant@example.org")],
        in_reply_to: Some("root@example.org".to_owned()),
        ..Envelope::default()
    };
    let out = outgoing(&original, "");
    assert_eq!(
        out.to,
        [Address::new(Some("Olga Organizer"), "olga@example.org")]
    );
    assert_eq!(out.in_reply_to.as_deref(), Some("inv@example.org"));
    assert_eq!(out.references, ["root@example.org", "inv@example.org"]);
    assert_eq!(out.subject, "Accepted: Planning, Q1");
    assert!(out.cc.is_empty());
}

#[test]
fn reply_snapshot() {
    let original = Envelope {
        message_id: Some("inv@example.org".to_owned()),
        ..Envelope::default()
    };
    let built = build(
        &outgoing(&original, "See you there"),
        STAMP,
        "m1@example.org",
    );
    let mail = String::from_utf8(built.bytes).unwrap();
    // The boundary is random; mask it.
    let boundary = mail
        .split("boundary=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .unwrap()
        .to_owned();
    insta::assert_snapshot!(mail.replace(&boundary, "BOUNDARY"));
}

#[test]
fn outgoing_without_calendar_is_unchanged() {
    let plain = Outgoing {
        from: Address::new(None, "bob@example.org"),
        to: vec![Address::new(None, "alice@example.com")],
        subject: "Hi".to_owned(),
        text: "Hello".to_owned(),
        ..Outgoing::default()
    };
    assert_eq!(plain.calendar, None);
    let mail = String::from_utf8(build(&plain, 0, "x@example.org").bytes).unwrap();
    assert!(mail.contains("Content-Type: text/plain"));
    assert!(!mail.contains("multipart"));
    // With attachments too: mixed around alternative.
    let with = Outgoing {
        calendar: Some(CalendarPart {
            method: "REPLY",
            ics: "BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n".to_owned(),
        }),
        attachments: vec![OutgoingAttachment {
            filename: "a.txt".to_owned(),
            content_type: "text/plain".to_owned(),
            bytes: b"x".to_vec(),
        }],
        ..plain
    };
    let mail = String::from_utf8(build(&with, 0, "x@example.org").bytes).unwrap();
    let mixed = mail.find("multipart/mixed").unwrap();
    assert!(mixed < mail.find("multipart/alternative").unwrap());
}

#[test]
fn the_built_reply_lists_an_unnamed_calendar_part_with_method_reply() {
    let built = build(&outgoing(&Envelope::default(), ""), STAMP, "m1@example.org");
    let envio_mime::Message::Parsed { attachments, .. } = envio_mime::parse(&built.bytes) else {
        panic!("not parsed");
    };
    let part = attachments
        .iter()
        .find(|a| a.content_type == "text/calendar")
        .unwrap();
    assert_eq!(part.method.as_deref(), Some("REPLY"));
    assert_eq!(part.filename, None);
}
