//! TLS client setup: which roots to trust, the handshake, and turning a
//! rejected certificate into `NetError::Untrusted` or `PinChanged`.

use std::fmt;
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, RootCertStore};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use crate::connect;
use crate::error::{NetError, Result};
use crate::server::{Endpoint, Fingerprint, Security};
use crate::trust::Verdict;
use crate::verifier::{self, PinningVerifier};

pub use tokio_rustls::client::TlsStream;

/// The certificate authorities an unpinned server must chain to.
#[derive(Clone)]
pub struct Roots {
    store: Arc<RootCertStore>,
    /// Why some or all of the system bundle did not load. Empty when
    /// everything did, and for `from_der` and `empty`.
    problems: Arc<[String]>,
}

impl fmt::Debug for Roots {
    /// Only the count: the CA certificates are not secrets, but a dump of
    /// the whole system bundle in a log is noise.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Roots")
            .field("len", &self.len())
            .field("load_errors", &self.problems.len())
            .finish()
    }
}

impl Roots {
    /// The system's CA bundle. Certificates that fail to load are skipped;
    /// if none load (say, AppArmor hides the bundle), every server goes to
    /// the certificate prompt rather than failing outright. The reasons
    /// are kept: see [`Roots::load_errors`].
    pub fn system() -> Self {
        let loaded = rustls_native_certs::load_native_certs();
        let mut store = RootCertStore::empty();
        let (_, ignored) = store.add_parsable_certificates(loaded.certs);
        let errors: Vec<String> = loaded.errors.iter().map(ToString::to_string).collect();
        Self {
            store: Arc::new(store),
            problems: load_problems(errors, ignored).into(),
        }
    }

    /// What went wrong loading the system bundle, as text for the log;
    /// empty if nothing did. Without this, a hidden bundle (AppArmor)
    /// would only show up as every server asking for a certificate prompt.
    pub fn load_errors(&self) -> &[String] {
        &self.problems
    }

    /// Specific CAs, as DER. For tests and private CAs.
    pub fn from_der(certs: impl IntoIterator<Item = Vec<u8>>) -> Self {
        let mut store = RootCertStore::empty();
        store.add_parsable_certificates(certs.into_iter().map(CertificateDer::from));
        Self::loaded(store)
    }

    /// No CAs: only pinned servers connect.
    pub fn empty() -> Self {
        Self::loaded(RootCertStore::empty())
    }

    fn loaded(store: RootCertStore) -> Self {
        Self {
            store: Arc::new(store),
            problems: Arc::new([]),
        }
    }

    pub fn len(&self) -> usize {
        self.store.len()
    }

    pub fn is_empty(&self) -> bool {
        self.store.is_empty()
    }
}

/// The load problems as text: each error, then the count of certificates
/// that were read but could not be parsed.
fn load_problems(errors: Vec<String>, ignored: usize) -> Vec<String> {
    let mut problems = errors;
    if ignored > 0 {
        problems.push(format!(
            "{ignored} system certificate(s) could not be parsed"
        ));
    }
    problems
}

/// Runs the TLS handshake on an open stream (a fresh TCP connection, or
/// one that just agreed to STARTTLS).
pub async fn handshake<S>(
    stream: S,
    host: &str,
    roots: &Roots,
    pin: Option<Fingerprint>,
) -> Result<TlsStream<S>>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = PinningVerifier::new(pin, roots.store.clone(), provider.clone());
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| NetError::Tls(e.to_string()))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    let name = ServerName::try_from(host.to_owned())
        .map_err(|_| NetError::Tls(format!("not a host name: {host}")))?;
    let connector = TlsConnector::from(Arc::new(config));
    connect::within(async {
        connector
            .connect(name, stream)
            .await
            .map_err(handshake_error)
    })
    .await
}

/// Opens TCP and does the handshake straight away: [`Security::Tls`].
/// STARTTLS is protocol-specific and lives with IMAP and SMTP.
pub async fn connect(endpoint: &Endpoint, roots: &Roots) -> Result<TlsStream<TcpStream>> {
    if endpoint.security != Security::Tls {
        return Err(NetError::Protocol("tls::connect needs implicit TLS".into()));
    }
    let tcp = connect::tcp(&endpoint.host, endpoint.port).await?;
    handshake(tcp, &endpoint.host, roots, endpoint.pin).await
}

fn handshake_error(error: std::io::Error) -> NetError {
    match verifier::rejection(&error) {
        Some(rejected) => match rejected.verdict {
            Verdict::Changed { pinned } => NetError::PinChanged {
                pinned,
                presented: rejected.presented,
            },
            Verdict::Untrusted(reason) => NetError::Untrusted {
                presented: rejected.presented,
                reason,
            },
            Verdict::Accept => NetError::Tls("accepted certificate rejected".into()),
        },
        None if error.kind() == std::io::ErrorKind::InvalidData => NetError::Tls(error.to_string()),
        None => NetError::Io(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_problems_keep_the_errors_and_count_the_ignored() {
        assert!(load_problems(vec![], 0).is_empty());
        let problems = load_problems(vec!["permission denied".into()], 2);
        assert_eq!(problems.len(), 2);
        assert_eq!(problems[0], "permission denied");
        assert!(problems[1].starts_with("2 "), "{problems:?}");
        assert_eq!(load_problems(vec![], 1).len(), 1);
    }

    #[test]
    fn roots_without_a_system_load_have_no_problems() {
        assert!(Roots::empty().load_errors().is_empty());
        assert!(Roots::from_der([vec![1, 2, 3]]).load_errors().is_empty());
    }
}
