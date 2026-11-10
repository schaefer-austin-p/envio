//! Sending mail: one SMTP session per Outbox run. TLS is set up by envio
//! (implicit, or STARTTLS in `starttls`), then lettre speaks SMTP over the
//! finished stream. The EHLO name is a fixed address literal, so mail
//! never carries the phone's host name.

mod reply;
mod starttls;
mod stream;

use std::net::Ipv4Addr;

use lettre::address::{Address, Envelope};
use lettre::transport::smtp::authentication::{Credentials as LettreCredentials, Mechanism};
use lettre::transport::smtp::client::AsyncSmtpConnection;
use lettre::transport::smtp::extension::ClientId;
use secrecy::ExposeSecret;

use crate::connect;
use crate::error::{NetError, Result};
use crate::server::{Credentials, Endpoint, Security};
use crate::tls::{self, Roots};
use stream::SmtpStream;

/// What envio says in EHLO: the same for every user.
pub const EHLO_NAME: &str = "[127.0.0.1]";

pub struct SmtpSession {
    conn: AsyncSmtpConnection,
}

impl SmtpSession {
    /// Connects, secures the connection and greets the server.
    pub async fn connect(endpoint: &Endpoint, roots: &Roots) -> Result<Self> {
        let stream = match endpoint.security {
            Security::Tls => SmtpStream::implicit(tls::connect(endpoint, roots).await?),
            Security::StartTls => {
                let tcp = connect::tcp(&endpoint.host, endpoint.port).await?;
                SmtpStream::upgraded(starttls::upgrade(tcp, endpoint, roots).await?)
            }
        };
        let hello = ClientId::Ipv4(Ipv4Addr::LOCALHOST);
        let conn = connect::within(async {
            AsyncSmtpConnection::connect_with_transport(Box::new(stream), &hello)
                .await
                .map_err(|e| map_error(e, false))
        })
        .await?;
        Ok(Self { conn })
    }

    /// Signs in with PLAIN, or LOGIN if that's all the server offers.
    pub async fn login(&mut self, credentials: &Credentials) -> Result<()> {
        // lettre keeps a plain String copy until this returns.
        let lettre = LettreCredentials::new(
            credentials.username.clone(),
            credentials.password.expose_secret().to_owned(),
        );
        connect::within(async {
            self.conn
                .auth(&[Mechanism::Plain, Mechanism::Login], &lettre)
                .await
                .map(drop)
                .map_err(|e| map_error(e, true))
        })
        .await
    }

    /// Sends one message. `eml` is the finished RFC 5322 message (from
    /// envio-mime); `from` and `to` are the envelope, which may differ
    /// from its headers (Bcc).
    pub async fn send(&mut self, from: &str, to: &[String], eml: &[u8]) -> Result<()> {
        let envelope = envelope(from, to)?;
        // The upload alone can outlast a command, so the limit grows with
        // the message.
        connect::within_for(connect::transfer_timeout(eml.len()), async {
            self.conn
                .send(&envelope, eml)
                .await
                .map(drop)
                .map_err(|e| map_error(e, false))
        })
        .await
    }

    pub async fn quit(mut self) -> Result<()> {
        connect::within(async {
            self.conn
                .quit()
                .await
                .map(drop)
                .map_err(|e| map_error(e, false))
        })
        .await
    }
}

fn envelope(from: &str, to: &[String]) -> Result<Envelope> {
    let address = |text: &str| {
        text.parse::<Address>()
            .map_err(|_| NetError::Refused(format!("not an address: {text:?}")))
    };
    let recipients = to.iter().map(|t| address(t)).collect::<Result<Vec<_>>>()?;
    Envelope::new(Some(address(from)?), recipients)
        .map_err(|e| NetError::Refused(format!("bad envelope: {e}")))
}

fn map_error(error: lettre::transport::smtp::Error, signing_in: bool) -> NetError {
    if let Some(code) = error.status() {
        let code = code.to_string().parse().unwrap_or(0);
        return reply::error_for(code, error.to_string(), signing_in);
    }
    if error.is_timeout() {
        return NetError::Timeout;
    }
    if error.is_client() || error.is_response() {
        return NetError::Protocol(error.to_string());
    }
    NetError::Io(std::io::Error::new(
        std::io::ErrorKind::ConnectionAborted,
        error.to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_envelope_needs_real_addresses() {
        let to = vec!["bob@example.org".to_owned()];
        assert!(envelope("ada@example.org", &to).is_ok());
        assert!(matches!(
            envelope("not an address", &to),
            Err(NetError::Refused(_))
        ));
        assert!(matches!(
            envelope("ada@example.org", &[]),
            Err(NetError::Refused(_))
        ));
    }
}
