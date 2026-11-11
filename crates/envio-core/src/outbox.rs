//! The Outbox without I/O: what the user may decide about a queued message,
//! how a state reads, and how a refused message becomes a draft again.

use envio_mime::{Address, Message, same_email};
use envio_store::SendState;

use crate::api::command::{DraftInput, OutboxDecision};
use crate::api::view::OutboxStatus;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decided {
    SetState(SendState),
    Remove,
}

/// The decision is not possible in the item's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotNow;

/// The only place a user decision changes an item. `asked`: Sent was
/// searched this session and the message was not there. Nothing is resent
/// without the user: `Sending` and `Accepted` never change, and a
/// `MayHaveBeenSent` item waits for that search.
pub fn decide(state: &SendState, decision: OutboxDecision, asked: bool) -> Result<Decided, NotNow> {
    use OutboxDecision as D;
    use SendState as S;
    match (state, decision) {
        (S::MayHaveBeenSent, D::SendAgain) if asked => Ok(Decided::SetState(S::Queued)),
        (S::MayHaveBeenSent, D::MoveToSent) if asked => Ok(Decided::SetState(S::Accepted)),
        (S::Failed { .. }, D::Retry) => Ok(Decided::SetState(S::Queued)),
        (S::Queued | S::Failed { .. }, D::Delete) => Ok(Decided::Remove),
        (S::MayHaveBeenSent, D::Delete) if asked => Ok(Decided::Remove),
        _ => Err(NotNow),
    }
}

pub fn status(state: &SendState, asked: bool) -> OutboxStatus {
    match state {
        SendState::Queued => OutboxStatus::Queued,
        SendState::Sending => OutboxStatus::Sending,
        SendState::MayHaveBeenSent => OutboxStatus::MayHaveBeenSent { asked },
        SendState::Accepted => OutboxStatus::SentNotFiled,
        SendState::Failed { reason } => OutboxStatus::Failed {
            reason: reason.clone(),
        },
    }
}

fn shown(list: &[Address]) -> Vec<String> {
    list.iter().map(Address::display).collect()
}

/// The message carries attachments. The draft store keeps only paths of
/// files, so Edit refuses such a message instead of losing them.
pub fn has_attachments(eml: &[u8]) -> bool {
    matches!(
        envio_mime::parse(eml),
        Message::Parsed { attachments, .. } if !attachments.is_empty()
    )
}

/// The draft a queued message was made from. Bcc is not in the `.eml`, so it
/// is every recipient of the envelope not shown in To or Cc. `None` for
/// mail that can not be read as a message.
pub fn draft_from_eml(account: &str, eml: &[u8], recipients: &[String]) -> Option<DraftInput> {
    let Message::Parsed { envelope, .. } = envio_mime::parse(eml) else {
        return None;
    };
    let seen = |email: &str| {
        envelope
            .to
            .iter()
            .chain(&envelope.cc)
            .any(|a| same_email(&a.email, email))
    };
    Some(DraftInput {
        account: account.to_owned(),
        draft_id: None,
        to: shown(&envelope.to),
        cc: shown(&envelope.cc),
        bcc: recipients.iter().filter(|r| !seen(r)).cloned().collect(),
        subject: envelope.subject.clone(),
        body: envio_mime::quote_text(eml),
        in_reply_to: envelope.in_reply_to.clone(),
        references: envelope.references.clone(),
        attachments: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use OutboxDecision as D;
    use SendState as S;

    use super::*;

    fn failed() -> SendState {
        S::Failed { reason: "x".into() }
    }

    #[test]
    fn send_again_only_after_asking() {
        assert_eq!(
            decide(&S::MayHaveBeenSent, D::SendAgain, true),
            Ok(Decided::SetState(S::Queued))
        );
        assert_eq!(
            decide(&S::MayHaveBeenSent, D::SendAgain, false),
            Err(NotNow)
        );
        assert_eq!(decide(&S::Queued, D::SendAgain, true), Err(NotNow));
        assert_eq!(decide(&failed(), D::SendAgain, true), Err(NotNow));
    }

    #[test]
    fn move_to_sent_never_sends() {
        assert_eq!(
            decide(&S::MayHaveBeenSent, D::MoveToSent, true),
            Ok(Decided::SetState(S::Accepted))
        );
        assert_eq!(
            decide(&S::MayHaveBeenSent, D::MoveToSent, false),
            Err(NotNow)
        );
        assert_eq!(decide(&failed(), D::MoveToSent, true), Err(NotNow));
    }

    #[test]
    fn accepted_and_sending_are_never_changed() {
        for state in [S::Accepted, S::Sending] {
            for decision in [D::SendAgain, D::MoveToSent, D::Retry, D::Delete] {
                for asked in [false, true] {
                    assert_eq!(decide(&state, decision, asked), Err(NotNow));
                }
            }
        }
    }

    #[test]
    fn retry_only_from_failed() {
        assert_eq!(
            decide(&failed(), D::Retry, false),
            Ok(Decided::SetState(S::Queued))
        );
        for state in [S::Queued, S::MayHaveBeenSent, S::Accepted, S::Sending] {
            assert_eq!(decide(&state, D::Retry, true), Err(NotNow));
        }
    }

    #[test]
    fn delete_of_an_unasked_maybe_sent_is_refused() {
        assert_eq!(decide(&S::MayHaveBeenSent, D::Delete, false), Err(NotNow));
        assert_eq!(
            decide(&S::MayHaveBeenSent, D::Delete, true),
            Ok(Decided::Remove)
        );
        assert_eq!(decide(&S::Queued, D::Delete, false), Ok(Decided::Remove));
        assert_eq!(decide(&failed(), D::Delete, false), Ok(Decided::Remove));
    }

    #[test]
    fn accepted_reads_as_sent_not_filed() {
        assert_eq!(status(&S::Accepted, false), OutboxStatus::SentNotFiled);
        assert_eq!(
            status(&S::MayHaveBeenSent, true),
            OutboxStatus::MayHaveBeenSent { asked: true }
        );
    }

    /// Every `.rs` file under `dir`, with its text.
    fn sources(dir: &std::path::Path, out: &mut Vec<(std::path::PathBuf, String)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                sources(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                out.push((path, text));
            }
        }
    }

    #[test]
    fn only_worker_outbox_sets_state() {
        // Spelled in two pieces so this file does not contain the call.
        let call = format!("{}{}", "set_state", "(");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        sources(&root, &mut files);
        let mut calls = Vec::new();
        for (path, text) in &files {
            let n = text.matches(&call).count();
            if n > 0 {
                calls.push((path.strip_prefix(&root).unwrap().to_owned(), n));
            }
        }
        assert_eq!(
            calls,
            vec![(std::path::PathBuf::from("worker/outbox.rs"), 1)],
            "set_state is called only in worker/outbox.rs, once"
        );
    }

    #[test]
    fn draft_from_eml_puts_hidden_recipients_in_bcc() {
        let eml = b"From: me@example.org\r\nTo: Ada <ada@example.org>\r\n\
Cc: bob@example.org\r\nSubject: Plans\r\nMessage-ID: <m@example.org>\r\n\
In-Reply-To: <p@example.org>\r\nReferences: <o@example.org> <p@example.org>\r\n\
MIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nSee you\r\n";
        let recipients = vec![
            "ada@example.org".to_owned(),
            "BOB@example.org".to_owned(),
            "hidden@example.org".to_owned(),
        ];
        let draft = draft_from_eml("acct", eml, &recipients).unwrap();
        assert_eq!(draft.account, "acct");
        assert_eq!(draft.to, vec!["Ada <ada@example.org>"]);
        assert_eq!(draft.cc, vec!["bob@example.org"]);
        assert_eq!(draft.bcc, vec!["hidden@example.org"]);
        assert_eq!(draft.subject, "Plans");
        assert!(draft.body.contains("See you"));
        assert_eq!(draft.in_reply_to.as_deref(), Some("p@example.org"));
        assert_eq!(draft.references, vec!["o@example.org", "p@example.org"]);
    }

    #[test]
    fn attachments_are_detected() {
        let with = b"From: a@example.org\r\nSubject: s\r\nMIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=b\r\n\r\n--b\r\n\
Content-Type: text/plain\r\n\r\nhi\r\n--b\r\n\
Content-Type: application/pdf; name=a.pdf\r\n\
Content-Disposition: attachment; filename=a.pdf\r\n\
Content-Transfer-Encoding: base64\r\n\r\nAAAA\r\n--b--\r\n";
        assert!(has_attachments(with));
        assert!(!has_attachments(
            b"From: a@example.org\r\nSubject: s\r\n\r\nhi\r\n"
        ));
    }

    #[test]
    fn draft_from_eml_refuses_a_raw_message() {
        assert_eq!(draft_from_eml("a", b"\x00\x01\x02 not mail", &[]), None);
    }
}
