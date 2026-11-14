//! What the Outbox does next with an item, and what a send result means.
//! An item leaves the Outbox only once the server accepted it (and it is
//! filed in Sent) or once its Message-ID is found in Sent. Envio never
//! resends on its own. Pure.

use envio_net::smtp::SendError;
use envio_store::{OutboxMeta, SendState};

use crate::error::{ErrorKind, net_kind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemStep {
    /// Queued: send over SMTP.
    Send,
    /// Accepted: APPEND to Sent (unless the server files it itself), then
    /// remove.
    File,
    /// May have been sent: look for it in Sent. Found: remove. Not found:
    /// it waits for the user (Send again, or Move to Sent).
    Resolve,
    /// Failed: waits for the user (Edit, Retry).
    Wait,
}

pub fn item_step(state: &SendState) -> ItemStep {
    match state {
        SendState::Queued => ItemStep::Send,
        SendState::Accepted => ItemStep::File,
        // `Outbox::open` turns a left-over Sending into MayHaveBeenSent;
        // seen here it means the same.
        SendState::Sending | SendState::MayHaveBeenSent => ItemStep::Resolve,
        SendState::Failed { .. } => ItemStep::Wait,
    }
}

/// Whether the item waits out its send-after time (an invite answer that can
/// still be undone). A held item is not due: callers skip it for `Send`.
pub fn held(meta: &OutboxMeta, now_ms: i64) -> bool {
    meta.send_after_ms > now_ms
}

/// The earliest send-after time past `after_ms` among Queued items: when the
/// loop must wake to send what was held.
pub fn next_release<'a>(
    metas: impl IntoIterator<Item = &'a OutboxMeta>,
    after_ms: i64,
) -> Option<i64> {
    metas
        .into_iter()
        .filter(|meta| meta.state == SendState::Queued && meta.send_after_ms > after_ms.max(0))
        .map(|meta| meta.send_after_ms)
        .min()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AfterSend {
    pub state: SendState,
    /// Retry after a backoff (temporary failures only).
    pub retry: bool,
    /// Sign-in or certificate trouble: pause the account.
    pub stop: Option<ErrorKind>,
}

pub fn after_send(result: &Result<(), SendError>) -> AfterSend {
    let error = match result {
        Ok(()) => return settled(SendState::Accepted),
        Err(error) if error.unconfirmed => return settled(SendState::MayHaveBeenSent),
        Err(error) => &error.error,
    };
    match net_kind(error) {
        ErrorKind::Refused => settled(SendState::Failed {
            reason: refusal(error),
        }),
        ErrorKind::Temporary | ErrorKind::Protocol | ErrorKind::Local => AfterSend {
            state: SendState::Queued,
            retry: true,
            stop: None,
        },
        kind @ (ErrorKind::SignIn | ErrorKind::Certificate) => AfterSend {
            state: SendState::Queued,
            retry: false,
            stop: Some(kind),
        },
    }
}

fn settled(state: SendState) -> AfterSend {
    AfterSend {
        state,
        retry: false,
        stop: None,
    }
}

/// The server's own words, shown on the item.
fn refusal(error: &envio_net::NetError) -> String {
    match error {
        envio_net::NetError::Refused(text) => text.clone(),
        other => other.to_string(),
    }
}

/// Gmail files sent mail in Sent itself; an APPEND would make a copy.
pub fn files_itself(smtp_host: &str) -> bool {
    let host = smtp_host.trim_end_matches('.').to_ascii_lowercase();
    ["gmail.com", "googlemail.com"]
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

#[cfg(test)]
mod tests {
    use envio_net::{Fingerprint, NetError, Untrusted};

    use super::*;

    fn failed(error: NetError, unconfirmed: bool) -> Result<(), SendError> {
        Err(SendError { error, unconfirmed })
    }

    #[test]
    fn each_state_has_its_next_step() {
        assert_eq!(item_step(&SendState::Queued), ItemStep::Send);
        assert_eq!(item_step(&SendState::Accepted), ItemStep::File);
        assert_eq!(item_step(&SendState::MayHaveBeenSent), ItemStep::Resolve);
        assert_eq!(item_step(&SendState::Sending), ItemStep::Resolve);
        let failed = SendState::Failed { reason: "x".into() };
        assert_eq!(item_step(&failed), ItemStep::Wait);
    }

    fn meta(send_after_ms: i64) -> OutboxMeta {
        OutboxMeta {
            account: "1".into(),
            message_id: "<a@example.org>".into(),
            queued_at_ms: 0,
            state: SendState::Queued,
            from: "ada@example.org".into(),
            recipients: vec!["bob@example.org".into()],
            send_after_ms,
        }
    }

    #[test]
    fn held_is_true_until_send_after() {
        assert!(!held(&meta(0), 0));
        assert!(!held(&meta(0), 5_000));
        assert!(held(&meta(5_000), 4_999));
        assert!(!held(&meta(5_000), 5_000));
    }

    #[test]
    fn the_next_release_is_the_earliest_queued_one_past_the_mark() {
        let mut sent = meta(1_000);
        sent.state = SendState::Accepted;
        let all = [meta(0), meta(9_000), meta(4_000), sent];
        assert_eq!(next_release(&all, i64::MIN), Some(4_000));
        assert_eq!(next_release(&all, 4_000), Some(9_000));
        assert_eq!(next_release(&all, 9_000), None);
    }

    fn fp(byte: u8) -> Fingerprint {
        Fingerprint::from_bytes([byte; 32])
    }

    fn certificate_errors() -> Vec<NetError> {
        vec![
            NetError::Untrusted {
                presented: fp(1),
                reason: Untrusted::UnknownIssuer,
            },
            NetError::PinChanged {
                pinned: fp(1),
                presented: fp(2),
            },
            NetError::Tls("handshake".into()),
            NetError::NoStartTls,
        ]
    }

    fn every_error() -> Vec<NetError> {
        let mut all = vec![
            NetError::Io(std::io::Error::other("reset")),
            NetError::Timeout,
            NetError::Busy("421 later".into()),
            NetError::Protocol("junk".into()),
            NetError::Refused("550 no".into()),
            NetError::SignIn("535".into()),
        ];
        all.extend(certificate_errors());
        all
    }

    #[test]
    fn an_unconfirmed_failure_is_never_retried() {
        for error in every_error() {
            let label = format!("{error:?}");
            let after = after_send(&failed(error, true));
            assert_eq!(after.state, SendState::MayHaveBeenSent, "{label}");
            assert!(!after.retry, "{label}");
            assert_eq!(after.stop, None, "{label}");
        }
    }

    #[test]
    fn sign_in_and_certificate_failures_stop_without_retrying() {
        let mut errors = certificate_errors();
        errors.push(NetError::SignIn("535".into()));
        for error in errors {
            let label = format!("{error:?}");
            let after = after_send(&failed(error, false));
            assert!(!after.retry, "{label}");
            assert!(after.stop.is_some(), "{label}");
            assert_eq!(after.state, SendState::Queued, "{label}");
        }
    }

    #[test]
    fn send_results_map_to_states() {
        assert_eq!(after_send(&Ok(())).state, SendState::Accepted);
        let refused = after_send(&failed(NetError::Refused("550 no such user".into()), false));
        assert_eq!(
            refused.state,
            SendState::Failed {
                reason: "550 no such user".into()
            }
        );
        let busy = after_send(&failed(NetError::Busy("421 later".into()), false));
        assert_eq!((busy.state, busy.retry), (SendState::Queued, true));
        let sign_in = after_send(&failed(NetError::SignIn("535".into()), false));
        assert_eq!(
            (sign_in.state, sign_in.retry, sign_in.stop),
            (SendState::Queued, false, Some(ErrorKind::SignIn))
        );
    }

    #[test]
    fn only_gmail_files_sent_mail_itself() {
        assert!(files_itself("smtp.gmail.com"));
        assert!(files_itself("SMTP.GoogleMail.com."));
        assert!(!files_itself("mail.notgmail.com"));
        assert!(!files_itself("smtp.example.org"));
    }
}
