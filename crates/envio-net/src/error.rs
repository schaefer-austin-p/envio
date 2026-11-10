//! One error type for the crate. Each variant maps to one of the spec's
//! failure kinds, which core picks (and words, with gettext): `Io`,
//! `Timeout` and `Busy` are temporary; `SignIn` pauses the account;
//! `Untrusted` asks the user; `PinChanged` stops sync; `Refused` is an
//! error on that item. The text here is for logs only and never holds a
//! password.

use std::io;

use crate::server::Fingerprint;

#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("network: {0}")]
    Io(#[from] io::Error),
    #[error("timed out")]
    Timeout,
    /// No pin, and the system roots don't vouch for the certificate. The
    /// wizard shows the fingerprint; if the user accepts, the caller
    /// stores it as the endpoint's pin and connects again.
    #[error("certificate not trusted ({reason:?})")]
    Untrusted {
        presented: Fingerprint,
        reason: Untrusted,
    },
    /// A pinned server showed another certificate. Never accepted here.
    #[error("certificate changed")]
    PinChanged {
        pinned: Fingerprint,
        presented: Fingerprint,
    },
    #[error("TLS: {0}")]
    Tls(String),
    /// STARTTLS was asked for and the server didn't offer or refused it.
    #[error("the server offers no STARTTLS")]
    NoStartTls,
    #[error("sign-in rejected: {0}")]
    SignIn(String),
    /// The server is up but said "not now" (SMTP 4xx, IMAP BYE).
    #[error("server busy: {0}")]
    Busy(String),
    /// The server refused this request for good (SMTP 5xx, IMAP NO).
    #[error("refused by the server: {0}")]
    Refused(String),
    #[error("protocol: {0}")]
    Protocol(String),
}

/// Why the system roots didn't vouch for a certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Untrusted {
    /// Self-signed, or signed by a CA the system doesn't know.
    UnknownIssuer,
    Expired,
    /// Valid, but for another host name.
    WrongName,
    Other,
}

impl NetError {
    /// Worth retrying with backoff.
    pub fn is_temporary(&self) -> bool {
        matches!(self, Self::Io(_) | Self::Timeout | Self::Busy(_))
    }
}

pub type Result<T> = std::result::Result<T, NetError>;
