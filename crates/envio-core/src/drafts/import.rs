//! A server draft as a local draft, for Edit when no local file matches
//! (#228, Decision 4). Pure.

use envio_mime::{Body, Message};
use envio_store::{Draft, ServerCopy};
use envio_sync::plan::message_id::search_id;

/// A non-inline attachment of the original, to copy into the draft.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Carried {
    pub index: usize,
    /// As the sender wrote it: sanitize before use.
    pub filename: Option<String>,
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
pub(crate) struct Imported {
    pub draft: Draft,
    pub attachments: Vec<Carried>,
    /// The draft stands in for the server copy: the first saved change
    /// replaces it. False: it is a new draft and the original stays.
    pub linked: bool,
}

/// `message_id` is the cached row's, with angle brackets. `None` when the
/// bytes are not mail the reader can parse.
///
/// Linked only when replacing the original loses nothing: a usable
/// Message-ID to replace it by, no HTML (Compose holds plain text), no
/// inline parts and no Bcc (neither fits in the draft).
pub(crate) fn draft_from_raw(
    raw: &[u8],
    account: &str,
    message_id: Option<&str>,
    now_ms: i64,
) -> Option<Imported> {
    let Message::Parsed {
        envelope,
        body,
        attachments,
        ..
    } = envio_mime::parse(raw)
    else {
        return None;
    };
    let usable = message_id.and_then(search_id);
    let linked = usable.is_some()
        && !matches!(body, Body::Html(_))
        && !attachments.iter().any(|a| a.inline)
        && !has_bcc(raw);
    let server = match (linked, usable) {
        (true, Some(id)) => ServerCopy {
            message_id: Some(id.to_owned()),
            uploaded_ms: now_ms,
            imported_from: Some(id.to_owned()),
            ..ServerCopy::default()
        },
        _ => ServerCopy {
            imported_from: usable.map(str::to_owned),
            ..ServerCopy::default()
        },
    };
    let shown = |list: &[envio_mime::Address]| -> Vec<String> {
        list.iter().map(envio_mime::Address::display).collect()
    };
    let draft = Draft {
        account: account.to_owned(),
        to: shown(&envelope.to),
        cc: shown(&envelope.cc),
        bcc: Vec::new(),
        subject: envelope.subject.clone(),
        body: envio_mime::quote_text(raw),
        in_reply_to: envelope
            .in_reply_to
            .as_deref()
            .map(envio_sync::convert::bracketed),
        references: envelope
            .references
            .iter()
            .map(|m| envio_sync::convert::bracketed(m))
            .collect(),
        attachments: Vec::new(),
        updated_at_ms: now_ms,
        server,
    };
    let carried = attachments
        .iter()
        .filter(|a| !a.inline)
        .filter_map(|a| {
            Some(Carried {
                index: a.index,
                filename: a.filename.clone(),
                bytes: envio_mime::attachment_bytes(raw, a.index)?,
            })
        })
        .collect();
    Some(Imported {
        draft,
        attachments: carried,
        linked,
    })
}

/// The header block has a Bcc line. The reader's envelope has no Bcc, so
/// a draft with one must not replace its original.
fn has_bcc(raw: &[u8]) -> bool {
    raw.split(|b| *b == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .take_while(|line| !line.is_empty())
        .any(|line| line.len() >= 4 && line[..4].eq_ignore_ascii_case(b"bcc:"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MID: &str = "<d1@example.org>";

    fn plain(extra: &str) -> Vec<u8> {
        format!(
            "From: me@example.org\r\nTo: Ada <ada@example.org>, bob@example.org\r\n\
Cc: cy@example.org\r\nSubject: Plan\r\nMessage-ID: {MID}\r\n\
In-Reply-To: <p@example.org>\r\nReferences: <r@example.org> <p@example.org>\r\n{extra}\r\n\
the text\r\n"
        )
        .into_bytes()
    }

    #[test]
    fn a_multipart_draft_with_an_attachment_is_linked() {
        let raw = b"From: me@example.org\r\nTo: Ada <ada@example.org>, bob@example.org\r\n\
Cc: cy@example.org\r\nSubject: Plan\r\nMessage-ID: <d1@example.org>\r\n\
In-Reply-To: <p@example.org>\r\nReferences: <r@example.org> <p@example.org>\r\n\
MIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=B\r\n\r\n\
--B\r\nContent-Type: text/plain\r\n\r\nthe text\r\n\
--B\r\nContent-Type: application/pdf; name=\"a.pdf\"\r\n\
Content-Disposition: attachment; filename=\"a.pdf\"\r\n\
Content-Transfer-Encoding: base64\r\n\r\nSGVsbG8=\r\n--B--\r\n";
        let got = draft_from_raw(raw, "acc", Some(MID), 9_000).unwrap();
        assert!(got.linked);
        let d = &got.draft;
        assert_eq!(d.to, vec!["Ada <ada@example.org>", "bob@example.org"]);
        assert_eq!(d.cc, vec!["cy@example.org"]);
        assert_eq!(d.subject, "Plan");
        assert_eq!(d.body.trim_end(), "the text");
        assert_eq!(d.in_reply_to.as_deref(), Some("<p@example.org>"));
        assert_eq!(d.references, vec!["<r@example.org>", "<p@example.org>"]);
        assert_eq!(d.account, "acc");
        assert_eq!(d.updated_at_ms, 9_000);
        assert_eq!(d.server.message_id.as_deref(), Some(MID));
        assert_eq!(d.server.uploaded_ms, 9_000);
        assert_eq!(d.server.imported_from.as_deref(), Some(MID));
        assert_eq!(got.attachments.len(), 1);
    }

    #[test]
    fn a_non_inline_attachment_is_carried_with_its_bytes() {
        let raw = b"From: me@example.org\r\nSubject: s\r\nMessage-ID: <d1@example.org>\r\n\
MIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=B\r\n\r\n\
--B\r\nContent-Type: text/plain\r\n\r\nt\r\n\
--B\r\nContent-Type: application/pdf; name=\"a.pdf\"\r\n\
Content-Disposition: attachment; filename=\"a.pdf\"\r\n\
Content-Transfer-Encoding: base64\r\n\r\nSGVsbG8=\r\n--B--\r\n";
        let got = draft_from_raw(raw, "acc", Some(MID), 1).unwrap();
        assert_eq!(got.attachments.len(), 1);
        assert_eq!(got.attachments[0].filename.as_deref(), Some("a.pdf"));
        assert_eq!(got.attachments[0].bytes, b"Hello");
    }

    #[test]
    fn an_html_only_draft_is_detached_with_a_text_body() {
        let raw =
            plain("Content-Type: text/html\r\n").replace_text("the text", "<p>the <b>text</b></p>");
        let got = draft_from_raw(&raw, "acc", Some(MID), 5).unwrap();
        assert!(!got.linked);
        assert!(got.draft.body.contains("the text"), "{}", got.draft.body);
        assert!(!got.draft.body.contains('<'), "{}", got.draft.body);
        assert_eq!(got.draft.server.message_id, None);
        assert_eq!(got.draft.server.uploaded_ms, 0);
        assert_eq!(got.draft.server.imported_from.as_deref(), Some(MID));
    }

    #[test]
    fn an_alternative_with_html_is_detached() {
        let raw = b"From: me@example.org\r\nSubject: Alt\r\nMessage-ID: <d1@example.org>\r\n\
MIME-Version: 1.0\r\nContent-Type: multipart/alternative; boundary=B\r\n\r\n\
--B\r\nContent-Type: text/plain\r\n\r\nplain version\r\n\
--B\r\nContent-Type: text/html\r\n\r\n<p>html version</p>\r\n--B--\r\n";
        let got = draft_from_raw(raw, "acc", Some(MID), 5).unwrap();
        assert!(!got.linked);
        assert_eq!(got.draft.body.trim_end(), "plain version");
        assert_eq!(got.draft.server.message_id, None);
        assert_eq!(got.draft.server.imported_from.as_deref(), Some(MID));
    }

    #[test]
    fn an_unusable_message_id_is_detached_and_remembers_nothing() {
        for id in [None, Some("<>"), Some("  "), Some("no-brackets@x")] {
            let got = draft_from_raw(&plain(""), "acc", id, 5).unwrap();
            assert!(!got.linked, "{id:?}");
            assert_eq!(got.draft.server, ServerCopy::default(), "{id:?}");
        }
    }

    #[test]
    fn a_bcc_header_or_an_inline_part_is_never_replaced() {
        let got =
            draft_from_raw(&plain("Bcc: hidden@example.org\r\n"), "acc", Some(MID), 5).unwrap();
        assert!(!got.linked);
        assert_eq!(got.draft.server.message_id, None);
        assert!(got.draft.bcc.is_empty());

        let raw = b"From: me@example.org\r\nSubject: In\r\nMessage-ID: <d1@example.org>\r\n\
MIME-Version: 1.0\r\nContent-Type: multipart/related; boundary=B\r\n\r\n\
--B\r\nContent-Type: text/plain\r\n\r\nbody\r\n\
--B\r\nContent-Type: image/png\r\nContent-ID: <i1>\r\n\
Content-Disposition: inline\r\nContent-Transfer-Encoding: base64\r\n\r\nSGk=\r\n--B--\r\n";
        let got = draft_from_raw(raw, "acc", Some(MID), 5).unwrap();
        assert!(!got.linked);
        assert!(got.attachments.is_empty());
    }

    #[test]
    fn a_linked_import_is_current_until_changed_then_replaces_the_original() {
        use envio_sync::plan::drafts::{needs_upload, replaced};
        let mut got = draft_from_raw(&plain(""), "acc", Some(MID), 5_000).unwrap();
        assert!(!needs_upload(&got.draft), "unchanged: never uploaded");
        got.draft.updated_at_ms = 6_000;
        assert!(needs_upload(&got.draft));
        assert_eq!(replaced(&got.draft.server, "<new@x>"), Some(MID));
    }

    #[test]
    fn a_detached_import_uploads_new_and_replaces_nothing() {
        use envio_sync::plan::drafts::{needs_upload, replaced};
        let raw = plain("Content-Type: text/html\r\n");
        let got = draft_from_raw(&raw, "acc", Some(MID), 5_000).unwrap();
        assert!(needs_upload(&got.draft));
        assert_eq!(replaced(&got.draft.server, "<new@x>"), None);
    }

    trait ReplaceText {
        fn replace_text(self, from: &str, to: &str) -> Vec<u8>;
    }

    impl ReplaceText for Vec<u8> {
        fn replace_text(self, from: &str, to: &str) -> Vec<u8> {
            String::from_utf8(self)
                .unwrap()
                .replace(from, to)
                .into_bytes()
        }
    }
}
