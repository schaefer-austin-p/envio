//! Opening an IMAP connection: TLS (implicit or STARTTLS), the greeting,
//! LOGIN; and what async-imap's errors mean for envio.

use async_imap::imap_proto::{Outcome, RequestId, Response, Status};

/// How the server greeted us.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Greeting {
    Ok,
    PreAuth,
    Bye,
    Other,
}
use async_imap::{Client, Session};
use secrecy::ExposeSecret;
use tokio::net::TcpStream;

use crate::connect;
use crate::error::{NetError, Result};
use crate::server::{Credentials, Endpoint, Security};
use crate::tls::{self, Roots, TlsStream};

pub(crate) type Stream = TlsStream<TcpStream>;

pub(crate) async fn open(
    endpoint: &Endpoint,
    roots: &Roots,
    credentials: &Credentials,
) -> Result<Session<Stream>> {
    let client = match endpoint.security {
        Security::Tls => {
            let mut client = Client::new(tls::connect(endpoint, roots).await?);
            connect::within(greeting(&mut client)).await?;
            client
        }
        Security::StartTls => {
            let tcp = connect::tcp(&endpoint.host, endpoint.port).await?;
            let tcp = connect::within(starttls(tcp)).await?;
            // No greeting after STARTTLS (RFC 3501 6.2.1).
            Client::new(tls::handshake(tcp, &endpoint.host, roots, endpoint.pin).await?)
        }
    };
    // async-imap refuses CR and LF here and sends both as quoted strings.
    let password = credentials.password.expose_secret();
    connect::within(async move {
        client
            .login(&credentials.username, password)
            .await
            .map_err(|(error, _)| match error {
                // A "not now" NO (RFC 5530) is not a wrong password.
                async_imap::error::Error::No(text) if !is_busy(&text) => NetError::SignIn(text),
                other => map_error(other),
            })
    })
    .await
}

/// The plaintext prelude: greeting, then STARTTLS. Nothing else is sent
/// in the clear. Bytes the server sent after its OK are dropped with the
/// buffer, never read as if they came over TLS.
async fn starttls(tcp: TcpStream) -> Result<TcpStream> {
    let mut client = Client::new(tcp);
    let status = greeting_status(&mut client).await?;
    if status == Greeting::PreAuth {
        // Already signed in, so STARTTLS is not allowed: refuse.
        return Err(NetError::NoStartTls);
    }
    check_greeting(status)?;
    match client.run_command_and_check_ok("STARTTLS", None).await {
        Ok(()) => Ok(client.into_inner()),
        Err(async_imap::error::Error::No(_) | async_imap::error::Error::Bad(_)) => {
            Err(NetError::NoStartTls)
        }
        Err(other) => Err(map_error(other)),
    }
}

async fn greeting<S>(client: &mut Client<S>) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + std::fmt::Debug + Send,
{
    check_greeting(greeting_status(client).await?)
}

async fn greeting_status<S>(client: &mut Client<S>) -> Result<Greeting>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + std::fmt::Debug + Send,
{
    let response = client
        .read_response()
        .await?
        .ok_or(NetError::Io(std::io::ErrorKind::UnexpectedEof.into()))?;
    match response.parsed() {
        Response::Data { status, .. } => Ok(match status {
            Status::Ok => Greeting::Ok,
            Status::PreAuth => Greeting::PreAuth,
            Status::Bye => Greeting::Bye,
            _ => Greeting::Other,
        }),
        other => Err(NetError::Protocol(format!(
            "not an IMAP greeting: {other:?}"
        ))),
    }
}

/// OK goes on; BYE is "busy, try later"; anything else is not IMAP.
pub(crate) fn check_greeting(status: Greeting) -> Result<()> {
    match status {
        Greeting::Ok => Ok(()),
        Greeting::Bye => Err(NetError::Busy("server said BYE".into())),
        other => Err(NetError::Protocol(format!("greeting {other:?}"))),
    }
}

pub(crate) fn map_error(error: async_imap::error::Error) -> NetError {
    use async_imap::error::Error;
    match error {
        Error::Io(e) if e.kind() == std::io::ErrorKind::TimedOut => NetError::Timeout,
        Error::Io(e) => NetError::Io(e),
        Error::ConnectionLost => NetError::Io(std::io::ErrorKind::UnexpectedEof.into()),
        Error::No(text) if is_busy(&text) => NetError::Busy(text),
        Error::No(text) => NetError::Refused(text),
        Error::Validate(e) => NetError::Refused(e.to_string()),
        Error::Append => NetError::Refused("APPEND failed".into()),
        other => NetError::Protocol(other.to_string()),
    }
}

/// RFC 5530 codes for "not now". Response codes are case-insensitive.
fn is_busy(text: &str) -> bool {
    let text = text.to_ascii_uppercase();
    ["[UNAVAILABLE]", "[INUSE]", "[LIMIT]"]
        .iter()
        .any(|code| text.contains(code))
}

/// The server's NO or BAD, as the error async-imap's own commands raise
/// for it (so LOGIN, APPEND and the rest map alike).
fn failure_of(status: &Status, outcome: &Outcome<'_>) -> Option<NetError> {
    let text = format!(
        "code: {:?}, info: {:?}",
        outcome.code,
        outcome.information.as_deref()
    );
    match status {
        Status::No => Some(map_error(async_imap::error::Error::No(text))),
        Status::Bad => Some(map_error(async_imap::error::Error::Bad(text))),
        _ => None,
    }
}

/// How a command with a literal ends: the tagged status, or a NO/BAD.
/// `None` means "keep reading" (untagged data).
pub(crate) fn command_end(response: &Response<'_>, id: &RequestId) -> Option<Result<()>> {
    match response {
        Response::Done {
            tag,
            status,
            outcome,
        } if tag == id => Some(match failure_of(status, outcome) {
            Some(error) => Err(error),
            None => Ok(()),
        }),
        Response::Data { status, outcome } => failure_of(status, outcome).map(Err),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_imap::error::Error;

    #[test]
    fn greetings_other_than_ok_stop_the_connection() {
        assert!(check_greeting(Greeting::Ok).is_ok());
        assert!(matches!(
            check_greeting(Greeting::Bye),
            Err(NetError::Busy(_))
        ));
        assert!(matches!(
            check_greeting(Greeting::Other),
            Err(NetError::Protocol(_))
        ));
    }

    #[test]
    fn imap_errors_become_error_kinds() {
        assert!(map_error(Error::ConnectionLost).is_temporary());
        assert!(matches!(
            map_error(Error::No("[OVERQUOTA] full".into())),
            NetError::Refused(_)
        ));
        assert!(matches!(
            map_error(Error::No("[UNAVAILABLE] later".into())),
            NetError::Busy(_)
        ));
        assert!(matches!(
            map_error(Error::Bad("eh".into())),
            NetError::Protocol(_)
        ));
    }

    fn end_of(line: &[u8]) -> Option<Result<()>> {
        let (_, response) = Response::parse(line).unwrap();
        command_end(&response, &RequestId("A1".into()))
    }

    #[test]
    fn append_replies_with_busy_codes_are_busy() {
        for line in [
            &b"A1 NO [UNAVAILABLE] try later\r\n"[..],
            b"A1 NO [inuse] mailbox busy\r\n",
            b"* NO [Limit] too many\r\n",
        ] {
            assert!(
                matches!(end_of(line), Some(Err(NetError::Busy(_)))),
                "{line:?}"
            );
        }
        assert!(matches!(
            end_of(b"A1 NO [OVERQUOTA] full\r\n"),
            Some(Err(NetError::Refused(_)))
        ));
        assert!(matches!(end_of(b"A1 OK done\r\n"), Some(Ok(()))));
        assert!(end_of(b"* 3 EXISTS\r\n").is_none());
        assert!(end_of(b"A2 NO [UNAVAILABLE] other\r\n").is_none());
    }

    #[test]
    fn busy_codes_match_in_any_case() {
        assert!(is_busy("[unavailable] x"));
        assert!(is_busy("code: None, info: Some(\"[InUse] x\")"));
        assert!(!is_busy("[OVERQUOTA]"));
    }
}
