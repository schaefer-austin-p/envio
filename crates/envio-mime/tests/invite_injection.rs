//! Hostile text in an invite or the note cannot add lines to the REPLY
//! calendar or headers to the mail.

use envio_mime::{Address, Envelope, PartStat, build, parse_invite, reply_ics, reply_outgoing};

const STAMP: i64 = 1_794_000_000;

#[test]
fn hostile_text_cannot_add_ics_lines_or_mail_headers() {
    let ics = "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\nUID:evt-1@example.org\r\n\
         DTSTART:20261118T100000Z\r\n\
         SUMMARY:Hi\rATTENDEE\\;PARTSTAT=ACCEPTED:mailto:evil@example.org\\nX-EVIL:1\r\n \
         \rX-FOLDED:1\r\n\
         ORGANIZER;CN=\"Ada\rX-CN:1\":mailto:olga@example.org\r\n\
         END:VEVENT\r\nEND:VCALENDAR\r\n";
    let hostile = parse_invite(ics.as_bytes()).unwrap();
    let note = "ok\r\nATTENDEE:mailto:evil@example.org\r\n X-NOTE:1";
    let text = reply_ics(&hostile, "bob@example.org", PartStat::Accepted, note, STAMP);
    // A reader that also breaks lines at a bare CR or LF sees no new line.
    assert!(!text.replace("\r\n", "").contains(['\r', '\n']), "{text:?}");
    // Unfold like a reader: a physical line starting with a space continues.
    let mut lines: Vec<String> = Vec::new();
    for raw in text.trim_end().split("\r\n") {
        match (raw.strip_prefix(' '), lines.last_mut()) {
            (Some(rest), Some(last)) => last.push_str(rest),
            _ => lines.push(raw.to_owned()),
        }
    }
    assert_eq!(
        lines.iter().filter(|l| l.starts_with("ATTENDEE")).count(),
        1,
        "{lines:?}"
    );
    for bad in ["X-EVIL", "X-FOLDED", "X-CN", "X-NOTE"] {
        assert!(
            !lines.iter().any(|l| l.starts_with(bad)),
            "{bad}: {lines:?}"
        );
    }
    let mut out = reply_outgoing(
        &Envelope::default(),
        &hostile,
        Address::new(Some("Bob"), "bob@example.org"),
        PartStat::Accepted,
        note,
        format!("Accepted: {}", hostile.summary),
        "Bob accepted.".to_owned(),
        STAMP,
    );
    out.subject = "Accepted: Hi\r\nBcc: evil@example.org".to_owned();
    let mail = String::from_utf8(build(&out, STAMP, "m1@example.org").bytes).unwrap();
    let head = mail.split("\r\n\r\n").next().unwrap();
    assert!(
        !head.lines().any(|h| h.starts_with("Bcc:")),
        "header injected:\n{head}"
    );
}
