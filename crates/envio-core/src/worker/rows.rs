//! List rows from cached messages: what a list shows for each.

use std::collections::HashMap;

use envio_mime::Address;
use envio_store::{Db, FolderId, Message};

use crate::api::view::MessageRow;
use crate::text;

/// The sender as a list shows it: (name else address else placeholder,
/// bare address).
fn sender(from: &str) -> (String, String) {
    match Address::parse(from) {
        Some(a) => (a.name.unwrap_or_else(|| a.email.clone()), a.email),
        None => (text::read::no_sender(), String::new()),
    }
}

fn subject(subject: String) -> String {
    if subject.trim().is_empty() {
        text::read::no_subject()
    } else {
        subject
    }
}

fn row_of(message: Message, account: String) -> MessageRow {
    let (from, from_address) = sender(&message.envelope.from);
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
    }
}

/// The rows for `messages`.
pub(super) fn rows(db: &Db, messages: Vec<Message>) -> Vec<MessageRow> {
    let mut accounts: HashMap<FolderId, String> = HashMap::new();
    messages
        .into_iter()
        .map(|message| {
            let account = accounts
                .entry(message.folder)
                .or_insert_with(|| {
                    db.folder(message.folder)
                        .map(|f| f.account)
                        .unwrap_or_default()
                })
                .clone();
            row_of(message, account)
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
        row_of(message(from, subject), "a".into())
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
