//! A saved draft as a message for the Drafts folder. Unlike Send it takes
//! what it can: an address that doesn't parse or a file that can't be read
//! is left out of the server copy (the local draft keeps it). Bcc is never
//! written into a message, so the copy doesn't have it either.

use envio_mime::{Address, Outgoing, OutgoingAttachment, build, content_type_for};
use envio_store::Draft;

/// `message_id` is in angle brackets, as the draft file keeps it.
pub fn draft_eml(draft: &Draft, from: Address, message_id: &str, now_ms: i64) -> Vec<u8> {
    let parse = |list: &[String]| -> Vec<Address> {
        list.iter()
            .filter_map(|text| Address::parse(text))
            .collect()
    };
    let attachments = draft
        .attachments
        .iter()
        .filter_map(|path| {
            let filename = path.file_name()?.to_string_lossy().into_owned();
            let bytes = std::fs::read(path).ok()?;
            Some(OutgoingAttachment {
                content_type: content_type_for(&filename).into(),
                filename,
                bytes,
            })
        })
        .collect();
    let outgoing = Outgoing {
        from,
        to: parse(&draft.to),
        cc: parse(&draft.cc),
        bcc: Vec::new(),
        subject: draft.subject.clone(),
        text: draft.body.clone(),
        in_reply_to: draft.in_reply_to.clone(),
        references: draft.references.clone(),
        attachments,
        calendar: None,
    };
    let bare = message_id
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>');
    build(&outgoing, now_ms / 1000, bare).bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_draft_with_a_bad_address_and_a_missing_file_still_builds() {
        let draft = Draft {
            to: vec!["not an address".into(), "bob@example.org".into()],
            subject: "Hello".into(),
            body: "text".into(),
            attachments: vec!["/nonexistent/file.pdf".into()],
            ..Draft::default()
        };
        let from = Address::new(Some("Carol"), "carol@example.org");
        let eml = String::from_utf8(draft_eml(
            &draft,
            from,
            "<id1@example.org>",
            1_800_000_000_000,
        ))
        .unwrap();
        assert!(eml.contains("Message-ID: <id1@example.org>"), "{eml}");
        assert!(eml.contains("bob@example.org"));
        assert!(!eml.contains("not an address"));
        assert!(!eml.contains("file.pdf"));
    }
}
