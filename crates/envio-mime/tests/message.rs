//! parse, envelope and attachment_bytes against the corpus, by property.

use std::path::Path;

use envio_mime::{Body, Message, attachment_bytes, envelope, parse, quote_text};

fn corpus(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus")
        .join(name);
    std::fs::read(path).unwrap()
}

fn parsed(name: &str) -> (Body, Vec<envio_mime::AttachmentInfo>) {
    match parse(&corpus(name)) {
        Message::Parsed {
            body, attachments, ..
        } => (body, attachments),
        Message::Raw(_) => panic!("{name} came back raw"),
    }
}

#[test]
fn gmail_prefers_html() {
    let (body, attachments) = parsed("gmail-alternative.eml");
    assert!(matches!(body, Body::Html(ref h) if h.contains("<b>Main Street</b>")));
    assert!(attachments.is_empty());
    let e = envelope(&corpus("gmail-alternative.eml")).unwrap();
    assert_eq!(
        e.message_id.as_deref(),
        Some("CAF1a2b3c4d5e6f7@mail.example.com")
    );
    assert_eq!(e.subject, "Lunch on Friday?");
    assert_eq!(e.from[0].name.as_deref(), Some("Alice Example"));
    assert_eq!(e.date, Some(1_793_693_702));
    assert!(e.preview.starts_with("Hi Bob, Are you free"));
}

#[test]
fn outlook_inline_image_is_listed_inline() {
    let (body, attachments) = parsed("outlook-cid.eml");
    assert!(matches!(body, Body::Html(_)));
    assert_eq!(attachments.len(), 1);
    let image = &attachments[0];
    assert_eq!(
        image.content_id.as_deref(),
        Some("image001.png@01DA0000.12345670")
    );
    assert_eq!(image.content_type, "image/png");
    assert!(image.inline);
    assert!(
        !envelope(&corpus("outlook-cid.eml"))
            .unwrap()
            .has_attachments
    );
}

#[test]
fn apple_mail_is_flowed_with_delsp() {
    let (body, _) = parsed("apple-flowed.eml");
    assert!(matches!(
        body,
        Body::Text {
            flowed: true,
            delsp: true,
            ..
        }
    ));
    let e = envelope(&corpus("apple-flowed.eml")).unwrap();
    assert_eq!(e.in_reply_to.as_deref(), Some("trip-1@example.org"));
    assert_eq!(e.references, ["trip-0@example.org", "trip-1@example.org"]);
    assert_eq!(e.thread_root(), "trip-0@example.org");
}

#[test]
fn thunderbird_attachment_bytes_round_trip() {
    let raw = corpus("thunderbird-attachment.eml");
    let (_, attachments) = parsed("thunderbird-attachment.eml");
    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0].filename.as_deref(), Some("contract.pdf"));
    assert_eq!(attachments[0].content_type, "application/pdf");
    assert!(!attachments[0].inline);
    let bytes = attachment_bytes(&raw, 0).unwrap();
    assert!(bytes.starts_with(b"%PDF-1.4"));
    assert_eq!(bytes.len(), attachments[0].size);
    assert_eq!(attachment_bytes(&raw, 1), None);
    let e = envelope(&raw).unwrap();
    assert!(e.has_attachments);
    assert_eq!(e.cc[0].email, "carol@example.com");
}

#[test]
fn mailing_list_reply_to() {
    let e = envelope(&corpus("mailing-list.eml")).unwrap();
    assert_eq!(e.reply_to[0].email, "dev@lists.example.org");
    assert_eq!(e.thread_root(), "list-40@example.org");
}

#[test]
fn legacy_charsets_decode() {
    let e = envelope(&corpus("latin1-qp.eml")).unwrap();
    assert_eq!(e.subject, "Café crème");
    assert_eq!(e.from[0].name.as_deref(), Some("François Example"));
    assert!(e.preview.contains("Le café est prêt. À bientôt !"));
    let e = envelope(&corpus("iso-2022-jp.eml")).unwrap();
    assert_eq!(e.subject, "会議の件");
    assert!(e.preview.starts_with("会議は明日の10時からです。"));
}

#[test]
fn broken_boundary_is_raw() {
    let raw = corpus("broken-boundary.eml");
    assert!(matches!(parse(&raw), Message::Raw(ref t) if t.contains("--some-other-boundary")));
    assert_eq!(attachment_bytes(&raw, 0), None);
    // The list can still show it.
    assert_eq!(envelope(&raw).unwrap().subject, "Report");
}

#[test]
fn broken_encoded_word_is_kept_as_text() {
    let e = envelope(&corpus("encoded-subject.eml")).unwrap();
    assert!(e.subject.starts_with("Grüße aus Köln"));
    assert_eq!(e.from[0].name.as_deref(), Some("Gråta"));
}

#[test]
fn calendar_part_is_an_attachment() {
    let (body, attachments) = parsed("calendar-invite.eml");
    assert!(matches!(body, Body::Text { .. }));
    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0].content_type, "text/calendar");
    let ics = attachment_bytes(&corpus("calendar-invite.eml"), 0).unwrap();
    assert!(ics.starts_with(b"BEGIN:VCALENDAR"));
}

#[test]
fn missing_date_is_none() {
    assert_eq!(envelope(&corpus("no-date.eml")).unwrap().date, None);
}

#[test]
fn garbage_is_raw_and_has_no_envelope() {
    let raw = corpus("garbage.eml");
    assert!(matches!(parse(&raw), Message::Raw(_)));
    assert_eq!(envelope(&raw), None);
}

#[test]
fn binary_and_empty_input_never_panic() {
    let binary: Vec<u8> = (0..=255u8).cycle().take(4096).collect();
    for raw in [&binary[..], b"", b"\xff\xfe\x00", b"Subject: only a header"] {
        let _ = parse(raw);
        let _ = envelope(raw);
        let _ = attachment_bytes(raw, 0);
    }
    assert!(matches!(parse(&binary), Message::Raw(_)));
}

#[test]
fn header_only_blob_gives_envelope_without_preview() {
    let raw = b"From: a@example.org\r\nSubject: Hi\r\nMessage-ID: <h@x>\r\n\r\n";
    let e = envelope(raw).unwrap();
    assert_eq!(e.subject, "Hi");
    assert_eq!(e.preview, "");
}

/// What reply and forward quote: the plain alternative, the HTML as text,
/// the plain body, or the raw text of a "Strange message".
#[test]
fn quote_text_picks_the_plain_text() {
    let cases: [(&str, &[&str], &[&str]); 5] = [
        (
            "gmail-alternative.eml",
            &["Hi Bob,\n\nAre you free for lunch on Friday? The place on Main Street"],
            &["<div", "<b>"],
        ),
        (
            "outlook-cid.eml",
            &["Hi Bob,", "The chart is below.", "Carol"],
            &["<p", "MsoNormal", "=3D"],
        ),
        (
            "apple-flowed.eml",
            &[
                "the space at the end says it goes on.",
                "> Did you keep the receipts?\n",
            ],
            &["=20", "at  \n"],
        ),
        ("garbage.eml", &["PK this is not mail", "<<<<<<< HEAD"], &[]),
        ("broken-boundary.eml", &["--some-other-boundary"], &[]),
    ];
    for (name, has, lacks) in cases {
        let text = quote_text(&corpus(name));
        for want in has {
            assert!(text.contains(want), "{name}: no {want:?} in {text:?}");
        }
        for unwanted in lacks {
            assert!(!text.contains(unwanted), "{name}: {unwanted:?} in {text:?}");
        }
    }
}

fn plain_mail(params: &str, body: &str) -> Vec<u8> {
    format!(
        "From: a@example.com\r\nSubject: t\r\nContent-Type: text/plain; charset=utf-8{params}\r\n\r\n{body}"
    )
    .into_bytes()
}

#[test]
fn quote_text_unflows_format_flowed() {
    let raw = plain_mail("; Format=Flowed", "one two \r\nthree\r\n four\r\n");
    assert_eq!(quote_text(&raw), "one two three\nfour\n");
}

#[test]
fn quote_text_drops_the_delsp_space() {
    let raw = plain_mail("; format=flowed; delsp=yes", "exam \r\nple text\r\n");
    assert_eq!(quote_text(&raw), "example text\n");
}

#[test]
fn quote_text_keeps_fixed_text_as_sent() {
    let raw = plain_mail("", "one two \r\nthree\r\n");
    assert_eq!(quote_text(&raw), "one two \r\nthree\r\n");
}

#[test]
fn quote_text_unflows_inside_quotes_by_depth() {
    let raw = plain_mail(
        "; format=flowed",
        "> quoted one \r\n> quoted two\r\n>> deeper \r\n>> still\r\nreply\r\n",
    );
    assert_eq!(
        quote_text(&raw),
        "> quoted one quoted two\n>> deeper still\nreply\n"
    );
}
