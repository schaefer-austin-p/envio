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
