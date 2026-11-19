//! The words of signing in with a provider (Microsoft today). No Microsoft
//! error text and no code or token ever goes into any of them.

use crate::i18n::{fill, tr};

pub fn found(provider: &str) -> String {
    fill(
        &tr("Sign in with {provider} to add this account"),
        &[("provider", provider)],
    )
}

/// "You signed in as X. Use this account for Y?" (decision 9).
pub fn confirm(signed_in_as: Option<&str>, typed: &str) -> String {
    match signed_in_as {
        Some(who) => fill(
            &tr("You signed in as {who}. Use this account for {typed}?"),
            &[("who", who), ("typed", typed)],
        ),
        None => fill(
            &tr("You signed in to a Microsoft account. Use it for {typed}?"),
            &[("typed", typed)],
        ),
    }
}

pub fn timeout() -> String {
    tr("The sign-in took too long and was stopped. Start again")
}

pub fn offline() -> String {
    tr("Could not reach Microsoft. Check the connection and start again")
}

pub fn blocked() -> String {
    tr("Microsoft or your organisation did not allow this sign-in")
}

pub fn other() -> String {
    tr("The sign-in did not work. Start again")
}

pub fn stale_link() -> String {
    tr("That sign-in link is no longer valid. Start again")
}

pub fn no_provider() -> String {
    tr("Signing in with Microsoft is not available in this version")
}

pub fn not_an_oauth_account() -> String {
    tr("This account does not sign in with Microsoft")
}

pub fn nothing_to_confirm() -> String {
    tr("There is no sign-in waiting for an answer")
}

pub fn no_password() -> String {
    tr("This account signs in with Microsoft and has no password. Sign in again instead")
}
