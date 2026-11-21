//! The account's mail connection: one enum over the kinds of server, so
//! the step functions are generic over `MailServer` and the worker holds
//! one type.

use envio_net::Result;
use envio_net::imap::{Caps, Flag, FlagUpdate, Folder, FolderState, ImapSession, Summary};
use envio_sync::server::MailServer;

// One connection per account, moved in and out: its size does not matter.
#[allow(clippy::large_enum_variant)]
pub(crate) enum Backend {
    Imap(ImapSession),
}

impl MailServer for Backend {
    fn caps(&self) -> Caps {
        match self {
            Self::Imap(s) => s.caps(),
        }
    }

    async fn list_folders(&mut self) -> Result<Vec<Folder>> {
        match self {
            Self::Imap(s) => s.list_folders().await,
        }
    }

    async fn select(&mut self, path: &str) -> Result<FolderState> {
        match self {
            Self::Imap(s) => MailServer::select(s, path).await,
        }
    }

    async fn search_since(&mut self, unix_secs: i64) -> Result<Vec<u32>> {
        match self {
            Self::Imap(s) => s.search_since(unix_secs).await,
        }
    }

    async fn search_all(&mut self) -> Result<Vec<u32>> {
        match self {
            Self::Imap(s) => s.search_all().await,
        }
    }

    async fn search_message_id(&mut self, message_id: &str) -> Result<Vec<u32>> {
        match self {
            Self::Imap(s) => s.search_message_id(message_id).await,
        }
    }

    async fn search_message_id_deep(&mut self, message_id: &str) -> Result<Vec<u32>> {
        match self {
            Self::Imap(s) => s.search_message_id_deep(message_id).await,
        }
    }

    async fn search_text(&mut self, text: &str) -> Result<Vec<u32>> {
        match self {
            Self::Imap(s) => s.search_text(text).await,
        }
    }

    async fn fetch_summaries(&mut self, uids: &[u32]) -> Result<Vec<Summary>> {
        match self {
            Self::Imap(s) => s.fetch_summaries(uids).await,
        }
    }

    async fn fetch_flags(
        &mut self,
        uids: &[u32],
        changed_since: Option<u64>,
    ) -> Result<Vec<FlagUpdate>> {
        match self {
            Self::Imap(s) => s.fetch_flags(uids, changed_since).await,
        }
    }

    async fn fetch_body(&mut self, uid: u32, size: usize) -> Result<Option<Vec<u8>>> {
        match self {
            Self::Imap(s) => s.fetch_body(uid, size).await,
        }
    }

    async fn fetch_partial(&mut self, uid: u32) -> Result<Option<Vec<u8>>> {
        match self {
            Self::Imap(s) => s.fetch_partial(uid).await,
        }
    }

    async fn fetch_section(
        &mut self,
        uid: u32,
        section: &str,
        size: usize,
    ) -> Result<Option<Vec<u8>>> {
        match self {
            Self::Imap(s) => s.fetch_section(uid, section, size).await,
        }
    }

    async fn store_flags(&mut self, uids: &[u32], add: bool, flags: &[Flag]) -> Result<()> {
        match self {
            Self::Imap(s) => s.store_flags(uids, add, flags).await,
        }
    }

    async fn move_to(&mut self, uids: &[u32], target: &str) -> Result<()> {
        match self {
            Self::Imap(s) => s.move_to(uids, target).await,
        }
    }

    async fn delete(&mut self, uids: &[u32]) -> Result<()> {
        match self {
            Self::Imap(s) => s.delete(uids).await,
        }
    }

    async fn append(&mut self, folder: &str, flags: &[Flag], eml: &[u8]) -> Result<()> {
        match self {
            Self::Imap(s) => s.append(folder, flags, eml).await,
        }
    }

    fn take_changes(&mut self) -> bool {
        match self {
            Self::Imap(s) => MailServer::take_changes(s),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/support/fake.rs"]
#[allow(dead_code)]
mod fake;

#[cfg(test)]
mod tests {
    use envio_net::imap::ImapSession;
    use envio_net::{Credentials, Endpoint, Fingerprint, Roots, Security};

    use super::fake;
    use super::*;

    #[test]
    fn imap_forwards_caps_and_take_changes() {
        let server = fake::serve(fake::Kind::Imap);
        let mut endpoint = Endpoint::new("127.0.0.1", server.port, Security::Tls);
        endpoint.pin = Fingerprint::from_hex(&server.hex);
        let login = Credentials::new("me@example.org", "pw");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let session = runtime
            .block_on(ImapSession::open(&endpoint, &Roots::empty(), &login))
            .unwrap();
        let caps = session.caps();
        let mut backend = Backend::Imap(session);
        assert_eq!(MailServer::caps(&backend), caps);
        assert!(!MailServer::take_changes(&mut backend));
    }
}
