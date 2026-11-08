//! Reply, reply all and forward: table tests.

use envio_mime::{Address, Envelope, ForwardLabels, Quote, ReplyKind, forward, reply};

fn addr(email: &str) -> Address {
    Address::new(None, email)
}

fn original() -> Envelope {
    Envelope {
        message_id: Some("m3@example.com".to_owned()),
        subject: "Plans".to_owned(),
        from: vec![Address::new(Some("Alice"), "alice@example.com")],
        to: vec![addr("Bob@Example.org"), addr("carol@example.com")],
        cc: vec![addr("dan@example.com"), addr("alice@example.com")],
        references: vec!["m1@example.com".to_owned(), "m2@example.com".to_owned()],
        in_reply_to: Some("m2@example.com".to_owned()),
        ..Envelope::default()
    }
}

const QUOTE: Quote<'static> = Quote {
    text: "Hi Bob,\n\nSee you.\n> older\n\n-- \nAlice\n",
    attribution: "On 3 Nov, Alice wrote:",
};

fn me() -> Vec<String> {
    vec!["bob@example.org".to_owned()]
}

#[test]
fn reply_to_sender() {
    let draft = reply(&original(), &QUOTE, &me(), ReplyKind::Sender);
    assert_eq!(draft.to, original().from);
    assert!(draft.cc.is_empty());
    assert_eq!(draft.subject, "Re: Plans");
    assert_eq!(draft.in_reply_to.as_deref(), Some("m3@example.com"));
    assert_eq!(
        draft.references,
        ["m1@example.com", "m2@example.com", "m3@example.com"]
    );
    assert_eq!(
        draft.quoted,
        "On 3 Nov, Alice wrote:\n> Hi Bob,\n>\n> See you.\n>> older\n>"
    );
    assert_eq!(draft.quoted_lines(), 6);
}

#[test]
fn reply_all_drops_me_and_the_new_to() {
    let draft = reply(&original(), &QUOTE, &me(), ReplyKind::All);
    assert_eq!(draft.to, original().from);
    assert_eq!(
        draft.cc,
        [addr("carol@example.com"), addr("dan@example.com")]
    );
}

#[test]
fn reply_to_header_wins() {
    let mut list = original();
    list.reply_to = vec![addr("dev@lists.example.org")];
    let draft = reply(&list, &QUOTE, &me(), ReplyKind::Sender);
    assert_eq!(draft.to, [addr("dev@lists.example.org")]);
}

#[test]
fn replying_to_my_own_message_goes_to_its_recipients() {
    let mut sent = original();
    sent.from = vec![addr("BOB@example.org")];
    sent.to = vec![addr("alice@example.com")];
    let draft = reply(&sent, &QUOTE, &me(), ReplyKind::Sender);
    assert_eq!(draft.to, [addr("alice@example.com")]);
}

#[test]
fn subject_prefixes_are_not_doubled() {
    for (subject, want) in [
        ("Re: Plans", "Re: Plans"),
        ("RE: Plans", "RE: Plans"),
        ("re:Plans", "re:Plans"),
        ("Plans", "Re: Plans"),
        ("", "Re: "),
        ("Fwd: Plans", "Re: Fwd: Plans"),
    ] {
        let mut e = original();
        e.subject = subject.to_owned();
        assert_eq!(reply(&e, &QUOTE, &me(), ReplyKind::Sender).subject, want);
    }
}

#[test]
fn missing_message_id_keeps_references() {
    let mut e = original();
    e.message_id = None;
    let draft = reply(&e, &QUOTE, &me(), ReplyKind::Sender);
    assert_eq!(draft.in_reply_to, None);
    assert_eq!(draft.references, ["m1@example.com", "m2@example.com"]);
}

#[test]
fn parent_without_references_passes_on_its_in_reply_to() {
    // RFC 5322 §3.6.4: the parent's In-Reply-To stands in for References.
    let mut e = original();
    e.references.clear();
    let draft = reply(&e, &QUOTE, &me(), ReplyKind::Sender);
    assert_eq!(draft.references, ["m2@example.com", "m3@example.com"]);
    e.in_reply_to = None;
    let draft = reply(&e, &QUOTE, &me(), ReplyKind::Sender);
    assert_eq!(draft.references, ["m3@example.com"]);
}

#[test]
fn reply_into_outgoing_puts_text_above_quote() {
    let draft = reply(&original(), &QUOTE, &me(), ReplyKind::Sender);
    let quoted = draft.quoted.clone();
    let out = draft.into_outgoing(addr("bob@example.org"), "Sounds good.\n");
    assert_eq!(out.text, format!("Sounds good.\n\n{quoted}"));
    assert_eq!(out.to, original().from);
    assert_eq!(out.in_reply_to.as_deref(), Some("m3@example.com"));
    assert!(out.bcc.is_empty());
}

fn labels() -> ForwardLabels {
    ForwardLabels {
        forwarded_message: "Forwarded message".to_owned(),
        from: "From".to_owned(),
        date: "Date".to_owned(),
        subject: "Subject".to_owned(),
        to: "To".to_owned(),
    }
}

#[test]
fn forward_has_header_block_and_no_references() {
    let draft = forward(&original(), "Body text\n", &labels(), "3 Nov 2026, 09:15");
    assert_eq!(draft.subject, "Fwd: Plans");
    assert_eq!(
        draft.forwarded,
        "---------- Forwarded message ----------\n\
         From: Alice <alice@example.com>\n\
         Date: 3 Nov 2026, 09:15\n\
         Subject: Plans\n\
         To: Bob@Example.org, carol@example.com\n\
         \n\
         Body text"
    );
    let out = draft.into_outgoing(addr("bob@example.org"), "FYI");
    assert!(out.text.starts_with("FYI\n\n---------- Forwarded message"));
    assert!(out.references.is_empty());
    assert_eq!(out.in_reply_to, None);
    assert!(out.to.is_empty());
}

#[test]
fn forward_prefix_is_not_doubled() {
    for (subject, want) in [("Fwd: x", "Fwd: x"), ("FW: x", "FW: x"), ("x", "Fwd: x")] {
        let mut e = original();
        e.subject = subject.to_owned();
        assert_eq!(forward(&e, "", &labels(), "").subject, want);
    }
}
