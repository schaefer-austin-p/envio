//! Why an account is about to pause: one log line naming the stage that
//! failed (token refresh, a missing secret, IMAP or SMTP sign-in) and what
//! the server or the token endpoint said. The `Problem` that reaches health
//! carries none of this, so the lines are written where the error is still
//! known. Never holds a token, a code, a password, a SASL string or an
//! address: server text goes through `mask::server_text`, and the token
//! endpoint's `error` is already a plain code.

use envio_net::NetError;
use envio_net::oauth::TokenError;

use crate::mask;

pub(crate) const TOKEN_REFRESH: &str = "OAuth token refresh";
pub(crate) const TOKEN_MISSING: &str = "refresh token missing from the secret store";
pub(crate) const PASSWORD_MISSING: &str = "password missing from the secret store";
pub(crate) const NO_PROVIDER: &str = "no OAuth provider configured";
pub(crate) const IMAP_XOAUTH2: &str = "IMAP AUTHENTICATE XOAUTH2";
pub(crate) const IMAP_LOGIN: &str = "IMAP LOGIN";
pub(crate) const SMTP_XOAUTH2: &str = "SMTP AUTH XOAUTH2";
pub(crate) const SMTP_LOGIN: &str = "SMTP AUTH";

/// The line for a token endpoint failure that pauses the account.
pub(crate) fn token_line(error: &TokenError) -> String {
    let said = match error {
        TokenError::Revoked => "grant revoked or sign-in needed (invalid_grant and kin)".to_owned(),
        TokenError::Temporary => "temporary".to_owned(),
        TokenError::Refused(code) => format!("refused, error={}", mask::server_text(code)),
    };
    format!("pause cause: {TOKEN_REFRESH}: {said}")
}

/// The line for a network error at `stage`, for the kinds that pause an
/// account (sign-in and certificate); `None` for the rest.
pub(crate) fn net_line(stage: &str, error: &NetError) -> Option<String> {
    let kind = match error {
        NetError::SignIn(_) => "sign-in refused",
        NetError::Untrusted { .. }
        | NetError::PinChanged { .. }
        | NetError::Tls(_)
        | NetError::NoStartTls => "certificate or TLS",
        _ => return None,
    };
    Some(format!(
        "pause cause: {stage}: {kind}: {}",
        mask::server_text(&error.to_string())
    ))
}

pub(crate) fn log_net(stage: &str, error: &NetError) {
    if let Some(line) = net_line(stage, error) {
        log::warn!("{line}");
    }
}

pub(crate) fn log_token(error: &TokenError) {
    if !matches!(error, TokenError::Temporary) {
        log::warn!("{}", token_line(error));
    }
}

pub(crate) fn log_stage(stage: &str) {
    log::warn!("pause cause: {stage}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_token_refresh_names_the_stage_and_the_error_code() {
        let line = token_line(&TokenError::Refused("invalid_scope".into()));
        assert!(line.contains(TOKEN_REFRESH), "{line}");
        assert!(line.contains("error=invalid_scope"), "{line}");
        assert!(token_line(&TokenError::Revoked).contains("revoked"));
    }

    #[test]
    fn an_imap_refusal_names_the_stage_and_keeps_no_secret() {
        let secret = "EwBwA8l6BAAUbDba3x2OMJElkF7gJ4z/VbCPEz0AAfj0fmHWqWbTB2xt";
        let error = NetError::SignIn(format!(
            "code: None, info: Some(\"AUTHENTICATE failed for bob@outlook.com {secret}\")"
        ));
        let line = net_line(IMAP_XOAUTH2, &error).unwrap();
        assert!(line.contains("IMAP AUTHENTICATE XOAUTH2"), "{line}");
        assert!(line.contains("AUTHENTICATE failed"), "{line}");
        assert!(!line.contains(secret), "{line}");
        assert!(!line.contains("bob@"), "{line}");
        assert!(line.contains("b…@outlook.com"), "{line}");
    }

    #[test]
    fn only_pausing_kinds_get_a_line() {
        assert!(net_line(SMTP_XOAUTH2, &NetError::Timeout).is_none());
        assert!(net_line(SMTP_XOAUTH2, &NetError::Refused("quota".into())).is_none());
        let tls = net_line(IMAP_LOGIN, &NetError::NoStartTls).unwrap();
        assert!(tls.contains("certificate"), "{tls}");
    }
}
