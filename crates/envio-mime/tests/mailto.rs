//! mailto: links (RFC 6068): table tests.

use envio_mime::{Address, MailtoDraft, parse_mailto};

fn addr(email: &str) -> Address {
    Address::new(None, email)
}

#[test]
fn mailto_links() {
    assert_eq!(parse_mailto("https://example.com"), None);
    assert_eq!(parse_mailto("mailto"), None);
    assert_eq!(parse_mailto("mailto:"), Some(MailtoDraft::default()));
    let draft = parse_mailto(
        "MAILTO:alice@example.com,bob@example.org?to=carol@example.com\
         &cc=dan%40example.com&bcc=eve@example.net&subject=Hello%20there%2C%20%C3%BC+plus\
         &body=Line%201%0D%0ALine%202&In-Reply-To=%3Cm1@example.com%3E\
         &X-Evil=1&from=mallory@example.com&attach=/etc/passwd",
    )
    .unwrap();
    assert_eq!(
        draft.to,
        [
            addr("alice@example.com"),
            addr("bob@example.org"),
            addr("carol@example.com")
        ]
    );
    assert_eq!(draft.cc, [addr("dan@example.com")]);
    assert_eq!(draft.bcc, [addr("eve@example.net")]);
    assert_eq!(draft.subject, "Hello there, ü+plus");
    assert_eq!(draft.body, "Line 1\nLine 2");
    assert_eq!(draft.in_reply_to.as_deref(), Some("m1@example.com"));
}

#[test]
fn mailto_keeps_bad_percent_escapes() {
    let draft = parse_mailto("mailto:a@example.com?subject=100%25%zz%").unwrap();
    assert_eq!(draft.subject, "100%%zz%");
}

#[test]
fn mailto_plus_after_percent_stays_literal() {
    let draft = parse_mailto("mailto:a@example.com?subject=a%+1b").unwrap();
    assert_eq!(draft.subject, "a%+1b");
}

#[test]
fn mailto_drops_control_characters() {
    let draft = parse_mailto(
        "mailto:a@example.com%0D%0ABcc:%20evil@y.org?cc=ok@example.com%0Aevil@y.org\
         &subject=Hi%0D%0ABcc:%20x%00!&body=a%0D%0Ab%0Bc%09d\
         &in-reply-to=%3Cp@e.com%0D%0ABcc:%20evil@y.org%3E",
    )
    .unwrap();
    assert!(draft.to.is_empty());
    assert!(draft.cc.is_empty());
    assert_eq!(draft.subject, "HiBcc: x!");
    assert_eq!(draft.body, "a\nbc\td");
    assert_eq!(draft.in_reply_to, None);
}

#[test]
fn mailto_quoted_display_name_keeps_its_comma() {
    let draft = parse_mailto(
        "mailto:?to=%22Doe%2C%20Jane%22%20%3Cj%40x.example%3E%2C%20b%40y.example\
         &cc=%22Smith%2C%20A%22%20%3Ca%40z.example%3E",
    )
    .unwrap();
    assert_eq!(
        draft.to,
        [
            Address::new(Some("Doe, Jane"), "j@x.example"),
            addr("b@y.example")
        ]
    );
    assert_eq!(draft.cc.len(), 1);
    assert_eq!(draft.cc[0].email, "a@z.example");
}

#[test]
fn mailto_plain_addr_specs_still_split_at_commas() {
    let draft = parse_mailto("mailto:a@x.example,b@y.example?to=c@z.example,d@w.example").unwrap();
    let emails: Vec<_> = draft.to.iter().map(|a| a.email.as_str()).collect();
    assert_eq!(
        emails,
        ["a@x.example", "b@y.example", "c@z.example", "d@w.example"]
    );
}
