//! Sync's error: a network or a store failure, sorted into the spec's
//! failure kinds so core can pick the fixed response (retry, pause the
//! account, stop, mark the item). The text is for logs only and never
//! holds a password: sync formats no credentials anywhere.

use envio_net::NetError;
use envio_store::StoreError;

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error(transparent)]
    Net(#[from] NetError),
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// The spec's failure kinds ("Error handling"), as far as sync can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Network, timeout, server busy: retry with backoff.
    Temporary,
    /// Wrong or revoked password: pause the account, no retry loop.
    SignIn,
    /// Certificate not trusted or changed: stop and ask.
    Certificate,
    /// The server refused this item for good.
    Refused,
    /// The server spoke nonsense; retried like a temporary failure, but
    /// logged.
    Protocol,
    /// Disk or database trouble.
    Local,
}

impl SyncError {
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::Net(error) => net_kind(error),
            Self::Store(_) => ErrorKind::Local,
        }
    }
}

pub(crate) fn net_kind(error: &NetError) -> ErrorKind {
    match error {
        NetError::Io(_) | NetError::Timeout | NetError::Busy(_) => ErrorKind::Temporary,
        NetError::SignIn(_) => ErrorKind::SignIn,
        NetError::Untrusted { .. }
        | NetError::PinChanged { .. }
        | NetError::Tls(_)
        | NetError::NoStartTls => ErrorKind::Certificate,
        NetError::Refused(_) => ErrorKind::Refused,
        NetError::Protocol(_) => ErrorKind::Protocol,
    }
}

pub type Result<T> = std::result::Result<T, SyncError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_failure_has_its_kind() {
        let cases = [
            (NetError::Timeout, ErrorKind::Temporary),
            (NetError::Busy("later".into()), ErrorKind::Temporary),
            (NetError::SignIn("no".into()), ErrorKind::SignIn),
            (NetError::NoStartTls, ErrorKind::Certificate),
            (NetError::Refused("quota".into()), ErrorKind::Refused),
            (NetError::Protocol("junk".into()), ErrorKind::Protocol),
        ];
        for (error, kind) in cases {
            assert_eq!(SyncError::from(error).kind(), kind);
        }
        let store = SyncError::from(StoreError::NotFound);
        assert_eq!(store.kind(), ErrorKind::Local);
    }
}
