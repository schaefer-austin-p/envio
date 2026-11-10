//! The rustls side of [`crate::trust`]: a certificate verifier that hashes
//! the server's certificate, asks `trust::decide`, and turns a "no" into a
//! handshake error that carries the fingerprint back to the caller.

use std::error::Error as StdError;
use std::fmt;
use std::sync::Arc;

use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, WebPkiSupportedAlgorithms};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{CertificateError, DigitallySignedStruct, Error, OtherError, SignatureScheme};

use crate::error::Untrusted;
use crate::server::Fingerprint;
use crate::trust::{self, Verdict};

/// Why the verifier said no; rides inside the rustls error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rejected {
    pub presented: Fingerprint,
    pub verdict: Verdict,
}

impl fmt::Display for Rejected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "certificate rejected: {:?}", self.verdict)
    }
}

impl StdError for Rejected {}

#[derive(Debug)]
pub(crate) struct PinningVerifier {
    pin: Option<Fingerprint>,
    /// None when the system has no roots at all: then every unpinned
    /// server goes to the prompt instead of failing to build a config.
    webpki: Option<Arc<WebPkiServerVerifier>>,
    algorithms: WebPkiSupportedAlgorithms,
}

impl PinningVerifier {
    pub fn new(
        pin: Option<Fingerprint>,
        roots: Arc<rustls::RootCertStore>,
        provider: Arc<CryptoProvider>,
    ) -> Self {
        let algorithms = provider.signature_verification_algorithms;
        let webpki = WebPkiServerVerifier::builder_with_provider(roots, provider)
            .build()
            .ok();
        Self {
            pin,
            webpki,
            algorithms,
        }
    }
}

impl ServerCertVerifier for PinningVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        let presented = Fingerprint::of(end_entity.as_ref());
        let system = || match &self.webpki {
            None => Err(Untrusted::UnknownIssuer),
            Some(webpki) => webpki
                .verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now)
                .map(|_| ())
                .map_err(|e| reason_of(&e)),
        };
        match trust::decide(self.pin.as_ref(), &presented, system) {
            Verdict::Accept => Ok(ServerCertVerified::assertion()),
            verdict => Err(Error::InvalidCertificate(CertificateError::Other(
                OtherError(Arc::new(Rejected { presented, verdict })),
            ))),
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

/// The prompt's reason for a webpki failure.
pub(crate) fn reason_of(error: &Error) -> Untrusted {
    match error {
        Error::InvalidCertificate(cert) => match cert {
            CertificateError::UnknownIssuer => Untrusted::UnknownIssuer,
            CertificateError::Expired | CertificateError::ExpiredContext { .. } => {
                Untrusted::Expired
            }
            CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. } => {
                Untrusted::WrongName
            }
            _ => Untrusted::Other,
        },
        _ => Untrusted::Other,
    }
}

/// Finds a [`Rejected`] inside a failed handshake's error.
pub(crate) fn rejection(error: &std::io::Error) -> Option<Rejected> {
    let tls = error.get_ref()?.downcast_ref::<Error>()?;
    match tls {
        Error::InvalidCertificate(CertificateError::Other(other)) => {
            other.0.downcast_ref::<Rejected>().copied()
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webpki_failures_become_prompt_reasons() {
        let cert = |e| Error::InvalidCertificate(e);
        assert_eq!(
            reason_of(&cert(CertificateError::UnknownIssuer)),
            Untrusted::UnknownIssuer
        );
        assert_eq!(
            reason_of(&cert(CertificateError::Expired)),
            Untrusted::Expired
        );
        assert_eq!(
            reason_of(&cert(CertificateError::NotValidForName)),
            Untrusted::WrongName
        );
        assert_eq!(
            reason_of(&cert(CertificateError::BadEncoding)),
            Untrusted::Other
        );
        assert_eq!(reason_of(&Error::DecryptError), Untrusted::Other);
    }

    #[test]
    fn a_rejection_survives_the_trip_through_io_error() {
        let rejected = Rejected {
            presented: Fingerprint::from_bytes([7; 32]),
            verdict: Verdict::Untrusted(Untrusted::UnknownIssuer),
        };
        let tls =
            Error::InvalidCertificate(CertificateError::Other(OtherError(Arc::new(rejected))));
        let io = std::io::Error::new(std::io::ErrorKind::InvalidData, tls);
        assert_eq!(rejection(&io), Some(rejected));
    }
}
