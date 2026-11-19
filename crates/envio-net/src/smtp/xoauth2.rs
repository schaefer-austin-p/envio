//! XOAUTH2 sign-in. lettre's mechanism cannot answer the server's 334
//! error challenge (it fails on the client side, and the server's 535 is
//! never read), so the exchange is done here with lettre's raw command:
//! the initial response, then an empty line for the challenge (RFC 7628
//! section 3.2.3), then the server's final reply.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use lettre::transport::smtp::client::AsyncSmtpConnection;

use super::map_error;
use crate::error::{NetError, Result};

/// The de facto XOAUTH2 initial response, base64.
fn initial_response(user: &str, token: &str) -> String {
    STANDARD.encode(format!("user={user}\x01auth=Bearer {token}\x01\x01"))
}

/// The server's own words in a 334 error challenge: base64 JSON, shown as
/// is when it decodes to text, else nothing.
fn challenge_text(message: &str) -> Option<String> {
    let bytes = STANDARD.decode(message.trim()).ok()?;
    let text = String::from_utf8(bytes).ok()?;
    (!text.trim().is_empty()).then_some(text)
}

pub(super) async fn sign_in(conn: &mut AsyncSmtpConnection, user: &str, token: &str) -> Result<()> {
    let line = format!("AUTH XOAUTH2 {}\r\n", initial_response(user, token));
    let reply = conn.command(line).await.map_err(|e| map_error(e, true))?;
    if !reply.has_code(334) {
        return Ok(());
    }
    // A refusal arrives as 334 + JSON; the 535 follows the empty answer and
    // comes back from `command` as an error carrying the server's text.
    let status = reply.message().next().and_then(challenge_text);
    let reply = conn.command("\r\n").await.map_err(|e| map_error(e, true))?;
    if reply.has_code(334) {
        return Err(NetError::SignIn(status.unwrap_or_default()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_initial_response_is_the_de_facto_format() {
        let expected = STANDARD.encode(b"user=a@b\x01auth=Bearer t\x01\x01");
        assert_eq!(initial_response("a@b", "t"), expected);
    }

    #[test]
    fn a_challenge_shows_its_json_or_nothing() {
        let json = STANDARD.encode(br#"{"status":"401"}"#);
        assert_eq!(challenge_text(&json).unwrap(), r#"{"status":"401"}"#);
        assert_eq!(challenge_text("!!not base64"), None);
        assert_eq!(challenge_text(""), None);
    }
}
