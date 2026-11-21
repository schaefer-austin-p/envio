//! What the driver needs from an IMAP or SMTP session, as traits, so the
//! driver runs against envio-net's sessions in the app and against an
//! in-memory server in tests. IDLE stays on `ImapSession` itself: envio-core
//! waits on it between sync runs.

use envio_net::Result;
use envio_net::imap::{Caps, Flag, FlagUpdate, Folder, FolderState, ImapSession, Summary};
use envio_net::smtp::{SendError, SmtpSession};

/// An IMAP session. Every call but `caps` may fail; after a failure the
/// driver drops the session.
///
/// The futures are deliberately not `Send` (`async_fn_in_trait`): core runs
/// them on one current-thread runtime with `block_on` and never moves them
/// between threads.
#[allow(async_fn_in_trait)]
pub trait MailServer {
    fn caps(&self) -> Caps;
    async fn list_folders(&mut self) -> Result<Vec<Folder>>;
    async fn select(&mut self, path: &str) -> Result<FolderState>;
    async fn search_since(&mut self, unix_secs: i64) -> Result<Vec<u32>>;
    /// Every UID in the selected folder (sync window "All", "Load older").
    async fn search_all(&mut self) -> Result<Vec<u32>>;
    async fn search_message_id(&mut self, message_id: &str) -> Result<Vec<u32>>;
    /// Like `search_message_id`, but a server that cannot search headers is
    /// scanned from end to end (for small folders such as Drafts).
    async fn search_message_id_deep(&mut self, message_id: &str) -> Result<Vec<u32>>;
    /// "Search on server" in the selected folder.
    async fn search_text(&mut self, text: &str) -> Result<Vec<u32>>;
    async fn fetch_summaries(&mut self, uids: &[u32]) -> Result<Vec<Summary>>;
    async fn fetch_flags(
        &mut self,
        uids: &[u32],
        changed_since: Option<u64>,
    ) -> Result<Vec<FlagUpdate>>;
    async fn fetch_body(&mut self, uid: u32, size: usize) -> Result<Option<Vec<u8>>>;
    /// A big message with only its text and small inline images; its other
    /// parts are stubs (see `ImapSession::fetch_partial`).
    async fn fetch_partial(&mut self, uid: u32) -> Result<Option<Vec<u8>>>;
    /// One part of a message as a message of its own (headers, then the
    /// transfer-encoded body); `size` is the part's, for the time limit.
    async fn fetch_section(
        &mut self,
        uid: u32,
        section: &str,
        size: usize,
    ) -> Result<Option<Vec<u8>>>;
    async fn store_flags(&mut self, uids: &[u32], add: bool, flags: &[Flag]) -> Result<()>;
    async fn move_to(&mut self, uids: &[u32], target: &str) -> Result<()>;
    async fn delete(&mut self, uids: &[u32]) -> Result<()>;
    async fn append(&mut self, folder: &str, flags: &[Flag], eml: &[u8]) -> Result<()>;
    /// Whether the server reported a change in the selected folder since the
    /// last call (IMAP's unsolicited replies). Drains what was reported.
    fn take_changes(&mut self) -> bool {
        false
    }
}

/// An SMTP session.
/// Like `MailServer`, its futures are deliberately not `Send`: one
/// current-thread runtime, `block_on`.
#[allow(async_fn_in_trait)]
pub trait MailSender {
    async fn send(
        &mut self,
        from: &str,
        to: &[String],
        eml: &[u8],
    ) -> std::result::Result<(), SendError>;
}

impl MailServer for ImapSession {
    fn caps(&self) -> Caps {
        ImapSession::caps(self)
    }

    async fn list_folders(&mut self) -> Result<Vec<Folder>> {
        ImapSession::list_folders(self).await
    }

    async fn select(&mut self, path: &str) -> Result<FolderState> {
        ImapSession::select(self, path).await
    }

    async fn search_since(&mut self, unix_secs: i64) -> Result<Vec<u32>> {
        ImapSession::search_since(self, unix_secs).await
    }

    async fn search_all(&mut self) -> Result<Vec<u32>> {
        ImapSession::search_all(self).await
    }

    async fn search_message_id(&mut self, message_id: &str) -> Result<Vec<u32>> {
        ImapSession::search_message_id(self, message_id).await
    }

    async fn search_message_id_deep(&mut self, message_id: &str) -> Result<Vec<u32>> {
        ImapSession::search_message_id_deep(self, message_id).await
    }

    async fn search_text(&mut self, text: &str) -> Result<Vec<u32>> {
        ImapSession::search_text(self, text).await
    }

    async fn fetch_summaries(&mut self, uids: &[u32]) -> Result<Vec<Summary>> {
        ImapSession::fetch_summaries(self, uids).await
    }

    async fn fetch_flags(
        &mut self,
        uids: &[u32],
        changed_since: Option<u64>,
    ) -> Result<Vec<FlagUpdate>> {
        ImapSession::fetch_flags(self, uids, changed_since).await
    }

    async fn fetch_body(&mut self, uid: u32, size: usize) -> Result<Option<Vec<u8>>> {
        ImapSession::fetch_body(self, uid, size).await
    }

    async fn fetch_partial(&mut self, uid: u32) -> Result<Option<Vec<u8>>> {
        ImapSession::fetch_partial(self, uid).await
    }

    async fn fetch_section(
        &mut self,
        uid: u32,
        section: &str,
        size: usize,
    ) -> Result<Option<Vec<u8>>> {
        ImapSession::fetch_section(self, uid, section, size).await
    }

    async fn store_flags(&mut self, uids: &[u32], add: bool, flags: &[Flag]) -> Result<()> {
        ImapSession::store_flags(self, uids, add, flags).await
    }

    async fn move_to(&mut self, uids: &[u32], target: &str) -> Result<()> {
        ImapSession::move_to(self, uids, target).await
    }

    async fn delete(&mut self, uids: &[u32]) -> Result<()> {
        ImapSession::delete(self, uids).await
    }

    async fn append(&mut self, folder: &str, flags: &[Flag], eml: &[u8]) -> Result<()> {
        ImapSession::append(self, folder, flags, eml).await
    }

    fn take_changes(&mut self) -> bool {
        ImapSession::take_changes(self)
    }
}

impl MailSender for SmtpSession {
    async fn send(
        &mut self,
        from: &str,
        to: &[String],
        eml: &[u8],
    ) -> std::result::Result<(), SendError> {
        SmtpSession::send(self, from, to, eml).await
    }
}
