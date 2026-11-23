//! Envio's logic. Qt-free: the app talks to it through qt/.

pub mod api;
pub mod build_info;
pub mod core;
pub mod health;
pub mod logging;
pub mod oauth_tokens;
pub mod paths;
pub mod problem;
pub mod secrets;
pub mod servers;
mod startup;

#[cfg(feature = "test-hooks")]
#[doc(hidden)]
pub mod hooks;

mod alert;
mod app_password;
mod cause;
mod clock;
mod compose;
mod drafts;
mod fsutil;
mod invite_card;
mod known;
mod migrate_graph;
mod outbox;
mod setup_thread;
// The helpers below are still partly consumed (tr_n, mask, more text).
#[allow(dead_code)]
mod i18n;
#[allow(dead_code)]
mod mask;
mod supervisor;
#[allow(dead_code)]
mod text;
mod undo;
mod worker;

pub use alert::{Alert, Poster};
pub use api::command::{
    AccountSetup, Answer, Command, ComposeId, ConnectRequest, DraftInput, ImagesChoice,
    OutboxDecision, SearchScope, StartFrom, UndoId, UserAction,
};
pub use api::event::{
    AppPasswordHint, Event, NewMail, OAuthFailure, OAuthOffer, OAuthOutcome, UndoScope,
};
pub use api::view::{
    AccountInfo, AttachmentRow, CacheState, CardMethod, CardState, FolderRow, InviteCard,
    MessageHeader, MessageRow, MessageView, MigrationStage, MigrationState, OutboxRow,
    OutboxStatus, Suggestion, UnsentCounts,
};
pub use app_password::{AppPasswordProvider, app_password_help_url, app_password_provider};
pub use core::{Config, Core, EventSink, default_oauth_providers, microsoft_sign_in_offered};
pub use envio_net::oauth::ProviderKind;
pub use envio_store::{
    Contact, Cursor, DarkMode, Flags, FolderId, FolderRole, ImagesDefault, LIST_PANE_DEFAULT_GU,
    LIST_PANE_MAX_GU, LIST_PANE_MIN_GU, ListRef, MessageRowId, PAGE, Settings, ThemeName,
};
pub use health::AccountStatus;
pub use known::{app_password_setup, google_found};
pub use problem::CertDetails;
pub use servers::{ServerAddr, Service};
pub use startup::saved_settings;
pub use text::count::{CountKind, count_text};
pub use text::link::{looks_like, malformed as link_malformed, userinfo_decoration};
pub use text::notice::start_failed;
pub use text::outbox::{check_certificate_label, open_accounts_label};
pub use text::setup::{ispdb_note, trust_title};
// What the app's glue names without depending on envio-net.
pub use envio_net::autoconfig::Source as SetupSource;
pub use envio_net::{Roots, RootsSource, Security};
