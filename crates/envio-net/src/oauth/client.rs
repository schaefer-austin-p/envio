//! The thin HTTPS layer of OAuth: one form POST to the provider's token
//! endpoint over HTTP/1.0 on `tls::connect`. The host and path come from
//! the `Provider` only. No redirect is followed (a 3xx is a refusal), the
//! body is capped at `http::MAX_BODY`, and the whole call, connect
//! included, runs under `COMMAND_TIMEOUT`. Nothing here logs, and the
//! request body (which holds the secrets) is never put in an error.

use std::io;
use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};
use tokio::io::AsyncWriteExt;

use super::{Grant, Provider, TokenError, Tokens, parse_token_reply, token_request_body};
use crate::COMMAND_TIMEOUT;
use crate::connect;
use crate::error::{NetError, Result};
use crate::http::{self, Url};
use crate::server::{Endpoint, Security};
use crate::tls::{self, Roots};

/// Trades the authorization `code` (and its PKCE `verifier`) for tokens.
pub async fn exchange_code(
    p: &Provider,
    roots: &Roots,
    code: &SecretString,
    verifier: &SecretString,
) -> std::result::Result<Tokens, TokenError> {
    let grant = Grant::Code {
        code: code.expose_secret(),
        verifier: verifier.expose_secret(),
    };
    post(p, roots, &grant, COMMAND_TIMEOUT).await
}

/// Trades a refresh token for new tokens.
pub async fn refresh(
    p: &Provider,
    roots: &Roots,
    refresh_token: &SecretString,
) -> std::result::Result<Tokens, TokenError> {
    refresh_within(p, roots, refresh_token, COMMAND_TIMEOUT).await
}

/// [`refresh`] with its own time limit, so a test need not wait a minute.
#[doc(hidden)]
pub async fn refresh_within(
    p: &Provider,
    roots: &Roots,
    refresh_token: &SecretString,
    limit: Duration,
) -> std::result::Result<Tokens, TokenError> {
    let grant = Grant::Refresh {
        token: refresh_token.expose_secret(),
    };
    post(p, roots, &grant, limit).await
}

async fn post(
    p: &Provider,
    roots: &Roots,
    grant: &Grant<'_>,
    limit: Duration,
) -> std::result::Result<Tokens, TokenError> {
    let body = token_request_body(p, grant);
    let (status, reply) = connect::within_for(limit, round_trip(p, roots, &body))
        .await
        .map_err(|e| {
            if e.is_temporary() {
                TokenError::Temporary
            } else {
                // Fixed text: nothing from the request or the error.
                TokenError::Refused("connection".to_owned())
            }
        })?;
    parse_token_reply(status, &reply)
}

async fn round_trip(p: &Provider, roots: &Roots, body: &SecretString) -> Result<(u16, Vec<u8>)> {
    let url = http::parse_url(&p.token_url)
        .ok_or_else(|| NetError::Refused("token URL is not https".into()))?;
    // No pin: the token host must have a certificate the roots trust.
    let endpoint = Endpoint::new(&url.host, url.port, Security::Tls);
    let mut stream = tls::connect(&endpoint, roots).await?;
    stream.write_all(request(&url, body).as_bytes()).await?;
    let (raw, clean) = http::read_reply(&mut stream).await?;
    // A reply that is cut short, chunked or not HTTP is a bad connection:
    // retry, as for a dropped socket.
    http::parse_response(&raw, clean)
        .ok_or_else(|| NetError::Io(io::Error::from(io::ErrorKind::UnexpectedEof)))
}

/// The POST, with the form as its body.
fn request(url: &Url, body: &SecretString) -> String {
    let body = body.expose_secret();
    let host = if url.port == 443 {
        url.host.clone()
    } else {
        format!("{}:{}", url.host, url.port)
    };
    format!(
        "POST {} HTTP/1.0\r\nHost: {host}\r\nAccept: application/json\r\n\
         Content-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n{body}",
        url.path,
        body.len()
    )
}
