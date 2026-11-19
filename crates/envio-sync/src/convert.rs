//! Translations between envio-net's view of the server and envio-store's
//! rows. Pure: no I/O.

use envio_net::imap::{Flag, MessageFlags, Role, Summary};
use envio_store::{Envelope, Flags, FolderRole, Message, NewMessage};

/// The store's flag bits from the server's flags. Keywords are not
/// cached.
pub fn store_flags(flags: &MessageFlags) -> Flags {
    [
        (flags.seen, Flags::SEEN),
        (flags.answered, Flags::ANSWERED),
        (flags.flagged, Flags::FLAGGED),
        (flags.deleted, Flags::DELETED),
        (flags.draft, Flags::DRAFT),
    ]
    .into_iter()
    .filter(|(set, _)| *set)
    .fold(Flags::empty(), |all, (_, flag)| all.with(flag))
}

/// The IMAP flags named by `flags`, for STORE.
pub fn net_flags(flags: Flags) -> Vec<Flag> {
    [
        (Flags::SEEN, Flag::Seen),
        (Flags::ANSWERED, Flag::Answered),
        (Flags::FLAGGED, Flag::Flagged),
        (Flags::DELETED, Flag::Deleted),
        (Flags::DRAFT, Flag::Draft),
    ]
    .into_iter()
    .filter(|(bit, _)| flags.contains(*bit))
    .map(|(_, flag)| flag)
    .collect()
}

pub fn folder_role(role: Option<Role>) -> FolderRole {
    match role {
        Some(Role::Inbox) => FolderRole::Inbox,
        Some(Role::Sent) => FolderRole::Sent,
        Some(Role::Drafts) => FolderRole::Drafts,
        Some(Role::Trash) => FolderRole::Trash,
        Some(Role::Junk) => FolderRole::Junk,
        Some(Role::Archive) => FolderRole::Archive,
        Some(Role::All | Role::Flagged) | None => FolderRole::Other,
    }
}

/// The row for a message from its summary. A header that doesn't parse
/// still gives a row (empty subject and sender), so sync goes on; the
/// date falls back to the server's arrival time.
pub fn new_message(summary: &Summary) -> NewMessage {
    let parsed = envio_mime::envelope(&summary.header).unwrap_or_default();
    NewMessage {
        uid: summary.uid,
        size: summary.size,
        envelope: Envelope {
            has_attachments: envio_mime::header_suggests_attachments(&summary.header),
            ..store_envelope(&parsed, summary.internal_date.unwrap_or(0))
        },
        flags: store_flags(&summary.flags),
        preview: parsed.preview,
    }
}

/// The people a message's header names (From, then To) as (name, address)
/// pairs, and when it was written: what autocomplete remembers.
pub fn seen_addresses(summary: &Summary) -> (Vec<(String, String)>, i64) {
    let parsed = envio_mime::envelope(&summary.header).unwrap_or_default();
    let when = parsed.date.or(summary.internal_date).unwrap_or(0);
    let found = parsed
        .from
        .iter()
        .chain(&parsed.to)
        .map(|a| (a.name.clone().unwrap_or_default(), a.email.clone()))
        .collect();
    (found, when)
}

/// A cached message as a row to write back (Undo puts a removed one back).
pub fn row_of(message: &Message) -> NewMessage {
    NewMessage {
        uid: message.uid,
        size: message.size,
        envelope: message.envelope.clone(),
        flags: message.flags,
        preview: message.preview.clone(),
    }
}

/// `message`'s row again with what the whole message adds: the preview and
/// the exact attachment flag (sync only guessed it from the header).
pub fn with_body(message: &Message, eml: &[u8]) -> NewMessage {
    let parsed = envio_mime::envelope(eml).unwrap_or_default();
    let mut row = row_of(message);
    row.preview = parsed.preview;
    row.envelope.has_attachments = parsed.has_attachments;
    row
}

/// The Message-ID as the store and the journal keep it: in angle brackets.
pub fn bracketed(message_id: &str) -> String {
    format!("<{message_id}>")
}

/// `Name <email>` for the cached list. A name that holds an address special
/// (a comma, `@`, quote, bracket) is quoted with `\` escapes, so a reader
/// can split the comma-joined list again.
fn shown_address(address: &envio_mime::Address) -> String {
    match &address.name {
        Some(name) if name.contains(|c| ",;:@<>()[]\"\\".contains(c)) => {
            let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
            format!("\"{escaped}\" <{}>", address.email)
        }
        _ => address.display(),
    }
}

fn store_envelope(parsed: &envio_mime::Envelope, arrived: i64) -> Envelope {
    let shown = |list: &[envio_mime::Address], one: fn(&envio_mime::Address) -> String| {
        list.iter().map(one).collect::<Vec<_>>().join(", ")
    };
    Envelope {
        message_id: parsed.message_id.as_deref().map(bracketed),
        subject: parsed.subject.clone(),
        from: shown(&parsed.from, envio_mime::Address::display),
        to: shown(&parsed.to, shown_address),
        date: parsed.date.unwrap_or(arrived),
        in_reply_to: parsed.in_reply_to.as_deref().map(bracketed),
        references: parsed.references.iter().map(|id| bracketed(id)).collect(),
        has_attachments: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recipient_name_with_a_special_is_cached_quoted() {
        let plain = envio_mime::Address::new(Some("Cy"), "cy@example.org");
        assert_eq!(shown_address(&plain), "Cy <cy@example.org>");
        let comma = envio_mime::Address::new(Some("jo@x, Inc"), "a@example.org");
        assert_eq!(shown_address(&comma), r#""jo@x, Inc" <a@example.org>"#);
        let quote = envio_mime::Address::new(Some(r#"A"B\"#), "a@example.org");
        assert_eq!(shown_address(&quote), r#""A\"B\\" <a@example.org>"#);
    }

    fn summary(header: &[u8]) -> Summary {
        Summary {
            uid: 12,
            flags: MessageFlags::from_flags([Flag::Seen, Flag::Keyword("$Junk".into())]),
            size: 3_456,
            internal_date: Some(1_800_000_000),
            modseq: Some(9),
            header: header.to_vec(),
        }
    }

    #[test]
    fn flags_go_both_ways_without_keywords() {
        let flags =
            MessageFlags::from_flags([Flag::Seen, Flag::Flagged, Flag::Keyword("x".into())]);
        let bits = store_flags(&flags);
        assert_eq!(bits, Flags::SEEN.with(Flags::FLAGGED));
        assert_eq!(net_flags(bits), [Flag::Seen, Flag::Flagged]);
        assert!(net_flags(Flags::empty()).is_empty());
    }

    #[test]
    fn roles_without_a_store_twin_are_other() {
        assert_eq!(folder_role(Some(Role::Trash)), FolderRole::Trash);
        assert_eq!(folder_role(Some(Role::All)), FolderRole::Other);
        assert_eq!(folder_role(None), FolderRole::Other);
    }

    #[test]
    fn a_summary_becomes_a_row_with_bracketed_ids() {
        let header = b"From: Ada <ada@example.org>\r\nTo: bob@example.org, Cy <cy@example.org>\r\n\
            Subject: Plans\r\nMessage-ID: <m1@example.org>\r\nIn-Reply-To: <m0@example.org>\r\n\
            References: <r1@example.org> <m0@example.org>\r\n\
            Date: Sun, 08 Nov 2026 10:00:00 +0000\r\n\r\n";
        let row = new_message(&summary(header));
        assert_eq!(row.uid, 12);
        assert_eq!(row.size, 3_456);
        assert_eq!(row.flags, Flags::SEEN);
        assert_eq!(row.envelope.message_id.as_deref(), Some("<m1@example.org>"));
        assert_eq!(
            row.envelope.in_reply_to.as_deref(),
            Some("<m0@example.org>")
        );
        assert_eq!(
            row.envelope.references,
            ["<r1@example.org>", "<m0@example.org>"]
        );
        assert_eq!(row.envelope.from, "Ada <ada@example.org>");
        assert_eq!(row.envelope.to, "bob@example.org, Cy <cy@example.org>");
        assert_eq!(row.envelope.subject, "Plans");
        assert_eq!(row.envelope.date, 1_794_132_000);
    }

    #[test]
    fn a_header_that_does_not_parse_still_gives_a_row() {
        let row = new_message(&summary(b""));
        assert_eq!(row.envelope.subject, "");
        assert_eq!(row.envelope.message_id, None);
        assert_eq!(row.envelope.date, 1_800_000_000);
    }

    #[test]
    fn a_mixed_header_guesses_an_attachment_and_a_full_parse_corrects_it() {
        let mixed = b"Subject: Files\r\nMessage-ID: <f@example.org>\r\n\
            Content-Type: multipart/mixed; boundary=b\r\n\r\n";
        let plain = b"Subject: Hi\r\nMessage-ID: <p@example.org>\r\n\
            Content-Type: multipart/alternative; boundary=b\r\n\r\n";
        assert!(new_message(&summary(mixed)).envelope.has_attachments);
        assert!(!new_message(&summary(plain)).envelope.has_attachments);
        assert!(
            !new_message(&summary(b"Subject: x\r\n\r\n"))
                .envelope
                .has_attachments
        );

        // The mixed guess was wrong: the body has only text.
        let db = envio_store::Db::open_in_memory().unwrap();
        let folder = db.upsert_folder("1", "INBOX", FolderRole::Inbox).unwrap();
        let id = db
            .upsert_message(folder, &new_message(&summary(mixed)))
            .unwrap();
        let cached = db.message(id).unwrap();
        assert!(cached.envelope.has_attachments);
        let eml = b"Subject: Files\r\nMessage-ID: <f@example.org>\r\n\
            Content-Type: multipart/mixed; boundary=b\r\n\r\n\
            --b\r\nContent-Type: text/plain\r\n\r\nJust text\r\n--b--\r\n";
        assert!(!with_body(&cached, eml).envelope.has_attachments);
        // And a body with an attachment where the header said nothing.
        let cached = db
            .message(
                db.upsert_message(folder, &new_message(&summary(plain)))
                    .unwrap(),
            )
            .unwrap();
        let eml = b"Subject: Hi\r\nMessage-ID: <p@example.org>\r\n\
            Content-Type: multipart/mixed; boundary=b\r\n\r\n\
            --b\r\nContent-Type: text/plain\r\n\r\nSee file\r\n--b\r\n\
            Content-Type: application/pdf; name=a.pdf\r\n\
            Content-Disposition: attachment; filename=a.pdf\r\n\
            Content-Transfer-Encoding: base64\r\n\r\nJVBERi0=\r\n--b--\r\n";
        assert!(with_body(&cached, eml).envelope.has_attachments);
    }

    #[test]
    fn a_body_brings_the_preview() {
        let header = b"Subject: Hi\r\nMessage-ID: <p@example.org>\r\n\r\n";
        let row = new_message(&summary(header));
        let db = envio_store::Db::open_in_memory().unwrap();
        let folder = db.upsert_folder("1", "INBOX", FolderRole::Inbox).unwrap();
        let id = db.upsert_message(folder, &row).unwrap();
        let cached = db.message(id).unwrap();
        let eml = b"Subject: Hi\r\nMessage-ID: <p@example.org>\r\n\r\nSee you at noon.\r\n";
        let full = with_body(&cached, eml);
        assert_eq!(full.preview, "See you at noon.");
        assert_eq!(full.envelope, cached.envelope);
        assert_eq!(full.size, 3_456);
    }

    #[test]
    fn flags_round_trip_for_every_combination() {
        let bits = [
            Flags::SEEN,
            Flags::ANSWERED,
            Flags::FLAGGED,
            Flags::DELETED,
            Flags::DRAFT,
        ];
        for combo in 0u8..32 {
            let flags = bits
                .iter()
                .enumerate()
                .filter(|(n, _)| combo & (1 << n) != 0)
                .fold(Flags::empty(), |all, (_, bit)| all.with(*bit));
            assert_eq!(
                store_flags(&MessageFlags::from_flags(net_flags(flags))),
                flags,
                "combination {combo:05b}"
            );
        }
    }
}
