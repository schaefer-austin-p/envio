//! Signing in: LOGIN for ASCII credentials, AUTHENTICATE PLAIN (RFC 4616)
//! for the rest, AUTHENTICATE XOAUTH2 for an OAuth access token. LOGIN sends quoted strings, which are 7-bit in IMAP4rev1,
//! so a password like `pässwort` would be refused or reach the server in
//! some other encoding. PLAIN carries UTF-8 in base64.

use async_imap::imap_proto::{Capability, Response};
use async_imap::{Authenticator, Client, Session};
use secrecy::ExposeSecret;
use tokio::io::AsyncWriteExt;

use super::client::{Stream, is_busy, map_error};
use crate::error::{NetError, Result};
use crate::server::Credentials;

/// How to sign in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Method {
    Login,
    Plain,
    Xoauth2,
    /// Credentials LOGIN cannot carry and the server has no PLAIN for.
    Refuse(&'static str),
    /// A bearer credential and a server without XOAUTH2: the server cannot
    /// do this, which is not an expired or revoked token.
    Unsupported(&'static str),
}

/// The choice, from the credentials and what the server offers. The
/// capability list is only needed (and only read) when `ascii` is false.
pub(crate) fn choose(ascii: bool, has_nul: bool, auth_plain: bool) -> Method {
    // NUL fits neither a quoted string nor PLAIN's separators.
    if has_nul {
        Method::Refuse("the user name or password contains a NUL character")
    } else if ascii {
        Method::Login
    } else if auth_plain {
        Method::Plain
    } else {
        Method::Refuse(
            "the user name or password is not ASCII and the server does not offer AUTH=PLAIN",
        )
    }
}

/// A bearer credential always uses XOAUTH2, never LOGIN or PLAIN.
pub(crate) fn choose_bearer(offers_xoauth2: bool) -> Method {
    if offers_xoauth2 {
        Method::Xoauth2
    } else {
        Method::Unsupported("the server does not offer AUTH=XOAUTH2")
    }
}

/// The RFC 4616 initial response: `\0username\0password`, UTF-8.
fn plain_response(username: &str, password: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(username.len() + password.len() + 2);
    out.push(0);
    out.extend_from_slice(username.as_bytes());
    out.push(0);
    out.extend_from_slice(password.as_bytes());
    out
}

/// The de facto XOAUTH2 initial response (not base64; async-imap encodes):
/// `user=` u `\x01auth=Bearer ` t `\x01\x01`.
fn xoauth2_response(username: &str, token: &str) -> Vec<u8> {
    format!("user={username}\x01auth=Bearer {token}\x01\x01").into_bytes()
}

/// Sends the response once; any later challenge is the server's failure
/// report (`+ <base64 json>`), which RFC-style gets an empty answer so the
/// tagged NO follows. Deliberately no `Debug`: it holds the token.
struct Xoauth2<'a> {
    username: &'a str,
    token: &'a str,
    sent: bool,
}

impl Authenticator for Xoauth2<'_> {
    type Response = Vec<u8>;

    fn process(&mut self, _challenge: &[u8]) -> Vec<u8> {
        if std::mem::replace(&mut self.sent, true) {
            Vec::new()
        } else {
            xoauth2_response(self.username, self.token)
        }
    }
}

/// Answers the server's (empty) challenge. Deliberately no `Debug`: it
/// holds the password.
struct Plain<'a> {
    username: &'a str,
    password: &'a str,
}

impl Authenticator for Plain<'_> {
    type Response = Vec<u8>;

    fn process(&mut self, _challenge: &[u8]) -> Vec<u8> {
        plain_response(self.username, self.password)
    }
}

pub(crate) async fn run(
    mut client: Client<Stream>,
    credentials: &Credentials,
) -> Result<Session<Stream>> {
    let username = credentials.username.as_str();
    let password = credentials.password.expose_secret();
    let method = if credentials.is_bearer() {
        choose_bearer(offers(&mut client, "XOAUTH2").await?)
    } else {
        let ascii = username.is_ascii() && password.is_ascii();
        let has_nul = username.contains('\0') || password.contains('\0');
        // Only the unusual path needs the list, so ASCII sign-in is unchanged.
        let auth_plain = if ascii {
            false
        } else {
            offers(&mut client, "PLAIN").await?
        };
        choose(ascii, has_nul, auth_plain)
    };
    match method {
        // async-imap refuses CR and LF here and sends both as quoted strings.
        Method::Login => client.login(username, password).await,
        Method::Plain => {
            client
                .authenticate("PLAIN", Plain { username, password })
                .await
        }
        Method::Xoauth2 => {
            client
                .authenticate(
                    "XOAUTH2",
                    Xoauth2 {
                        username,
                        token: password,
                        sent: false,
                    },
                )
                .await
        }
        Method::Refuse(reason) => return Err(NetError::SignIn(reason.into())),
        Method::Unsupported(reason) => return Err(NetError::Refused(reason.into())),
    }
    .map_err(|(error, _)| match error {
        // A "not now" NO (RFC 5530) is not a wrong password.
        async_imap::error::Error::No(text) if !is_busy(&text) => NetError::SignIn(text),
        other => map_error(other),
    })
}

/// Asks CAPABILITY before sign-in and looks for `AUTH=<mechanism>`.
async fn offers(client: &mut Client<Stream>, mechanism: &str) -> Result<bool> {
    // `Client` has no public way to send a command and read the untagged
    // data, so write the line on the stream; the tag cannot collide with
    // async-imap's own (`A0001`...).
    const TAG: &str = "CAP1";
    let stream = client.get_mut();
    stream
        .write_all(format!("{TAG} CAPABILITY\r\n").as_bytes())
        .await?;
    stream.flush().await?;
    let mut found = false;
    loop {
        let response = client
            .read_response()
            .await?
            .ok_or(NetError::Io(std::io::ErrorKind::UnexpectedEof.into()))?;
        match response.parsed() {
            Response::Capabilities(caps) => {
                found |= caps
                    .iter()
                    .any(|c| matches!(c, Capability::Auth(s) if s.eq_ignore_ascii_case(mechanism)));
            }
            Response::Done { tag, .. } if tag.0 == TAG => return Ok(found),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_credentials_use_login() {
        assert_eq!(choose(true, false, false), Method::Login);
        assert_eq!(choose(true, false, true), Method::Login);
    }

    #[test]
    fn non_ascii_credentials_use_plain_when_offered() {
        assert_eq!(choose(false, false, true), Method::Plain);
    }

    #[test]
    fn non_ascii_credentials_without_plain_are_refused() {
        assert!(matches!(choose(false, false, false), Method::Refuse(_)));
        assert!(matches!(choose(false, true, true), Method::Refuse(_)));
    }

    #[test]
    fn a_nul_is_refused_even_in_ascii_credentials() {
        assert!(matches!(choose(true, true, false), Method::Refuse(_)));
        assert!(matches!(choose(true, true, true), Method::Refuse(_)));
    }

    #[test]
    fn the_xoauth2_response_is_user_bearer_token_with_control_a() {
        assert_eq!(
            xoauth2_response("a@outlook.example", "tok"),
            b"user=a@outlook.example\x01auth=Bearer tok\x01\x01"
        );
    }

    #[test]
    fn a_bearer_credential_needs_xoauth2_from_the_server() {
        assert_eq!(choose_bearer(true), Method::Xoauth2);
        assert!(matches!(choose_bearer(false), Method::Unsupported(_)));
    }

    #[test]
    fn the_plain_response_is_nul_user_nul_password_in_utf8() {
        assert_eq!(
            plain_response("alice", "pässwort"),
            b"\0alice\0p\xc3\xa4sswort"
        );
    }
}
