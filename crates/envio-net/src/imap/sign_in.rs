//! Signing in: LOGIN for ASCII credentials, AUTHENTICATE PLAIN (RFC 4616)
//! for the rest. LOGIN sends quoted strings, which are 7-bit in IMAP4rev1,
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
    /// Credentials LOGIN cannot carry and the server has no PLAIN for.
    Refuse(&'static str),
}

/// The choice, from the credentials and what the server offers. The
/// capability list is only needed (and only read) when `ascii` is false.
pub(crate) fn choose(ascii: bool, has_nul: bool, auth_plain: bool) -> Method {
    if ascii {
        Method::Login
    } else if has_nul {
        Method::Refuse("the user name or password contains a NUL character")
    } else if auth_plain {
        Method::Plain
    } else {
        Method::Refuse(
            "the user name or password is not ASCII and the server does not offer AUTH=PLAIN",
        )
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
    let ascii = username.is_ascii() && password.is_ascii();
    let has_nul = username.contains('\0') || password.contains('\0');
    // Only the unusual path needs the list, so ASCII sign-in is unchanged.
    let auth_plain = if ascii {
        false
    } else {
        offers_plain(&mut client).await?
    };
    match choose(ascii, has_nul, auth_plain) {
        // async-imap refuses CR and LF here and sends both as quoted strings.
        Method::Login => client.login(username, password).await,
        Method::Plain => {
            client
                .authenticate("PLAIN", Plain { username, password })
                .await
        }
        Method::Refuse(reason) => return Err(NetError::SignIn(reason.into())),
    }
    .map_err(|(error, _)| match error {
        // A "not now" NO (RFC 5530) is not a wrong password.
        async_imap::error::Error::No(text) if !is_busy(&text) => NetError::SignIn(text),
        other => map_error(other),
    })
}

/// Asks CAPABILITY before sign-in and looks for AUTH=PLAIN.
async fn offers_plain(client: &mut Client<Stream>) -> Result<bool> {
    // `Client` has no public way to send a command and read the untagged
    // data, so write the line on the stream; the tag cannot collide with
    // async-imap's own (`A0001`...).
    const TAG: &str = "CAP1";
    let stream = client.get_mut();
    stream
        .write_all(format!("{TAG} CAPABILITY\r\n").as_bytes())
        .await?;
    stream.flush().await?;
    let mut plain = false;
    loop {
        let response = client
            .read_response()
            .await?
            .ok_or(NetError::Io(std::io::ErrorKind::UnexpectedEof.into()))?;
        match response.parsed() {
            Response::Capabilities(caps) => {
                plain |= caps
                    .iter()
                    .any(|c| matches!(c, Capability::Auth(s) if s.eq_ignore_ascii_case("PLAIN")));
            }
            Response::Done { tag, .. } if tag.0 == TAG => return Ok(plain),
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
    fn the_plain_response_is_nul_user_nul_password_in_utf8() {
        assert_eq!(
            plain_response("alice", "pässwort"),
            b"\0alice\0p\xc3\xa4sswort"
        );
    }
}
