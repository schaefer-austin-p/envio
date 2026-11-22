//! What the core tells the app.

use super::command::{AccountSetup, ComposeId, DraftInput, SearchScope, UndoId};
use std::path::PathBuf;

use super::view::{
    AccountInfo, CacheState, FolderRow, MessageRow, MessageView, OutboxRow, Suggestion,
    UnsentCounts,
};
use crate::health::AccountStatus;
use crate::servers::Service;
use envio_store::{Cursor, FolderId, ListRef, MessageRowId, Settings};

/// Which screen shows an Undo offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoScope {
    /// Actions on list rows: the inbox toast.
    List,
    /// An invite answer: the reader's toast.
    Reader,
}

/// What an Inbox sync found that is new to the user (see `Event::NewMail`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMail {
    pub account: String,
    pub count: u32,
    /// (sender, subject) of the one message; set only when `count == 1`.
    pub first: Option<(String, String)>,
}

/// What `Detect` says about signing in with a provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthOffer {
    /// "Microsoft".
    pub provider: String,
    /// The suggested account name.
    pub name_hint: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthFailure {
    Timeout,
    Offline,
    Blocked,
    Other,
    /// The address is already an account.
    Duplicate,
    /// The account does not sign in with Microsoft (or is gone or broken).
    NotOAuth,
    /// No Microsoft sign-in in this build.
    Unavailable,
    /// The account could not be written to disk.
    NotSaved,
    /// A work tenant's admin has not allowed Envio: trying again will not help.
    Admin,
    /// The second sign-in named another account or tenant, or no tenant.
    Mismatch,
    /// The first token of a new account named no tenant, so Envio cannot
    /// tell which kind of account it is.
    NoTenant,
}

impl OAuthFailure {
    /// The translated text that goes with it.
    pub fn text(self) -> String {
        match self {
            Self::Timeout => crate::text::oauth::timeout(),
            Self::Offline => crate::text::oauth::offline(),
            Self::Blocked => crate::text::oauth::blocked(),
            Self::Other => crate::text::oauth::other(),
            Self::Duplicate => crate::text::setup::duplicate(),
            Self::NotOAuth => crate::text::oauth::not_an_oauth_account(),
            Self::Unavailable => crate::text::oauth::no_provider(),
            Self::NotSaved => crate::text::setup::not_saved(),
            Self::Admin => crate::text::oauth::admin(),
            Self::Mismatch => crate::text::oauth::wrong_account(),
            Self::NoTenant => crate::text::oauth::no_tenant(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthOutcome {
    /// A new account was saved (`AccountAdded` came first).
    Added,
    /// "Sign in again" worked and the account runs again.
    Resumed,
    Cancelled,
    Failed(OAuthFailure),
}

#[derive(Debug, Clone)]
pub enum Event {
    /// The store is open; sent at every (re)start of the worker.
    Started {
        accounts: Vec<AccountInfo>,
        cache: CacheState,
    },
    /// The store will not open; the worker keeps trying. Drafts are safe.
    /// `Started` follows once it opens.
    StoreProblem {
        text: String,
    },
    /// The worker panicked and came back.
    WorkerRestarted {
        text: String,
    },
    /// A one-line message for the user.
    Notice {
        text: String,
    },
    /// An account's status changed; `text` is what to show.
    AccountStatus {
        account: String,
        status: AccountStatus,
        text: String,
    },
    /// The account's Inbox unread count changed (sent only when it did).
    AccountUnread {
        account: String,
        unread: u32,
    },
    /// An Inbox sync found mail that is new to the user (not the first sync,
    /// not seen, not their own). Sent once per sync, after the count.
    NewMail(NewMail),
    /// The account's folder list changed.
    FoldersChanged {
        account: String,
    },
    /// Messages in these folders changed (new, gone, flags, bodies).
    MessagesChanged {
        folders: Vec<FolderId>,
    },
    /// The local drafts of `account` changed in a way the Drafts list shows
    /// (made, closed, deleted, uploaded). Not sent for throttled saves.
    LocalDraftsChanged {
        account: String,
    },
    /// The Outbox changed (sent, failed, filed).
    OutboxChanged,
    /// The Outbox as it is now (after `ListOutbox`, a send or file step, or
    /// a decision).
    Outbox {
        items: Vec<OutboxRow>,
    },
    /// Servers found for an address; `text` says where, or asks the user to
    /// confirm the hosts.
    Detected {
        email: String,
        setup: AccountSetup,
        text: String,
        /// The two servers, one per line, whatever the source.
        servers: String,
        /// Set when the address can sign in with its provider.
        oauth: Option<OAuthOffer>,
    },
    /// Open `url` in the browser (the user signs in there).
    OAuthBrowser {
        email: String,
        url: String,
        /// For a device-code flow; always `None` for now.
        user_code: Option<String>,
    },
    /// The sign-in is for another address than the one typed: ask.
    OAuthConfirm {
        email: String,
        signed_in_as: Option<String>,
        text: String,
    },
    /// The sign-in was for the other kind of account than the pick (a work
    /// account picked as Outlook.com, or the reverse): the answer to
    /// `ConfirmOAuth` is yes for the one second sign-in, no to stop.
    OAuthMismatch {
        email: String,
        title: String,
        text: String,
    },
    /// The sign-in ended. `email` is the address it was for.
    OAuthDone {
        email: String,
        outcome: OAuthOutcome,
    },
    /// Adding the account did not work.
    SetupProblem {
        email: String,
        text: String,
    },
    /// The address is one Envio cannot take at all (a Microsoft address
    /// while Microsoft sign-in is off, #381): a card with its own title, not
    /// an error line, and it never comes with a sign-in or a lookup.
    SetupRefused {
        email: String,
        title: String,
        text: String,
    },
    /// The domain's MX records say its mail is on Microsoft 365 (#384): a
    /// card that offers Microsoft sign-in. Comes instead of `Detected`.
    MailOnMicrosoft {
        email: String,
        title: String,
        text: String,
    },
    /// The server's certificate is not trusted: show the fingerprint and
    /// ask; on yes, `Connect` again with it in `accepted`.
    TrustPrompt {
        email: String,
        service: Service,
        host: String,
        /// Names the server (IMAP or SMTP) the prompt is about.
        title: String,
        fingerprint: String,
        hex: String,
        text: String,
    },
    AccountAdded {
        account: AccountInfo,
    },
    /// Unsent work would be lost: ask, then `RemoveAccount` confirmed.
    ConfirmRemove {
        account: String,
        unsent: UnsentCounts,
        text: String,
    },
    /// An account's label changed (blank: cleared).
    AccountLabel {
        account: String,
        label: String,
    },
    AccountRemoved {
        account: String,
    },
    /// The answer to `ListFolders`.
    Folders {
        account: String,
        folders: Vec<FolderRow>,
    },
    /// The answer to `ListMessages`.
    Messages {
        list: ListRef,
        /// The cursor the page was asked after (`None`: the first page).
        after: Option<Cursor>,
        rows: Vec<MessageRow>,
        /// This page was the last: it held fewer than `PAGE` rows.
        end: bool,
    },
    /// `ListMessages` could not read the cache (a `Notice` says so): the
    /// page is not coming.
    MessagesFailed {
        list: ListRef,
        after: Option<Cursor>,
    },
    /// The answer to `Search` (`server: false`), or to `SearchServer`
    /// (`server: true`, every match: cached and fetched, newest first).
    SearchResults {
        scope: SearchScope,
        query: String,
        server: bool,
        rows: Vec<MessageRow>,
    },
    /// A server search that could not ask an account; `text` says why (the
    /// account's status, or the server's refusal). Other accounts of the
    /// same search still answer with `SearchResults`.
    SearchFailed {
        scope: SearchScope,
        query: String,
        text: String,
    },
    /// `LoadOlder` fetched `fetched` messages; `exhausted`: nothing older is
    /// left on the server. `MessagesChanged` follows.
    OlderLoaded {
        list: ListRef,
        fetched: usize,
        exhausted: bool,
    },
    /// What Load older reached in `folder` is dropped (the sync window
    /// changed, or the folder was reset) and its sync has removed those rows,
    /// so a list showing the folder that said "no older messages" can load
    /// older again. The unified Inbox shows the accounts' Inboxes only:
    /// `inbox` says `folder` is one.
    OlderReset {
        folder: FolderId,
        inbox: bool,
    },
    /// `LoadOlder` could not; `text` says why. Pressing it again retries.
    OlderFailed {
        list: ListRef,
        text: String,
    },
    /// The answer to `Autocomplete`, for the same `query`.
    Suggestions {
        query: String,
        items: Vec<Suggestion>,
    },
    /// The settings as they are now: sent after `Started` and after each
    /// save.
    Settings(Settings),
    MessageOpened {
        id: MessageRowId,
        view: MessageView,
    },
    /// The body can't be had now (gone from the server, or the account
    /// can't connect); `text` says so.
    MessageUnavailable {
        id: MessageRowId,
        text: String,
    },
    AttachmentSaved {
        id: MessageRowId,
        index: usize,
        path: PathBuf,
    },
    /// `SaveAttachment` for this attachment did not write the file (a
    /// `Notice` says so too).
    AttachmentFailed {
        id: MessageRowId,
        index: usize,
    },
    /// `ExportInvite` wrote the calendar file.
    InviteExported {
        id: MessageRowId,
        path: PathBuf,
    },
    /// `ExportInvite` did not write it (a `Notice` says so too).
    InviteExportFailed {
        id: MessageRowId,
    },
    /// An action was done; it can be taken back until `expires_at_ms`
    /// (unix milliseconds).
    UndoOffer {
        undo: UndoId,
        text: String,
        expires_at_ms: i64,
        scope: UndoScope,
    },
    /// The offer is over: taken back (`undone`) or gone for good.
    UndoClosed {
        undo: UndoId,
        undone: bool,
    },
    /// The answer to `StartCompose`: the fields to show. `quoted` is the
    /// reply or forward text, kept apart so the app can fold it.
    ComposeReady {
        compose: ComposeId,
        draft: DraftInput,
        quoted: String,
        /// A note Compose shows while it is open; empty for none.
        note: String,
    },
    /// The draft was stored for the first time as `draft_id`.
    DraftSaved {
        compose: ComposeId,
        draft_id: String,
    },
    /// A file was copied into the draft: `path` is the copy, which is what
    /// the draft's attachments hold.
    Attached {
        compose: ComposeId,
        path: PathBuf,
        name: String,
        size: u64,
    },
    /// The file could not be attached; `text` says why.
    AttachFailed {
        compose: ComposeId,
        text: String,
    },
    /// Compose can not go on (or `Send` did not queue); `text` says why.
    /// The draft is kept.
    ComposeRefused {
        compose: ComposeId,
        text: String,
    },
    /// The message is in the Outbox as `item`.
    Queued {
        compose: ComposeId,
        item: String,
        /// For the toast that tells the user ("Sent to the Outbox").
        text: String,
    },
}
