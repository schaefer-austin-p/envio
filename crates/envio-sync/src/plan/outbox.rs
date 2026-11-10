//! What the Outbox does next with an item, and what a send result means.
//! An item leaves the Outbox only once the server accepted it (and it is
//! filed in Sent) or once its Message-ID is found in Sent. Envio never
//! resends on its own. Pure.

use envio_net::smtp::SendError;
use envio_store::SendState;

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
    use envio_net::NetError;

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

    #[test]
    fn an_unconfirmed_failure_is_never_retried() {
        let after = after_send(&failed(NetError::Timeout, true));
        assert_eq!(after.state, SendState::MayHaveBeenSent);
        assert!(!after.retry);
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
