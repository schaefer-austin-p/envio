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
    /// A plain copy: `server.imported_from` names the original and nothing
    /// else is set, so until `link` is made it is a new draft.
    pub draft: Draft,
    pub attachments: Vec<Carried>,
    /// The original's Message-ID when the draft may stand in for it: the
    /// store links it (after the files are in) and the first saved change
    /// then replaces the original. `None`: the original stays.
    pub link: Option<String>,
}

/// What the import needs to know about the message and its account.
pub(crate) struct Source<'a> {
    pub account: &'a str,
    /// The account's own address, what Envio writes as From.
    pub identity: &'a str,
    /// The cached row's Message-ID, with angle brackets.
    pub message_id: Option<&'a str>,
    /// The cached row's flags are \Seen and \Draft at most (it does not
    /// know keywords or labels; those are lost when the original goes).
    pub plain_flags: bool,
    /// The file name an attachment gets in the draft, from its name and
    /// index.
    pub safe_name: fn(&str, usize) -> String,
}

/// Headers Envio writes into a draft it uploads. Any other header (Reply-To,
/// X-Priority, Disposition-Notification-To, a client's own) would be lost
/// when the rebuilt draft replaces the original.
const KNOWN_HEADERS: [&str; 11] = [
    "from",
    "to",
    "cc",
    "subject",
    "message-id",
    "in-reply-to",
    "references",
    "date",
    "mime-version",
    "content-type",
    "content-transfer-encoding",
];

/// `None` when the bytes are not mail the reader can parse.
///
/// Linked only when the rebuilt draft loses nothing the original has (an
/// allowlist, see `loses_nothing`); everything else is a detached copy,
/// with Bcc carried into the draft.
pub(crate) fn draft_from_raw(raw: &[u8], src: &Source<'_>, now_ms: i64) -> Option<Imported> {
    let Message::Parsed {
        envelope,
        body,
        attachments,
        ..
    } = envio_mime::parse(raw)
    else {
        return None;
    };
    let layout = envio_mime::layout(raw)?;
    let usable = src.message_id.and_then(search_id);
    let carried: Vec<Carried> = attachments
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
    let link = usable.filter(|_| {
        !matches!(body, Body::Html(_))
            && src.plain_flags
            && loses_nothing((&envelope, &layout), (&attachments, carried.len()), src)
    });
    let shown = |list: &[envio_mime::Address]| -> Vec<String> {
        list.iter().map(envio_mime::Address::display).collect()
    };
    let draft = Draft {
        account: src.account.to_owned(),
        to: shown(&envelope.to),
        cc: shown(&envelope.cc),
        bcc: shown(&layout.bcc),
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
        server: ServerCopy {
            imported_from: usable.map(str::to_owned),
            ..ServerCopy::default()
        },
    };
    Some(Imported {
        draft,
        attachments: carried,
        link: link.map(str::to_owned),
    })
}

/// Whether Envio's own upload of the rebuilt draft has everything the
/// original has: only headers it writes, From as the account's own
/// address, one text/plain (then attachments it keeps by name and type),
/// and every attachment's bytes at hand.
fn loses_nothing(
    (envelope, layout): (&envio_mime::Envelope, &envio_mime::Layout),
    (attachments, carried): (&[envio_mime::AttachmentInfo], usize),
    src: &Source<'_>,
) -> bool {
    let own_from = matches!(&envelope.from[..], [from] if from.is(src.identity));
    let files = attachments.iter().filter(|a| !a.inline);
    let kept = files.clone().all(|a| {
        a.filename.as_deref().is_some_and(|name| {
            let name = (src.safe_name)(name, a.index);
            envio_mime::content_type_for(&name) == a.content_type
        })
    });
    layout.plain_shape
        && layout.bcc.is_empty()
        && layout
            .headers
            .iter()
            .all(|h| KNOWN_HEADERS.contains(&h.as_str()))
        && own_from
        && attachments.iter().all(|a| !a.inline)
        && kept
        && files.count() == carried
}

#[cfg(test)]
mod tests {
    use super::*;

    const MID: &str = "<d1@example.org>";

    fn safe(name: &str, _index: usize) -> String {
        name.to_owned()
    }

    fn source(message_id: Option<&str>) -> Source<'_> {
        Source {
            account: "acc",
            identity: "me@example.org",
            message_id,
            plain_flags: true,
            safe_name: safe,
        }
    }

    fn import(raw: &[u8]) -> Imported {
        draft_from_raw(raw, &source(Some(MID)), 9_000).unwrap()
    }

    fn plain(extra: &str) -> Vec<u8> {
        format!(
            "From: me@example.org\r\nTo: Ada <ada@example.org>, bob@example.org\r\n\
Cc: cy@example.org\r\nSubject: Plan\r\nMessage-ID: {MID}\r\n\
In-Reply-To: <p@example.org>\r\nReferences: <r@example.org> <p@example.org>\r\n{extra}\r\n\
the text\r\n"
        )
        .into_bytes()
    }

    fn plain_with(from: &str, to: &str) -> Vec<u8> {
        String::from_utf8(plain(""))
            .unwrap()
            .replace(from, to)
            .into_bytes()
    }

    const MIXED: &str = "From: me@example.org\r\nSubject: s\r\nMessage-ID: <d1@example.org>\r\n\
MIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=B\r\n\r\n\
--B\r\nContent-Type: text/plain\r\n\r\nt\r\n";
    const PDF: &str = "--B\r\nContent-Type: application/pdf; name=\"a.pdf\"\r\n\
Content-Disposition: attachment; filename=\"a.pdf\"\r\n\
Content-Transfer-Encoding: base64\r\n\r\nSGVsbG8=\r\n";

    fn mixed(parts: &str) -> Vec<u8> {
        format!("{MIXED}{parts}--B--\r\n").into_bytes()
    }

    fn assert_detached(got: &Imported, why: &str) {
        assert_eq!(got.link, None, "{why}");
        assert_eq!(got.draft.server.message_id, None, "{why}");
        assert_eq!(got.draft.server.uploaded_ms, 0, "{why}");
        assert_eq!(
            got.draft.server.imported_from.as_deref(),
            Some(MID),
            "{why}"
        );
    }

    #[test]
    fn a_plain_draft_with_an_attachment_is_linked() {
        let raw = format!(
            "From: me@example.org\r\nTo: Ada <ada@example.org>, bob@example.org\r\n\
Cc: cy@example.org\r\nSubject: Plan\r\nMessage-ID: <d1@example.org>\r\n\
In-Reply-To: <p@example.org>\r\nReferences: <r@example.org> <p@example.org>\r\n\
Date: Mon, 1 Jan 2026 10:00:00 +0000\r\n{}{PDF}--B--\r\n",
            &MIXED[MIXED.find("MIME-Version").unwrap()..]
        );
        let got = import(raw.as_bytes());
        assert_eq!(got.link.as_deref(), Some(MID));
        let d = &got.draft;
        assert_eq!(d.to, vec!["Ada <ada@example.org>", "bob@example.org"]);
        assert_eq!(d.cc, vec!["cy@example.org"]);
        assert_eq!(d.subject, "Plan");
        assert_eq!(d.body.trim_end(), "t");
        assert_eq!(d.in_reply_to.as_deref(), Some("<p@example.org>"));
        assert_eq!(d.references, vec!["<r@example.org>", "<p@example.org>"]);
        assert_eq!(d.account, "acc");
        assert_eq!(d.updated_at_ms, 9_000);
        // Until the store links it, the draft is a plain copy.
        assert_eq!(d.server.message_id, None);
        assert_eq!(d.server.imported_from.as_deref(), Some(MID));
        assert_eq!(got.attachments.len(), 1);
        assert_eq!(got.attachments[0].filename.as_deref(), Some("a.pdf"));
        assert_eq!(got.attachments[0].bytes, b"Hello");
    }

    #[test]
    fn a_calendar_attachment_with_a_name_is_kept_and_linked() {
        let ics = "--B\r\nContent-Type: text/calendar; name=\"inv.ics\"\r\n\
Content-Disposition: attachment; filename=\"inv.ics\"\r\n\r\nBEGIN:VCALENDAR\r\n";
        assert_eq!(import(&mixed(ics)).link.as_deref(), Some(MID));
    }

    #[test]
    fn an_html_only_draft_is_detached_with_a_text_body() {
        let raw = String::from_utf8(plain("Content-Type: text/html\r\n"))
            .unwrap()
            .replace("the text", "<p>the <b>text</b></p>");
        let got = import(raw.as_bytes());
        assert_detached(&got, "html");
        assert!(got.draft.body.contains("the text"), "{}", got.draft.body);
        assert!(!got.draft.body.contains('<'), "{}", got.draft.body);
    }

    #[test]
    fn an_alternative_with_html_is_detached() {
        let raw = b"From: me@example.org\r\nSubject: Alt\r\nMessage-ID: <d1@example.org>\r\n\
MIME-Version: 1.0\r\nContent-Type: multipart/alternative; boundary=B\r\n\r\n\
--B\r\nContent-Type: text/plain\r\n\r\nplain version\r\n\
--B\r\nContent-Type: text/html\r\n\r\n<p>html version</p>\r\n--B--\r\n";
        let got = import(raw);
        assert_detached(&got, "alternative");
        assert_eq!(got.draft.body.trim_end(), "plain version");
    }

    #[test]
    fn an_unusable_message_id_is_detached_and_remembers_nothing() {
        for id in [None, Some("<>"), Some("  "), Some("no-brackets@x")] {
            let got = draft_from_raw(&plain(""), &source(id), 5).unwrap();
            assert_eq!(got.link, None, "{id:?}");
            assert_eq!(got.draft.server, ServerCopy::default(), "{id:?}");
        }
    }

    #[test]
    fn a_bcc_is_carried_into_the_detached_draft() {
        let got = import(&plain("Bcc: Hid <hidden@example.org>, h2@example.org\r\n"));
        assert_detached(&got, "bcc");
        assert_eq!(
            got.draft.bcc,
            vec!["Hid <hidden@example.org>", "h2@example.org"]
        );
    }

    #[test]
    fn an_inline_part_is_never_replaced() {
        let inline = "--B\r\nContent-Type: image/png; name=\"i.png\"\r\nContent-ID: <i1>\r\n\
Content-Disposition: inline; filename=\"i.png\"\r\nContent-Transfer-Encoding: base64\r\n\r\nSGk=\r\n";
        let got = import(&mixed(inline));
        assert_detached(&got, "inline");
        assert!(got.attachments.is_empty());
    }

    #[test]
    fn a_header_envio_does_not_write_is_never_replaced() {
        for header in [
            "Reply-To: other@example.org",
            "X-Priority: 1",
            "Disposition-Notification-To: me@example.org",
            "X-Mailer: Other 1.0",
            "Importance: high",
        ] {
            assert_detached(&import(&plain(&format!("{header}\r\n"))), header);
        }
    }

    #[test]
    fn a_from_that_is_not_the_account_is_never_replaced() {
        let alias = plain_with("From: me@example.org", "From: alias@example.org");
        assert_detached(&import(&alias), "alias");
        let two = plain_with(
            "From: me@example.org",
            "From: me@example.org, other@example.org",
        );
        assert_detached(&import(&two), "two froms");
        let own = plain_with(
            "From: me@example.org",
            "From: \"Me Myself\" <ME@Example.org>",
        );
        assert_eq!(import(&own).link.as_deref(), Some(MID));
    }

    #[test]
    fn flags_beyond_seen_and_draft_are_never_replaced() {
        let src = Source {
            plain_flags: false,
            ..source(Some(MID))
        };
        let got = draft_from_raw(&plain(""), &src, 5).unwrap();
        assert_detached(&got, "flags");
    }

    #[test]
    fn attachments_envio_would_change_or_drop_are_never_replaced() {
        let rfc822 = "--B\r\nContent-Type: message/rfc822; name=\"fwd.eml\"\r\n\
Content-Disposition: attachment; filename=\"fwd.eml\"\r\n\r\nFrom: a@example.org\r\n\r\nx\r\n";
        assert_detached(&import(&mixed(rfc822)), "message/rfc822");
        let nameless = "--B\r\nContent-Type: text/calendar; method=REQUEST\r\n\
Content-Disposition: attachment\r\n\r\nBEGIN:VCALENDAR\r\n";
        assert_detached(&import(&mixed(nameless)), "nameless calendar");
    }

    #[test]
    fn a_second_text_part_is_never_replaced() {
        let second = "--B\r\nContent-Type: text/plain\r\n\r\nsecond text\r\n";
        assert_detached(&import(&mixed(second)), "two text parts");
    }

    #[test]
    fn a_linked_import_is_current_until_changed_then_replaces_the_original() {
        use envio_sync::plan::drafts::{needs_upload, replaced};
        let mut got = import(&plain(""));
        // What the store's link does once the files are in.
        got.draft.updated_at_ms = 5_000;
        got.draft.server.message_id = got.link.clone();
        got.draft.server.uploaded_ms = 5_000;
        assert!(!needs_upload(&got.draft), "unchanged: never uploaded");
        got.draft.updated_at_ms = 6_000;
        assert!(needs_upload(&got.draft));
        assert_eq!(replaced(&got.draft.server, "<new@x>"), Some(MID));
    }

    #[test]
    fn a_detached_import_uploads_new_and_replaces_nothing() {
        use envio_sync::plan::drafts::{needs_upload, replaced};
        let got = import(&plain("Content-Type: text/html\r\n"));
        assert!(needs_upload(&got.draft));
        assert_eq!(replaced(&got.draft.server, "<new@x>"), None);
    }
}
