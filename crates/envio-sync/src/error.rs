//! Sync's error: a network or a store failure, sorted into the spec's
//! failure kinds so core can pick the fixed response (retry, pause the
//! account, stop, mark the item). The text is for logs only and never
//! holds a password: sync formats no credentials anywhere.

use envio_net::{NetError, StorageKind};
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
    /// The local database is damaged: stop and rebuild it from replay.
    Damaged,
}

impl SyncError {
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::Net(error) => net_kind(error),
            Self::Store(e) if e.is_damage() => ErrorKind::Damaged,
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
        NetError::Storage {
            kind: StorageKind::Damaged,
            ..
        } => ErrorKind::Damaged,
        NetError::Storage { .. } => ErrorKind::Local,
        // The send driver reads `unconfirmed` before this; any other path
        // treats it like a dropped connection.
        NetError::Unconfirmed(_) => ErrorKind::Temporary,
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

    #[test]
    fn damage_is_its_own_kind_for_store_and_graph_storage_errors() {
        let damaged = SyncError::from(StoreError::Damaged("bad page".into()));
        assert_eq!(damaged.kind(), ErrorKind::Damaged);
        let storage = |kind| NetError::Storage {
            kind,
            msg: "x".into(),
        };
        assert_eq!(net_kind(&storage(StorageKind::Damaged)), ErrorKind::Damaged);
        assert_eq!(net_kind(&storage(StorageKind::DiskFull)), ErrorKind::Local);
        assert_eq!(net_kind(&storage(StorageKind::Other)), ErrorKind::Local);
        assert_eq!(
            net_kind(&NetError::Unconfirmed("cut".into())),
            ErrorKind::Temporary
        );
    }
}
