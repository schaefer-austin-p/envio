//! The names the sign-in flow uses across the Qt boundary, pinned once.
//! `qt/` emits the outcome names to QML and parses the kind strings QML
//! passes back; `tests/qml_names.rs` checks the QML files against these.

use envio_core::{OAuthFailure, OAuthOutcome, ProviderKind};

/// Every outcome name QML can receive in `oauthDone`.
pub const OUTCOME_NAMES: [&str; 8] = [
    "added",
    "resumed",
    "updated",
    "cancelled",
    "admin",
    "work",
    "mismatch",
    "failed",
];

/// The kind string for Outlook.com (Graph) sign-in.
pub const KIND_GRAPH: &str = "graph";
/// The kind string for IMAP and SMTP with OAuth.
pub const KIND_IMAP: &str = "imap";
/// Every kind string QML can pass to `startOAuth` (empty means "default").
pub const KIND_NAMES: [&str; 2] = [KIND_GRAPH, KIND_IMAP];

/// The name QML gets for an outcome. Exhaustive: a new variant must be named
/// here before it compiles.
pub fn outcome_name(outcome: &OAuthOutcome) -> &'static str {
    match outcome {
        OAuthOutcome::Added => "added",
        OAuthOutcome::Resumed => "resumed",
        OAuthOutcome::Updated => "updated",
        OAuthOutcome::Cancelled => "cancelled",
        OAuthOutcome::Failed(why) => match why {
            OAuthFailure::Admin => "admin",
            OAuthFailure::NotPersonal => "work",
            OAuthFailure::Mismatch | OAuthFailure::NoTenant => "mismatch",
            OAuthFailure::Timeout
            | OAuthFailure::Offline
            | OAuthFailure::Blocked
            | OAuthFailure::Other
            | OAuthFailure::Duplicate
            | OAuthFailure::NotOAuth
            | OAuthFailure::Unavailable
            | OAuthFailure::NotSaved => "failed",
        },
    }
}

/// The provider kind QML asked for; `None` for empty or unknown (the
/// address's own provider, or the account's).
pub fn parse_kind(kind: &str) -> Option<ProviderKind> {
    match kind {
        KIND_GRAPH => Some(ProviderKind::MicrosoftGraph),
        KIND_IMAP => Some(ProviderKind::Microsoft),
        _ => None,
    }
}

/// The inverse of `parse_kind`, exhaustive over `ProviderKind`.
pub fn kind_name(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::MicrosoftGraph => KIND_GRAPH,
        ProviderKind::Microsoft => KIND_IMAP,
    }
}
