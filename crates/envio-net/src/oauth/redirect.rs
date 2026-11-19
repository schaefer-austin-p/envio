//! The authorize URL and the redirect that comes back.

use super::urlenc::{decode, form, safe_code};
use super::{Pkce, Provider};
use secrecy::SecretString;

/// A redirect longer than this is not ours.
const MAX_URL: usize = 8 * 1024;

pub struct Redirect {
    pub code: SecretString,
    pub state: String,
}

impl std::fmt::Debug for Redirect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Redirect").finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedirectError {
    /// The user declined (`access_denied`).
    Denied,
    /// The `state` is missing or is not this attempt's.
    Mismatch,
    /// Not our redirect, or no usable code.
    Malformed,
    /// The provider's `error` code (never its description).
    Server(String),
}

pub fn authorize_url(p: &Provider, pkce: &Pkce, login_hint: &str) -> String {
    let query = form(&[
        ("response_type", "code"),
        ("client_id", &p.client_id),
        ("redirect_uri", &p.redirect_uri),
        ("scope", p.scopes),
        ("state", &pkce.state),
        ("code_challenge", &pkce.challenge),
        ("code_challenge_method", "S256"),
        ("login_hint", login_hint),
    ]);
    format!("{}?{query}", p.authorize_url)
}

/// Equality that does not stop at the first differing byte (the length is
/// not secret). `ring`'s own helper is deprecated.
fn same_bytes(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[derive(Default)]
struct Params {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

fn params(query: &str) -> Result<Params, RedirectError> {
    let mut out = Params::default();
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let slot = match decode(k).ok_or(RedirectError::Malformed)?.as_str() {
            "code" => &mut out.code,
            "state" => &mut out.state,
            "error" => &mut out.error,
            _ => continue,
        };
        if slot
            .replace(decode(v).ok_or(RedirectError::Malformed)?)
            .is_some()
        {
            return Err(RedirectError::Malformed);
        }
    }
    Ok(out)
}

/// Check a redirect against this attempt: exact scheme and host, the
/// provider's `error` (only its code is kept), the `state` in constant time,
/// then the code.
pub fn parse_redirect(
    url: &str,
    expect_state: &str,
    redirect_uri: &str,
) -> Result<Redirect, RedirectError> {
    if url.len() > MAX_URL {
        return Err(RedirectError::Malformed);
    }
    let url = url.split('#').next().unwrap_or("");
    let query = url
        .strip_prefix(redirect_uri)
        .and_then(|rest| rest.strip_prefix('?'))
        .ok_or(RedirectError::Malformed)?;
    let p = params(query)?;
    if let Some(e) = p.error {
        return Err(if e == "access_denied" {
            RedirectError::Denied
        } else {
            RedirectError::Server(safe_code(&e))
        });
    }
    let state = p.state.unwrap_or_default();
    if !same_bytes(state.as_bytes(), expect_state.as_bytes()) {
        return Err(RedirectError::Mismatch);
    }
    match p.code {
        Some(code) if !code.is_empty() => Ok(Redirect {
            code: SecretString::from(code),
            state,
        }),
        _ => Err(RedirectError::Malformed),
    }
}

#[cfg(test)]
mod tests;
