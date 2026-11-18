//! DESCRIPTION on read: the cap (with a marker), control characters, and the
//! shape of the PRODID the app writes.

use envio_mime::{Invite, calendar_file, parse_invite};

const CAP: usize = 4000;

fn read(description: &str) -> Invite {
    let ics = format!(
        "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\nUID:d@example.org\r\n\
         DTSTART:20261201T100000Z\r\nDESCRIPTION:{description}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
    );
    parse_invite(ics.as_bytes()).unwrap()
}

#[test]
fn a_description_at_the_cap_is_kept_whole() {
    let text = "é".repeat(CAP);
    assert_eq!(read(&text).description, text);
}

#[test]
fn a_multibyte_description_past_the_cap_is_cut_on_a_char_boundary_with_a_marker() {
    for unit in ["é", "😀"] {
        let got = read(&unit.repeat(CAP + 50)).description;
        assert_eq!(got.chars().count(), CAP);
        assert_eq!(got, format!("{}…", unit.repeat(CAP - 1)));
    }
}

/// An invite whose zone has no VTIMEZONE, so export rebuilds it.
fn rebuilt(description: &str) -> String {
    let ics = format!(
        "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\nUID:d@example.org\r\n\
         DTSTART;TZID=Nowhere/Land:20261201T100000\r\nDESCRIPTION:{description}\r\n\
         END:VEVENT\r\nEND:VCALENDAR\r\n"
    );
    calendar_file(&parse_invite(ics.as_bytes()).unwrap(), 0)
}

#[test]
fn a_cut_description_is_exported_with_its_marker() {
    let unfolded = rebuilt(&"x".repeat(CAP + 1)).replace("\r\n ", "");
    assert!(unfolded.contains(&format!("DESCRIPTION:{}…\r\n", "x".repeat(CAP - 1))));
}

#[test]
fn control_characters_are_stripped_and_newlines_kept() {
    let got = read("a\u{1}b\u{7}c\td\\ne\\Nf").description;
    assert_eq!(got, "abcd\ne\nf");
}

/// The workspace `version =` line's major.minor.
fn workspace_major_minor() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
    let toml = std::fs::read_to_string(path).unwrap();
    let version = toml
        .lines()
        .find_map(|l| l.strip_prefix("version = \""))
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .to_owned();
    let mut parts = version.split('.');
    format!("{}.{}", parts.next().unwrap(), parts.next().unwrap())
}

#[test]
fn prodid_has_its_shape_and_the_workspace_major_minor() {
    let out = rebuilt("x");
    let line = out.lines().find(|l| l.starts_with("PRODID")).unwrap();
    let version = line
        .strip_prefix("PRODID:-//Envio//Envio ")
        .and_then(|r| r.strip_suffix("//EN"))
        .unwrap();
    let (major, minor) = version.split_once('.').unwrap();
    assert!(
        [major, minor]
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    );
    assert_eq!(version, workspace_major_minor());
}
