//! OAuth 2.0 (authorization code with PKCE) for Microsoft accounts: the
//! pure parts. PKCE, authorize URL, redirect and token-reply parsing, the
//! identity check and the provider table. No I/O here; the token client
//! is a separate, thin layer. Nothing secret reaches `Debug`, errors or logs.

mod client;
pub mod client_ids;
mod pkce;
mod provider;
mod redirect;
mod token;
mod urlenc;

#[doc(hidden)]
pub use client::refresh_within;
pub use client::{exchange_code, refresh};
pub use pkce::Pkce;
pub use provider::{
    Provider, ProviderKind, consumer_microsoft_domain, is_consumer_domain, provider_kind_for,
};
pub use redirect::{Redirect, RedirectError, authorize_url, parse_redirect};
pub use token::{
    CONSUMER_TENANT, Grant, Identity, TokenError, Tokens, identity_matches, identity_of,
    parse_token_reply, token_request_body,
};
