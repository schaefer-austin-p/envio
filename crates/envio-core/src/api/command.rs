//! What the app asks the core to do.

use std::collections::HashMap;
use std::path::PathBuf;

use envio_net::autoconfig::Source;
use envio_store::{Cursor, FolderId, ListRef, MessageRowId, Settings};
use secrecy::SecretString;

use crate::servers::{ServerAddr, Service};

/// Where a search looks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchScope {
    /// Every account (locally); a server search takes each Inbox.
    All,
    /// One account (a server search takes its Inbox).
    Account(String),
    Folder(FolderId),
}

/// Whether `OpenMessage` loads remote images.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImagesChoice {
    /// As the settings say: for an allowed sender, or when images always
    /// load. Otherwise they stay blocked.
    Default,
    /// The user asked for them for this message.
    Load,
}

/// What the wizard has settled on for a new account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountSetup {
    pub name: String,
    pub email: String,
    pub username: String,
    pub imap: ServerAddr,
    pub smtp: ServerAddr,
    pub source: Option<Source>,
    /// The servers came from unauthenticated DNS: the user must agree
    /// to the hosts before the password goes to them.
    pub needs_confirm: bool,
}

#[derive(Debug, Clone)]
pub struct ConnectRequest {
    pub setup: AccountSetup,
    pub password: SecretString,
    /// The user agreed to the hosts shown (needed when `needs_confirm`).
    pub servers_confirmed: bool,
    /// Hex fingerprints of certificates the user accepted.
    pub accepted: Vec<(Service, String)>,
}

/// What the user did to messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserAction {
    MarkRead(bool),
    Flag(bool),
    Archive,
    Delete,
    Move { to: FolderId },
}

/// How the user answers a meeting invite. Kept here so the app's glue never
/// names an `envio_mime` type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Accepted,
    Tentative,
    Declined,
}

impl Answer {
    /// The word the card contract uses: `accepted`, `tentative`, `declined`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Tentative => "tentative",
            Self::Declined => "declined",
        }
    }

    /// Anything else is `None`.
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "accepted" => Some(Self::Accepted),
            "tentative" => Some(Self::Tentative),
            "declined" => Some(Self::Declined),
            _ => None,
        }
    }
}

/// One offer to take an action back, from `Event::UndoOffer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UndoId(pub u64);

/// One compose window, chosen by the app and unique among open ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComposeId(pub u64);

/// What compose holds. Recipients are as typed, `Name <a@b>` or `a@b`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DraftInput {
    pub account: String,
    /// The stored draft this one was saved as, if it was.
    pub draft_id: Option<String>,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub body: String,
    /// Without angle brackets.
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    /// Files to attach, read when the message is sent.
    pub attachments: Vec<PathBuf>,
}

/// What the user decides about a queued message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboxDecision {
    SendAgain,
    MoveToSent,
    Retry,
    Delete,
}

/// Where a compose window starts from. `Draft` and `Mailto` carry their
/// data in `StartCompose`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartFrom {
    New {
        account: String,
    },
    Reply {
        id: MessageRowId,
        all: bool,
    },
    /// Quotes the message and attaches its attachments.
    Forward {
        id: MessageRowId,
    },
    Draft,
    /// A cached message in a Drafts-role folder.
    Edit {
        id: MessageRowId,
    },
    Mailto,
}

#[derive(Debug)]
pub enum Command {
    /// Flush unsent work and stop the worker.
    Shutdown,
    /// The app came to the foreground (`true`) or went to the background.
    AppActive(bool),
    /// Sync now: one account, or all. Ends an Offline wait.
    Refresh { account: Option<String> },
    /// Look up the servers for an address (only its domain leaves the phone).
    Detect { email: String },
    /// Check the servers and password, then add the account.
    Connect(Box<ConnectRequest>),
    /// Sign in with the provider of `email`'s domain in the browser. With
    /// `account` (the key of an OAuth account) it is "Sign in again".
    StartOAuth {
        email: String,
        account: Option<String>,
    },
    /// The `envio-oauth://` link the browser sent back.
    OAuthRedirect { url: String },
    /// The answer to `Event::OAuthConfirm`.
    ConfirmOAuth { accept: bool },
    /// Give up the sign-in that waits.
    CancelOAuth,
    /// A new password for an account whose sign-in failed.
    UpdatePassword {
        account: String,
        password: SecretString,
    },
    /// Trust the certificate the account's server presented.
    AcceptCertificate {
        account: String,
        service: Service,
        fingerprint: String,
    },
    /// Remove an account and everything kept for it. Asks first when
    /// work is unsent, unless `confirmed`.
    RemoveAccount { account: String, confirmed: bool },
    /// Name the account for the tabs, drawer and Accounts page; blank
    /// clears it. The sender name on mail is not touched.
    SetAccountLabel { account: String, label: String },
    /// The account's folders, from the cache.
    ListFolders { account: String },
    /// One page (`PAGE` rows) of a list, newest first, from the cache:
    /// the first page, or the one after `after`.
    ListMessages {
        list: ListRef,
        after: Option<Cursor>,
    },
    /// Header and body search over cached mail (`All`: every account).
    Search { scope: SearchScope, query: String },
    /// Ask the server too: the folder of `scope` (an account's or the
    /// unified Inbox: each account's Inbox), found mail is cached.
    SearchServer { scope: SearchScope, query: String },
    /// Fetch the next mail older than what the list holds (for `Inboxes`:
    /// the same for each account's Inbox). `tail` is the date of the list's
    /// last shown row: batches dated above it show nothing new, so the
    /// fetch goes on (up to a limit) until mail below it arrives; `None`
    /// fetches one batch. The answer is `OlderLoaded` or `OlderFailed`.
    LoadOlder { list: ListRef, tail: Option<i64> },
    /// Show a message; its body is fetched first if it is not cached.
    OpenMessage {
        id: MessageRowId,
        /// Remote images stay blocked unless the settings or the user
        /// say otherwise.
        images: ImagesChoice,
        /// Widest image or table, in pixels.
        width_px: u32,
    },
    /// The natural pixel size (width, height) of remote images the reader
    /// loaded for message `id`, by URL. When the open message is that one
    /// and a size changes how it draws (an image with no width wider than
    /// the reader), it comes again as `MessageOpened`.
    ImageSizes {
        id: MessageRowId,
        sizes: HashMap<String, (u32, u32)>,
    },
    /// Write an attachment into `dir` under a safe name.
    SaveAttachment {
        id: MessageRowId,
        index: usize,
        dir: PathBuf,
    },
    /// Write the meeting invite in cached message `id` into `dir` as a
    /// file a calendar app can import; the answer is `InviteExported` or
    /// `InviteExportFailed`.
    ExportInvite { id: MessageRowId, dir: PathBuf },
    /// Do `action` to the messages; the answer is an `UndoOffer`.
    Act {
        ids: Vec<MessageRowId>,
        action: UserAction,
    },
    /// Take back an offered action while its window is open.
    Undo { undo: UndoId },
    /// Answer the meeting invite in a cached message. The reply is queued at
    /// once but held for the Undo window; the answer is an `UndoOffer`
    /// (scope `Reader`) and `MessageOpened` again with the new card.
    AnswerInvite {
        id: MessageRowId,
        answer: Answer,
        /// Added to the reply; may be empty.
        note: String,
    },
    /// Open a compose window; the answer is `ComposeReady` or `ComposeRefused`.
    StartCompose {
        compose: ComposeId,
        from: StartFrom,
        /// For `StartFrom::Draft`.
        draft_id: Option<String>,
        /// For `StartFrom::Mailto`.
        mailto: Option<String>,
    },
    /// The user changed the draft; it is saved within a second.
    DraftChanged {
        compose: ComposeId,
        draft: DraftInput,
    },
    /// The window closed: keep the draft, or delete it.
    CloseCompose { compose: ComposeId, discard: bool },
    /// A file the user picked: copied into the draft's folder; the answer is
    /// `Attached` or `AttachFailed`.
    Attach { compose: ComposeId, path: PathBuf },
    /// Queue the message in the Outbox; the answer is `Queued` or
    /// `ComposeRefused`.
    Send {
        compose: ComposeId,
        draft: DraftInput,
    },
    /// List the Outbox; the answer is `Outbox`.
    ListOutbox,
    /// The user's decision about a queued message; refused with a `Notice`
    /// when the item's state does not allow it.
    DecideOutbox {
        item: String,
        decision: OutboxDecision,
    },
    /// Turn a refused message into a draft in compose window `compose`, then
    /// remove it from the Outbox.
    EditOutbox { item: String, compose: ComposeId },
    /// Addresses to suggest for what the user has typed in a recipient field.
    Autocomplete { query: String },
    /// Keep a sender in the contacts ("Add sender to Contacts").
    AddContact { name: String, address: String },
    /// Save the settings except the allowed image senders and the
    /// notification switches, which the worker keeps as they are (they
    /// change only by `AllowImagesFrom`, `DisallowImagesFrom`, `SetNotify`
    /// and `SetNotifyText`); the answer is `Settings`.
    SaveSettings(Settings),
    /// Turn one account's new-mail notifications on or off. Applied to the
    /// worker's own settings, so toggles never overwrite one another; the
    /// answer is `Settings`.
    SetNotify { account: String, on: bool },
    /// Show the sender and subject in new-mail notifications, or not; the
    /// answer is `Settings`.
    SetNotifyText { on: bool },
    /// Always load remote images from this sender. Open the message again
    /// to see them.
    AllowImagesFrom { address: String },
    /// Take a sender off the allowed list ("Remove" in the image settings).
    DisallowImagesFrom { address: String },
    /// Panic the worker, to test the restart.
    #[cfg(feature = "test-hooks")]
    Crash,
    /// Make the worker sleep, to test what must not wait for it.
    #[cfg(feature = "test-hooks")]
    Stall { ms: u64 },
}

impl Command {
    /// The variant's name, for logs (never the payload).
    pub fn name(&self) -> &'static str {
        match self {
            Self::Shutdown => "Shutdown",
            Self::AppActive(_) => "AppActive",
            Self::Refresh { .. } => "Refresh",
            Self::Detect { .. } => "Detect",
            Self::Connect(_) => "Connect",
            Self::StartOAuth { .. } => "StartOAuth",
            Self::OAuthRedirect { .. } => "OAuthRedirect",
            Self::ConfirmOAuth { .. } => "ConfirmOAuth",
            Self::CancelOAuth => "CancelOAuth",
            Self::UpdatePassword { .. } => "UpdatePassword",
            Self::AcceptCertificate { .. } => "AcceptCertificate",
            Self::RemoveAccount { .. } => "RemoveAccount",
            Self::SetAccountLabel { .. } => "SetAccountLabel",
            Self::ListFolders { .. } => "ListFolders",
            Self::ListMessages { .. } => "ListMessages",
            Self::Search { .. } => "Search",
            Self::SearchServer { .. } => "SearchServer",
            Self::LoadOlder { .. } => "LoadOlder",
            Self::OpenMessage { .. } => "OpenMessage",
            Self::ImageSizes { .. } => "ImageSizes",
            Self::SaveAttachment { .. } => "SaveAttachment",
            Self::Act { .. } => "Act",
            Self::Undo { .. } => "Undo",
            Self::AnswerInvite { .. } => "AnswerInvite",
            Self::ExportInvite { .. } => "ExportInvite",
            Self::StartCompose { .. } => "StartCompose",
            Self::DraftChanged { .. } => "DraftChanged",
            Self::CloseCompose { .. } => "CloseCompose",
            Self::Attach { .. } => "Attach",
            Self::Send { .. } => "Send",
            Self::ListOutbox => "ListOutbox",
            Self::DecideOutbox { .. } => "DecideOutbox",
            Self::EditOutbox { .. } => "EditOutbox",
            Self::Autocomplete { .. } => "Autocomplete",
            Self::AddContact { .. } => "AddContact",
            Self::SaveSettings(_) => "SaveSettings",
            Self::SetNotify { .. } => "SetNotify",
            Self::SetNotifyText { .. } => "SetNotifyText",
            Self::AllowImagesFrom { .. } => "AllowImagesFrom",
            Self::DisallowImagesFrom { .. } => "DisallowImagesFrom",
            #[cfg(feature = "test-hooks")]
            Self::Crash => "Crash",
            #[cfg(feature = "test-hooks")]
            Self::Stall { .. } => "Stall",
        }
    }
}
