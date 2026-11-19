//! Token requests and replies, the id_token identity and the match rule.

use super::Provider;
use super::urlenc::{form, safe_code};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use secrecy::SecretString;
use serde::Deserialize;

/// The mailbox claims of an id_token; either may be absent.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Identity {
    pub email: Option<String>,
    pub preferred_username: Option<String>,
}

pub struct Tokens {
    pub access: SecretString,
    pub refresh: Option<SecretString>,
    /// Seconds; 0 when the reply gave none or a negative number.
    pub expires_in: u64,
    pub identity: Option<Identity>,
}

impl std::fmt::Debug for Tokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tokens")
            .field("expires_in", &self.expires_in)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenError {
    /// The grant is gone or needs the user: sign in again.
    Revoked,
    /// Network, 5xx or 429: try later.
    Temporary,
    /// Anything else; the provider's error code only.
    Refused(String),
}

pub enum Grant<'a> {
    Code { code: &'a str, verifier: &'a str },
    Refresh { token: &'a str },
}

/// The form body for the token endpoint. It holds a code or a refresh
/// token, so it is a secret.
pub fn token_request_body(p: &Provider, grant: &Grant<'_>) -> SecretString {
    let body = match grant {
        Grant::Code { code, verifier } => form(&[
            ("client_id", &p.client_id),
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &p.redirect_uri),
            ("code_verifier", verifier),
        ]),
        Grant::Refresh { token } => form(&[
            ("client_id", &p.client_id),
            ("grant_type", "refresh_token"),
            ("refresh_token", token),
            ("scope", p.scopes),
        ]),
    };
    SecretString::from(body)
}

#[derive(Deserialize)]
struct Reply {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    id_token: Option<String>,
    error: Option<String>,
}

/// Classify the token endpoint's answer. No part of the body reaches an
/// error except the provider's `error` code, and only if it is a plain code.
pub fn parse_token_reply(status: u16, body: &[u8]) -> Result<Tokens, TokenError> {
    if status == 429 || status >= 500 {
        return Err(TokenError::Temporary);
    }
    let success = (200..300).contains(&status);
    // A body that is not JSON at all (a captive portal's or proxy's page,
    // at any status; an empty or cut reply) is not the provider speaking:
    // try again later, never ask for a sign-in over it. OAuth errors are
    // always JSON, so this cannot hide `invalid_grant`.
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(body) else {
        return Err(TokenError::Temporary);
    };
    // JSON of the wrong shape is the provider speaking, wrongly.
    let reply: Reply = serde_json::from_value(json)
        .map_err(|_| TokenError::Refused("unreadable reply".to_owned()))?;
    if success {
        return tokens(reply);
    }
    match reply.error.as_deref() {
        Some("invalid_grant" | "interaction_required" | "consent_required" | "invalid_client")
            if status == 400 =>
        {
            Err(TokenError::Revoked)
        }
        Some(code) => Err(TokenError::Refused(safe_code(code))),
        None => Err(TokenError::Refused(format!("http_{status}"))),
    }
}

fn tokens(r: Reply) -> Result<Tokens, TokenError> {
    let access = r
        .access_token
        .filter(|a| !a.is_empty())
        .ok_or_else(|| TokenError::Refused("no_access_token".to_owned()))?;
    Ok(Tokens {
        access: SecretString::from(access),
        refresh: r
            .refresh_token
            .filter(|t| !t.is_empty())
            .map(SecretString::from),
        expires_in: r.expires_in.map_or(0, |n| u64::try_from(n).unwrap_or(0)),
        identity: r.id_token.as_deref().and_then(identity_of),
    })
}

/// The `email` and `preferred_username` claims of an id_token, or `None`
/// when it is not a JWT or has neither. The signature is NOT verified: the
/// token came straight from the token endpoint over TLS, which is the OIDC
/// rule for the code flow. Never use this on a token from anywhere else.
pub fn identity_of(id_token: &str) -> Option<Identity> {
    let payload = id_token.split('.').nth(1)?;
    let raw = B64.decode(payload.trim_end_matches('=')).ok()?;
    let claims: Identity = serde_json::from_slice(&raw).ok()?;
    let clean = |c: Option<String>| c.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
    let id = Identity {
        email: clean(claims.email),
        preferred_username: clean(claims.preferred_username),
    };
    (id.email.is_some() || id.preferred_username.is_some()).then_some(id)
}

/// Decision 9: the typed address (trimmed, ASCII case-insensitive) equals
/// the `email` claim or `preferred_username`. Whole-string equality only.
pub fn identity_matches(typed: &str, id: &Identity) -> bool {
    let typed = typed.trim();
    let same = |c: &Option<String>| {
        c.as_deref()
            .is_some_and(|c| c.trim().eq_ignore_ascii_case(typed))
    };
    !typed.is_empty() && (same(&id.email) || same(&id.preferred_username))
}

#[cfg(test)]
mod tests;
