//! One SMTP transaction (MAIL, RCPT, DATA, the message), and whether a
//! failure may still have delivered the message. lettre's own `send` does
//! the same steps but can't tell a cut before the end of the message
//! (nothing sent: safe to retry) from one after it (the server may have
//! taken it: retrying could send it twice).

use std::cell::Cell;

use lettre::address::Envelope;
use lettre::transport::smtp::commands::{Data, Mail, Rcpt};
use lettre::transport::smtp::extension::{Extension, MailBodyParameter, MailParameter};

use super::{SmtpSession, envelope, map_error};
use crate::connect;
use crate::error::NetError;

/// How much of the message goes out per write.
const CHUNK: usize = 64 * 1024;

/// A failed send. `unconfirmed` is true when the whole message went out
/// but no reply to it came back (cut or timeout after the final dot): the
/// server may have delivered it, so the Outbox must not resend it on its
/// own. A reply with a code (4xx, 5xx) is always confirmed: not sent.
#[derive(Debug)]
pub struct SendError {
    pub error: NetError,
    pub unconfirmed: bool,
}

impl SendError {
    fn confirmed(error: NetError) -> Self {
        Self {
            error,
            unconfirmed: false,
        }
    }
}

impl From<SendError> for NetError {
    fn from(error: SendError) -> Self {
        error.error
    }
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.unconfirmed {
            write!(f, "{} (the message may have been sent)", self.error)
        } else {
            self.error.fmt(f)
        }
    }
}

impl std::error::Error for SendError {}

impl SmtpSession {
    /// Sends one message. `eml` is the finished RFC 5322 message (from
    /// envio-mime); `from` and `to` are the envelope, which may differ
    /// from its headers (Bcc). Any error leaves the session unusable.
    pub async fn send(&mut self, from: &str, to: &[String], eml: &[u8]) -> Result<(), SendError> {
        let envelope = envelope(from, to).map_err(SendError::confirmed)?;
        let non_ascii = !from.is_ascii() || to.iter().any(|t| !t.is_ascii());
        let options = self.mail_options(non_ascii, eml)?;
        let all_out = Cell::new(false);
        // The upload alone can outlast a command, so the limit grows with
        // the message.
        let outcome = connect::within_for(connect::transfer_timeout(eml.len()), async {
            let chunks = Chunks {
                rest: eml,
                all_out: &all_out,
            };
            Ok(self.transaction(&envelope, options, chunks).await)
        })
        .await;
        match outcome {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(SendError {
                unconfirmed: all_out.get() && error.status().is_none(),
                error: map_error(error, false),
            }),
            Err(timeout) => Err(SendError {
                error: timeout,
                unconfirmed: all_out.get(),
            }),
        }
    }

    /// SMTPUTF8 and 8BITMIME, as lettre's `send` sets them.
    fn mail_options(&self, non_ascii: bool, eml: &[u8]) -> Result<Vec<MailParameter>, SendError> {
        let info = self.conn.server_info();
        let mut options = Vec::new();
        if non_ascii {
            if !info.supports_feature(Extension::SmtpUtfEight) {
                return Err(SendError::confirmed(NetError::Refused(
                    "the server can't take non-ASCII addresses (no SMTPUTF8)".into(),
                )));
            }
            options.push(MailParameter::SmtpUtfEight);
        }
        if !eml.is_ascii() {
            if !info.supports_feature(Extension::EightBitMime) {
                return Err(SendError::confirmed(NetError::Refused(
                    "the server can't take 8-bit mail (no 8BITMIME)".into(),
                )));
            }
            options.push(MailParameter::Body(MailBodyParameter::EightBitMime));
        }
        Ok(options)
    }

    async fn transaction(
        &mut self,
        envelope: &Envelope,
        options: Vec<MailParameter>,
        chunks: Chunks<'_>,
    ) -> Result<(), lettre::transport::smtp::Error> {
        self.conn
            .command(Mail::new(envelope.from().cloned(), options))
            .await?;
        for to in envelope.to() {
            self.conn.command(Rcpt::new(to.clone(), vec![])).await?;
        }
        self.conn.command(Data).await?;
        self.conn.message_iter(chunks).await?;
        Ok(())
    }
}

/// The message in [`CHUNK`]s. lettre asks for the next chunk only after
/// writing the previous one, so asking past the end means every byte was
/// written: from then on, only the final dot and the reply are missing.
struct Chunks<'a> {
    rest: &'a [u8],
    all_out: &'a Cell<bool>,
}

impl<'a> Iterator for Chunks<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<&'a [u8]> {
        if self.rest.is_empty() {
            self.all_out.set(true);
            return None;
        }
        let (chunk, rest) = self.rest.split_at(self.rest.len().min(CHUNK));
        self.rest = rest;
        Some(chunk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_mark_the_end_only_once_all_were_taken() {
        let eml = vec![b'a'; CHUNK * 2 + 10];
        let all_out = Cell::new(false);
        let mut chunks = Chunks {
            rest: &eml,
            all_out: &all_out,
        };
        let sizes: Vec<usize> = chunks.by_ref().take(3).map(<[u8]>::len).collect();
        assert_eq!(sizes, [CHUNK, CHUNK, 10]);
        assert!(!all_out.get());
        assert!(chunks.next().is_none());
        assert!(all_out.get());
    }

    #[test]
    fn a_send_error_turns_into_its_net_error() {
        let error = SendError {
            error: NetError::Timeout,
            unconfirmed: true,
        };
        assert!(error.to_string().contains("may have been sent"));
        assert!(matches!(NetError::from(error), NetError::Timeout));
    }
}
