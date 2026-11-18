//! One SMTP transaction (MAIL, RCPT, DATA, the message), and whether a
//! failure may still have delivered the message. lettre's own `send` does
//! the same steps but can't tell a cut before the end of the message
//! (nothing sent: safe to retry) from one after it (the server may have
//! taken it: retrying could send it twice).

use std::borrow::Cow;
use std::cell::Cell;

use lettre::address::Envelope;
use lettre::transport::smtp::commands::{Data, Mail, Rcpt};
use lettre::transport::smtp::extension::{Extension, MailBodyParameter, MailParameter};
use lettre::transport::smtp::response::{Response, Severity};

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
        let eml = crlf(eml);
        let eml: &[u8] = &eml;
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
            Ok(Err(Fault::Smtp(error))) => Err(SendError {
                unconfirmed: all_out.get() && error.status().is_none(),
                error: map_error(error, false),
            }),
            Ok(Err(Fault::Reply(what))) => Err(SendError {
                unconfirmed: all_out.get(),
                error: NetError::Protocol(what),
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
    ) -> Result<(), Fault> {
        self.conn
            .command(Mail::new(envelope.from().cloned(), options))
            .await?;
        for to in envelope.to() {
            self.conn.command(Rcpt::new(to.clone(), vec![])).await?;
        }
        let go = self.conn.command(Data).await?;
        expect(&go, Severity::PositiveIntermediate, "DATA")?;
        let done = self.conn.message_iter(chunks).await?;
        expect(&done, Severity::PositiveCompletion, "the message")?;
        Ok(())
    }
}

/// Why a transaction failed.
enum Fault {
    Smtp(lettre::transport::smtp::Error),
    /// A positive reply of the wrong class. lettre's own `send` checks
    /// these, but its low-level `command` does not. Going on would have the
    /// server read the message as commands, or a refused message counted as
    /// sent.
    Reply(String),
}

impl From<lettre::transport::smtp::Error> for Fault {
    fn from(error: lettre::transport::smtp::Error) -> Self {
        Self::Smtp(error)
    }
}

fn expect(reply: &Response, want: Severity, to: &str) -> Result<(), Fault> {
    if reply.code().severity == want {
        Ok(())
    } else {
        Err(Fault::Reply(format!(
            "unexpected reply code {} to {to}",
            reply.code()
        )))
    }
}

/// Every bare LF and bare CR becomes CRLF. lettre dot-stuffs only after
/// CRLF, so a bare `\n.\r\n` in the body would pass unstuffed and a server
/// that takes bare LF could read it as the end of the data (smuggling).
/// Borrowed when the message is already clean.
fn crlf(eml: &[u8]) -> Cow<'_, [u8]> {
    let bare = |i: usize| match eml[i] {
        b'\n' => i == 0 || eml[i - 1] != b'\r',
        b'\r' => eml.get(i + 1) != Some(&b'\n'),
        _ => false,
    };
    if !(0..eml.len()).any(bare) {
        return Cow::Borrowed(eml);
    }
    let mut out = Vec::with_capacity(eml.len() + 16);
    for (i, &b) in eml.iter().enumerate() {
        out.push(b);
        if bare(i) {
            if b == b'\r' {
                out.push(b'\n');
            } else {
                // A bare LF: put the CR in front of it.
                out.pop();
                out.extend_from_slice(b"\r\n");
            }
        }
    }
    Cow::Owned(out)
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
    fn clean_input_is_borrowed() {
        assert!(matches!(crlf(b"a\r\nb\r\n"), Cow::Borrowed(_)));
        assert!(matches!(crlf(b""), Cow::Borrowed(_)));
    }

    #[test]
    fn bare_line_ends_become_crlf() {
        assert_eq!(&*crlf(b"a\nb"), b"a\r\nb");
        assert_eq!(&*crlf(b"a\rb"), b"a\r\nb");
        assert_eq!(&*crlf(b"a\r\nb\nc\rd\r\n"), b"a\r\nb\r\nc\r\nd\r\n");
        assert_eq!(&*crlf(b"\na\n"), b"\r\na\r\n");
        assert_eq!(&*crlf(b"\r"), b"\r\n");
        assert_eq!(&*crlf(b"\n\n"), b"\r\n\r\n");
        assert_eq!(&*crlf(b"a\n.\r\n"), b"a\r\n.\r\n");
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
