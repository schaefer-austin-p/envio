//! Address specials (`< > , ; : " ( ) [ ] \`) never reach a header or the
//! SMTP recipients.

use envio_mime::{Address, Outgoing, build, parse_mailto};

fn outgoing() -> Outgoing {
    Outgoing {
        from: Address::new(None, "bob@example.org"),
        to: vec![Address::new(None, "alice@example.com")],
        text: "hi".to_owned(),
        ..Outgoing::default()
    }
}

fn header_text(bytes: &[u8], name: &str) -> String {
    let text = String::from_utf8_lossy(bytes);
    let head = text.split("\r\n\r\n").next().unwrap_or("");
    head.replace("\r\n ", " ")
        .lines()
        .filter(|l| l.to_ascii_lowercase().starts_with(&format!("{name}:")))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn build_skips_recipients_with_address_specials() {
    let mut o = outgoing();
    for special in "<>,;:\"()[]\\".chars() {
        o.to.push(Address {
            name: None,
            email: format!("a@b.test{special},evil@x.test"),
        });
    }
    o.cc.push(Address {
        name: None,
        email: "a@b.test>,evil@x.test".to_owned(),
    });
    o.bcc.push(Address {
        name: None,
        email: "c@b.test>,evil@x.test".to_owned(),
    });
    let built = build(&o, 0, "x@example.org");
    assert_eq!(built.recipients, ["alice@example.com"]);
    let to = header_text(&built.bytes, "to");
    assert!(!to.contains("evil"), "{to}");
    assert!(!String::from_utf8_lossy(&built.bytes).contains("evil@x.test"));
}

#[test]
fn build_skips_ids_with_specials_but_keeps_brackets() {
    let mut o = outgoing();
    o.in_reply_to = Some("p@e.com>,evil@x.test".to_owned());
    o.references = vec!["<r@e.com>".to_owned(), "q@e.com;evil@x.test".to_owned()];
    let built = build(&o, 0, "x@example.org");
    let text = String::from_utf8_lossy(&built.bytes);
    assert!(!text.contains("evil@x.test"), "{text}");
    assert!(header_text(&built.bytes, "in-reply-to").is_empty());
    assert!(header_text(&built.bytes, "references").contains("r@e.com"));
}

#[test]
fn from_address_specials_are_stripped() {
    for special in "<>,;:\"()[]\\".chars() {
        let mut o = outgoing();
        o.from = Address {
            name: None,
            email: format!("a@b.test{special},evil@x.test"),
        };
        let built = build(&o, 0, "x@example.org");
        let from = header_text(&built.bytes, "from");
        // One mailbox, and nothing but the stripped text of the email in it.
        assert_eq!(from, "From: <a@b.testevil@x.test>", "{special}: {from}");
    }
    let mut o = outgoing();
    o.from = Address {
        name: Some("Bob".to_owned()),
        email: "a@b.test>,evil@x.test".to_owned(),
    };
    let from = header_text(&build(&o, 0, "x@example.org").bytes, "from");
    assert_eq!(from, "From: \"Bob\" <a@b.testevil@x.test>");
}

#[test]
fn address_parse_rejects_specials() {
    for bad in [
        "a@b.test>,evil@x.test",
        "a@b.test;evil@x.test",
        "a@b.test:x",
        "a(b)@c.test",
        "a[b]@c.test",
        "a\\b@c.test",
        "a\"b@c.test",
        "a,b@c.test",
    ] {
        assert_eq!(Address::parse(bad), None, "{bad}");
    }
    assert_eq!(
        Address::parse("Bob <bob@example.org>"),
        Some(Address::new(Some("Bob"), "bob@example.org"))
    );
}

#[test]
fn mailto_address_specials_never_reach_a_recipient() {
    let draft = parse_mailto("mailto:?to=a@b.test%3E,evil@x.test").unwrap();
    // The comma splits the value: `a@b.test>` has a special and is skipped,
    // and `evil@x.test` is a clean address of its own, so it stays. What
    // matters is that no address carries the `>`.
    assert_eq!(draft.to, [Address::new(None, "evil@x.test")]);
    assert!(draft.to.iter().all(|a| !a.email.contains('>')));
}
