//! Built mail parsed back: every field, no Bcc, our Message-ID.

use envio_mime::{
    Address, Body, Message, Outgoing, OutgoingAttachment, attachment_bytes, build, envelope,
    new_message_id, parse,
};

fn outgoing() -> Outgoing {
    Outgoing {
        from: Address::new(Some("Bob Example"), "bob@example.org"),
        to: vec![
            Address::new(Some("Alice Example"), "alice@example.com"),
            Address::new(None, "carol@example.com"),
        ],
        cc: vec![Address::new(Some("Dan"), "dan@example.com")],
        bcc: vec![
            Address::new(Some("Secret"), "secret@example.net"),
            Address::new(None, "ALICE@example.com"),
        ],
        subject: "Grüße – the plan".to_owned(),
        text: "Line one\nLine two with ünïcödé\n\n> quoted\n".to_owned(),
        in_reply_to: Some("parent@example.com".to_owned()),
        references: vec![
            "root@example.com".to_owned(),
            "parent@example.com".to_owned(),
        ],
        attachments: vec![OutgoingAttachment {
            filename: "notes.pdf".to_owned(),
            content_type: "application/pdf".to_owned(),
            bytes: b"%PDF-1.4 test".to_vec(),
        }],
    }
}

const ID: &str = "0123456789abcdef0123456789abcdef@example.org";

#[test]
fn built_message_parses_back() {
    let built = build(&outgoing(), 1_794_000_000, ID);
    assert_eq!(built.message_id, ID);
    let e = envelope(&built.bytes).unwrap();
    assert_eq!(e.message_id.as_deref(), Some(ID));
    assert_eq!(e.subject, "Grüße – the plan");
    assert_eq!(
        e.from,
        vec![Address::new(Some("Bob Example"), "bob@example.org")]
    );
    assert_eq!(e.to, outgoing().to);
    assert_eq!(e.cc, outgoing().cc);
    assert_eq!(e.date, Some(1_794_000_000));
    assert_eq!(e.in_reply_to.as_deref(), Some("parent@example.com"));
    assert_eq!(e.references, ["root@example.com", "parent@example.com"]);
    assert!(e.has_attachments);
    let Message::Parsed {
        body, attachments, ..
    } = parse(&built.bytes)
    else {
        panic!("built message came back raw");
    };
    let Body::Text { text, .. } = body else {
        panic!("expected a text body");
    };
    assert_eq!(
        text.replace("\r\n", "\n").trim_end(),
        outgoing().text.trim_end()
    );
    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0].filename.as_deref(), Some("notes.pdf"));
    assert_eq!(attachment_bytes(&built.bytes, 0).unwrap(), b"%PDF-1.4 test");
}

#[test]
fn bcc_is_never_written_but_is_a_recipient() {
    let built = build(&outgoing(), 1_794_000_000, ID);
    let text = String::from_utf8_lossy(&built.bytes).to_ascii_lowercase();
    assert!(!text.contains("bcc:"));
    assert!(!text.contains("secret@example.net"));
    assert_eq!(
        built.recipients,
        [
            "alice@example.com",
            "carol@example.com",
            "dan@example.com",
            "secret@example.net"
        ]
    );
}

#[test]
fn no_hostname_or_client_headers() {
    let built = build(&outgoing(), 1_794_000_000, ID);
    let text = String::from_utf8_lossy(&built.bytes).to_ascii_lowercase();
    assert!(text.contains(&format!("message-id: <{}>", ID.to_ascii_lowercase())));
    assert!(!text.contains("localhost"));
    assert!(!text.contains("user-agent"));
    assert!(!text.contains("x-mailer"));
    let host = std::fs::read_to_string("/etc/hostname").unwrap_or_default();
    let host = host.trim().to_ascii_lowercase();
    if !host.is_empty() {
        assert!(!text.contains(&host), "hostname {host} leaked");
    }
}

#[test]
fn minimal_message_without_recipients_lists() {
    let outgoing = Outgoing {
        from: Address::new(None, "bob@example.org"),
        to: vec![Address::new(None, "alice@example.com")],
        subject: "Hi".to_owned(),
        text: "Hello".to_owned(),
        ..Outgoing::default()
    };
    let built = build(&outgoing, 0, "x@example.org");
    let text = String::from_utf8_lossy(&built.bytes).to_ascii_lowercase();
    assert!(!text.contains("cc:"));
    assert!(!text.contains("in-reply-to:"));
    assert!(!text.contains("references:"));
    assert_eq!(built.recipients, ["alice@example.com"]);
}

#[test]
fn message_ids_are_random_and_use_the_sender_domain() {
    let a = new_message_id("Bob@Example.org");
    let b = new_message_id("bob@example.org");
    assert_ne!(a, b);
    let (random, domain) = a.split_once('@').unwrap();
    assert_eq!(random.len(), 32);
    assert!(random.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(domain, "Example.org");
    assert!(new_message_id("no-at-sign").ends_with("@invalid"));
    assert!(new_message_id("a@").ends_with("@invalid"));
}

fn assert_header_block_clean(bytes: &[u8]) {
    let text = String::from_utf8_lossy(bytes);
    let head = text.split("\r\n\r\n").next().unwrap();
    for line in head.split("\r\n") {
        let folded = line.starts_with(' ') || line.starts_with('\t');
        let (name, _) = line.split_once(':').unwrap_or(("", ""));
        let valid = !name.is_empty() && name.chars().all(|c| c.is_ascii_graphic());
        assert!(folded || valid, "bad header line {line:?}");
    }
    let has_bcc_header = head
        .split("\r\n")
        .any(|l| l.to_ascii_lowercase().starts_with("bcc:"));
    assert!(!has_bcc_header, "{head}");
}

fn simple() -> Outgoing {
    Outgoing {
        from: Address::new(None, "bob@example.org"),
        to: vec![Address::new(None, "alice@example.com")],
        subject: "Hi".to_owned(),
        text: "Hello".to_owned(),
        ..Outgoing::default()
    }
}

#[test]
fn injected_in_reply_to_is_not_written() {
    let mut o = simple();
    o.in_reply_to = Some("p@e.com\r\nBcc: evil@y.org".to_owned());
    o.references = vec!["ok@e.com".to_owned(), "r@e.com\nBcc: evil@y.org".to_owned()];
    let built = build(&o, 0, "x@example.org");
    assert_header_block_clean(&built.bytes);
    let text = String::from_utf8_lossy(&built.bytes);
    assert!(!text.contains("evil@y.org"));
    assert!(!text.to_ascii_lowercase().contains("in-reply-to:"));
    assert!(text.contains("ok@e.com"));
}

#[test]
fn mailto_injection_parsed_then_built_stays_clean() {
    let draft = envio_mime::parse_mailto(
        "mailto:alice@example.com%0D%0ABcc:%20evil@y.org\
         ?to=bob@example.com%0D%0ABcc:%20evil@y.org\
         &in-reply-to=%3Cp@e.com%0D%0ABcc:%20evil@y.org%3E&subject=S%0D%0ABcc:%20evil@y.org",
    )
    .unwrap();
    let mut o = simple();
    o.to = draft.to;
    o.subject = draft.subject;
    o.in_reply_to = draft.in_reply_to;
    let built = build(&o, 0, "x@example.org");
    assert_header_block_clean(&built.bytes);
    assert!(built.recipients.is_empty());
}

#[test]
fn build_skips_unsafe_recipients_and_ids() {
    let mut o = simple();
    o.to.push(Address {
        name: None,
        email: "a@b.com\r\nBcc: evil@y.org".to_owned(),
    });
    o.cc.push(Address {
        name: Some("Eve\r\nBcc: evil@y.org".to_owned()),
        email: "eve@example.com".to_owned(),
    });
    o.bcc.push(Address {
        name: None,
        email: "x y@example.com".to_owned(),
    });
    o.subject = "Hi\r\nBcc: evil@y.org".to_owned();
    let built = build(&o, 0, "id@e.com\r\nBcc: evil@y.org");
    assert_header_block_clean(&built.bytes);
    assert_eq!(built.recipients, ["alice@example.com", "eve@example.com"]);
    assert!(!built.message_id.contains(['\r', '\n', ' ']));
    assert!(built.message_id.ends_with("@example.org"));
}

#[test]
fn message_id_domain_is_restricted() {
    assert!(new_message_id("a@ex\r\nBcc: x.org").ends_with("@invalid"));
    assert!(new_message_id("a@ex ample.org").ends_with("@invalid"));
    assert!(new_message_id("a@ex<ample>.org").ends_with("@invalid"));
    assert!(new_message_id("a@sub-1.example.org").ends_with("@sub-1.example.org"));
}

#[test]
fn attachment_content_type_is_validated() {
    let mut o = simple();
    o.attachments = vec![
        OutgoingAttachment {
            filename: "a.txt".to_owned(),
            content_type: "text/plain\r\nBcc: e@e.org".to_owned(),
            bytes: b"x".to_vec(),
        },
        OutgoingAttachment {
            filename: "b.txt".to_owned(),
            content_type: "text/plain\nBcc: e@e.org".to_owned(),
            bytes: b"x".to_vec(),
        },
        OutgoingAttachment {
            filename: "c.png".to_owned(),
            content_type: "image/svg+xml".to_owned(),
            bytes: b"x".to_vec(),
        },
        OutgoingAttachment {
            filename: "d".to_owned(),
            content_type: "nonsense".to_owned(),
            bytes: b"x".to_vec(),
        },
    ];
    let built = build(&o, 0, "x@example.org");
    let text = String::from_utf8_lossy(&built.bytes);
    assert!(!text.contains("e@e.org"), "{text}");
    assert!(
        !text
            .lines()
            .any(|l| l.to_ascii_lowercase().starts_with("bcc:"))
    );
    assert!(text.contains("image/svg+xml"));
    assert_eq!(text.matches("application/octet-stream").count(), 3);
}

#[test]
fn message_id_domain_is_punycoded() {
    assert!(new_message_id("a@例え.jp").ends_with("@xn--r8jz45g.jp"));
    assert!(new_message_id("a@bücher.example").ends_with("@xn--bcher-kva.example"));
    assert!(new_message_id("a@ex\u{200B}ample.org").ends_with("@invalid"));
    assert!(new_message_id("a@\u{202E}example.org").ends_with("@invalid"));
}
