//! List rows from cached messages: what a list shows for each.

use std::collections::HashMap;

use envio_mime::Address;
use envio_store::{Db, FolderId, FolderRole, Message};

use super::local_rows::recipients_of;
use crate::api::view::MessageRow;
use crate::text;

/// What names a sender: the display name, unless it is missing or looks like
/// an address (`"billing@paypal.com" <x@evil.example>`), then the address.
pub(super) fn sender_label(name: Option<&str>, email: &str) -> String {
    let name = name.map(envio_mime::strip_bidi_controls);
    match name {
        Some(n) if !n.trim().is_empty() && !n.contains(['@', '\u{FF20}', '\u{FE6B}']) => n,
        _ => email.to_owned(),
    }
}

/// The key the stamp is tinted by: a Rust mirror of `stampTint.addressOf`
/// applied to `address`, else to `from` (inside the last `<...>`, else the
/// whole line; trimmed, lowercased).
pub(super) fn stamp_key(address: &str, from: &str) -> String {
    let line = if address.is_empty() { from } else { address };
    let line = line.trim_end();
    let inner = line
        .strip_suffix('>')
        .and_then(|body| body.rfind(['<', '>']).map(|i| (body, i)))
        .filter(|(body, i)| body[*i..].starts_with('<'))
        .map(|(body, i)| &body[i + 1..]);
    inner.unwrap_or(line).trim().to_lowercase()
}

/// The sender as a list shows it: (label, the address, or the key the stamp
/// is tinted by when the line does not parse).
fn sender(from: &str) -> (String, String) {
    match Address::parse(from) {
        Some(a) => (sender_label(a.name.as_deref(), &a.email), a.email),
        None if envio_mime::strip_bidi_controls(from).trim().is_empty() => {
            (text::read::no_sender(), String::new())
        }
        None => {
            let line = envio_mime::strip_bidi_controls(from).trim().to_owned();
            let key = stamp_key("", &line);
            (line, key)
        }
    }
}

pub(super) fn subject(subject: String) -> String {
    if subject.trim().is_empty() {
        text::read::no_subject()
    } else {
        subject
    }
}

fn row_of(message: Message, account: String, drafts: bool) -> MessageRow {
    // A Drafts folder shows who the message is for, not who sent it.
    let (from, from_address) = if drafts {
        recipients_of(&message.envelope.to)
    } else {
        sender(&message.envelope.from)
    };
    MessageRow {
        id: message.id,
        folder: message.folder,
        account,
        cursor: Db::cursor_of(&message),
        has_attachments: message.envelope.has_attachments,
        from,
        from_address,
        subject: subject(message.envelope.subject),
        preview: message.preview,
        date: (message.envelope.date != 0).then_some(message.envelope.date),
        flags: message.flags,
        local_draft: None,
        not_uploaded_text: String::new(),
    }
}

/// The rows for `messages`.
pub(super) fn rows(db: &Db, messages: Vec<Message>) -> Vec<MessageRow> {
    let mut folders: HashMap<FolderId, (String, bool)> = HashMap::new();
    messages
        .into_iter()
        .map(|message| {
            let (account, drafts) = folders
                .entry(message.folder)
                .or_insert_with(|| {
                    db.folder(message.folder).map_or_else(
                        |_| (String::new(), false),
                        |f| (f.account, f.role == FolderRole::Drafts),
                    )
                })
                .clone();
            row_of(message, account, drafts)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use envio_store::{BodyState, Envelope, Flags, MessageRowId, NewMessage, StorePaths};

    use super::*;
    use crate::api::command::SearchScope;
    use crate::api::event::Event;
    use crate::worker::testing::{Sink, open, seed};

    fn message(from: &str, subject: &str) -> Message {
        Message {
            id: MessageRowId(1),
            folder: FolderId(1),
            uid: 1,
            size: 0,
            envelope: Envelope {
                from: from.into(),
                subject: subject.into(),
                ..Envelope::default()
            },
            flags: Flags::default(),
            preview: String::new(),
            body: BodyState::Missing,
        }
    }

    fn row(from: &str, subject: &str) -> MessageRow {
        row_of(message(from, subject), "a".into(), false)
    }

    #[test]
    fn row_shows_the_display_name() {
        let row = row("Ada Lovelace <ada@example.org>", "hi");
        assert_eq!(row.from, "Ada Lovelace");
        assert_eq!(row.from_address, "ada@example.org");
    }

    #[test]
    fn row_without_a_name_shows_the_address() {
        let row = row("<ada@example.org>", "hi");
        assert_eq!(row.from, "ada@example.org");
        assert_eq!(row.from_address, "ada@example.org");
    }

    #[test]
    fn a_name_that_is_an_address_shows_the_real_address() {
        let row = row("\"billing@paypal.com\" <x@evil.example>", "hi");
        assert_eq!(row.from, "x@evil.example");
        assert_eq!(row.from_address, "x@evil.example");
    }

    #[test]
    fn a_fullwidth_at_sign_counts_as_an_address() {
        for name in [
            "billing\u{FF20}paypal.com",
            "billing\u{FE6B}paypal.com",
            "\u{202E}b@p.com",
        ] {
            let row = row(&format!("\"{name}\" <x@evil.example>"), "hi");
            assert_eq!(row.from, "x@evil.example", "{name}");
        }
    }

    #[test]
    fn a_brand_name_without_an_at_sign_still_shows() {
        assert_eq!(row("\"PayPal\" <x@evil.example>", "hi").from, "PayPal");
    }

    #[test]
    fn a_name_equal_to_the_address_shows_the_address() {
        let row = row("\"ada@example.org\" <ada@example.org>", "hi");
        assert_eq!(row.from, "ada@example.org");
    }

    #[test]
    fn an_unparseable_sender_shows_the_raw_line_and_a_stamp_key() {
        let row = row("  Not An Address \u{202E} ", "hi");
        assert_eq!(row.from, "Not An Address");
        assert_eq!(row.from_address, "not an address");
        assert_eq!(stamp_key("", "Ann <A@X.org>"), "a@x.org");
        assert_eq!(stamp_key("", "a <b> <C@x.org> "), "c@x.org");
        assert_eq!(stamp_key("", "x > y <z"), "x > y <z");
        assert_eq!(stamp_key("a@b.c", "ignored"), "a@b.c");
    }

    #[test]
    fn stamp_key_of_the_row_matches_the_reader() {
        use crate::worker::read::stored_header;
        for line in [
            "Ada Lovelace <ada@example.org>",
            "ada@example.org",
            "\"billing@paypal.com\" <x@evil.example>",
            "Not An Address",
            "",
        ] {
            let row = row(line, "hi");
            let stored = Envelope {
                from: line.into(),
                ..Envelope::default()
            };
            let header = stored_header(&stored, "a");
            assert_eq!(
                row.from_address,
                stamp_key(&header.from_address, &header.from),
                "{line}"
            );
            let row_initials = if row.from_address.is_empty() {
                ""
            } else {
                &row.from
            };
            assert_eq!(row_initials, header.from, "{line}");
        }
    }

    #[test]
    fn row_without_a_sender_shows_the_placeholder() {
        let row = row("", "hi");
        assert_eq!(row.from, "(no sender)");
        assert_eq!(row.from_address, "");
    }

    #[test]
    fn row_without_a_subject_shows_the_placeholder() {
        assert_eq!(row("a@b.c", "").subject, "(no subject)");
        assert_eq!(row("a@b.c", "  \t").subject, "(no subject)");
        assert_eq!(row("a@b.c", "Re: x").subject, "Re: x");
    }

    #[test]
    fn search_rows_use_the_same_names() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let paths = StorePaths::new(&config.cache_dir, &config.data_dir);
        let db = Db::open(&paths.db()).unwrap();
        let new = NewMessage {
            uid: 2,
            size: 10,
            envelope: Envelope {
                subject: "findme".into(),
                from: "Ada <ada@example.org>".into(),
                date: 5,
                ..Envelope::default()
            },
            flags: Flags::default(),
            preview: String::new(),
        };
        db.upsert_message(seeded.folder, &new).unwrap();
        drop(db);
        let sink = Sink::default();
        let worker = open(&config, &sink);
        worker.search(SearchScope::All, "findme".into());
        let rows = sink
            .events()
            .into_iter()
            .find_map(|e| match e {
                Event::SearchResults { rows, .. } => Some(rows),
                _ => None,
            })
            .expect("search results");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].from, "Ada");
        assert_eq!(rows[0].from_address, "ada@example.org");
    }
}
