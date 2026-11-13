//! Snapshot of every corpus message: envelope, attachments, rendered body.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;

use envio_mime::{Address, Message, RenderOptions, envelope, parse, render};

fn options() -> RenderOptions {
    RenderOptions {
        allow_remote_images: false,
        cid_urls: HashMap::from([(
            "image001.png@01DA0000.12345670".to_owned(),
            "image://envio/part/0".to_owned(),
        )]),
        blocked_label: "[image]".to_owned(),
        max_image_width: 600,
    }
}

fn names(addresses: &[Address]) -> String {
    let list: Vec<String> = addresses.iter().map(Address::display).collect();
    format!("[{}]", list.join(", "))
}

/// The snapshot text for one corpus file.
fn describe(raw: &[u8]) -> String {
    let mut out = String::new();
    match envelope(raw) {
        Some(e) => {
            writeln!(out, "message_id: {:?}", e.message_id).unwrap();
            writeln!(out, "subject: {:?}", e.subject).unwrap();
            writeln!(out, "from: {}", names(&e.from)).unwrap();
            writeln!(out, "to: {}", names(&e.to)).unwrap();
            writeln!(out, "cc: {}", names(&e.cc)).unwrap();
            writeln!(out, "reply_to: {}", names(&e.reply_to)).unwrap();
            writeln!(out, "date: {:?}", e.date).unwrap();
            writeln!(out, "in_reply_to: {:?}", e.in_reply_to).unwrap();
            writeln!(out, "references: {:?}", e.references).unwrap();
            writeln!(out, "thread_root: {:?}", e.thread_root()).unwrap();
            writeln!(out, "preview: {:?}", e.preview).unwrap();
            writeln!(out, "has_attachments: {}", e.has_attachments).unwrap();
        }
        None => writeln!(out, "envelope: none").unwrap(),
    }
    let message = parse(raw);
    match &message {
        Message::Parsed { attachments, .. } => {
            for a in attachments {
                writeln!(
                    out,
                    "attachment {}: {:?} {} {} bytes cid={:?} inline={}",
                    a.index, a.filename, a.content_type, a.size, a.content_id, a.inline
                )
                .unwrap();
            }
        }
        Message::Raw(_) => writeln!(out, "raw message").unwrap(),
    }
    let rendered = render(&message, &options());
    writeln!(
        out,
        "blocked: {} trackers: {}",
        rendered.blocked_images, rendered.removed_trackers
    )
    .unwrap();
    writeln!(out, "---").unwrap();
    out.push_str(&rendered.html);
    out
}

fn snapshot(name: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus")
        .join(name);
    let raw = std::fs::read(&path).unwrap();
    insta::assert_snapshot!(name, describe(&raw));
}

#[test]
fn gmail_alternative() {
    snapshot("gmail-alternative.eml");
}

#[test]
fn outlook_cid() {
    snapshot("outlook-cid.eml");
}

#[test]
fn apple_flowed() {
    snapshot("apple-flowed.eml");
}

#[test]
fn thunderbird_attachment() {
    snapshot("thunderbird-attachment.eml");
}

#[test]
fn mailing_list() {
    snapshot("mailing-list.eml");
}

#[test]
fn latin1_qp() {
    snapshot("latin1-qp.eml");
}

#[test]
fn iso_2022_jp() {
    snapshot("iso-2022-jp.eml");
}

#[test]
fn broken_boundary() {
    snapshot("broken-boundary.eml");
}

#[test]
fn encoded_subject() {
    snapshot("encoded-subject.eml");
}

#[test]
fn calendar_invite() {
    snapshot("calendar-invite.eml");
}

#[test]
fn no_date() {
    snapshot("no-date.eml");
}

#[test]
fn garbage() {
    snapshot("garbage.eml");
}

#[test]
fn receipt_like() {
    snapshot("receipt-like.eml");
}

#[test]
fn newsletter_like() {
    snapshot("newsletter-like.eml");
}

#[test]
fn notice_like() {
    snapshot("notice-like.eml");
}
