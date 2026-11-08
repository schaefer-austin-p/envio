//! Addresses and thread ids.

use envio_mime::{Address, normalize_id, preview, same_email, thread_root};

#[test]
fn new_trims_and_drops_blank_names() {
    assert_eq!(
        Address::new(Some("  "), " bob@example.org "),
        Address {
            name: None,
            email: "bob@example.org".to_owned()
        }
    );
}

#[test]
fn parse_reads_common_forms() {
    let cases = [
        (
            "Bob Example <bob@example.org>",
            Some("Bob Example"),
            "bob@example.org",
        ),
        (
            "\"Example, Bob\" <bob@example.org>",
            Some("Example, Bob"),
            "bob@example.org",
        ),
        ("<bob@example.org>", None, "bob@example.org"),
        ("  bob@example.org ", None, "bob@example.org"),
    ];
    for (text, name, email) in cases {
        assert_eq!(
            Address::parse(text),
            Some(Address::new(name, email)),
            "{text}"
        );
    }
    assert_eq!(Address::parse("not an address"), None);
    assert_eq!(Address::parse(""), None);
}

#[test]
fn display_and_case_insensitive_equality() {
    let bob = Address::new(Some("Bob"), "Bob@Example.org");
    assert_eq!(bob.display(), "Bob <Bob@Example.org>");
    assert_eq!(
        Address::new(None, "a@example.org").display(),
        "a@example.org"
    );
    assert!(bob.is("bob@example.org"));
    assert!(same_email(" A@B.example ", "a@b.example"));
    assert!(!same_email("a@example.org", "b@example.org"));
}

#[test]
fn normalize_id_strips_brackets() {
    assert_eq!(normalize_id(" <a@b> "), Some("a@b".to_owned()));
    assert_eq!(normalize_id("a@b"), Some("a@b".to_owned()));
    assert_eq!(normalize_id("<>"), None);
    assert_eq!(normalize_id("  "), None);
}

#[test]
fn thread_root_prefers_first_reference() {
    let refs = vec!["<root@x>".to_owned(), "<mid@x>".to_owned()];
    let cases = [
        ("<c@x>", Some("<mid@x>"), refs.as_slice(), "root@x"),
        ("<c@x>", Some("<mid@x>"), &[][..], "mid@x"),
        ("<c@x>", None, &[][..], "c@x"),
        ("", None, &[][..], ""),
    ];
    for (id, irt, refs, want) in cases {
        assert_eq!(thread_root(id, irt, refs), want);
    }
    let blank = vec!["<>".to_owned(), "<real@x>".to_owned()];
    assert_eq!(thread_root("c@x", None, &blank), "real@x");
}

#[test]
fn preview_skips_quotes_and_signature() {
    let text = "Hi Bob,\n\n> quoted line\nSee   you\tsoon.\n-- \nAlice\n";
    assert_eq!(preview(text), "Hi Bob, See you soon.");
}

#[test]
fn preview_caps_length_on_char_boundaries() {
    let text = "é".repeat(500);
    let p = preview(&text);
    assert_eq!(p.chars().count(), envio_mime::PREVIEW_CHARS);
}
