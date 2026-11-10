//! Test certificates and an in-process TLS acceptor.

use std::sync::Arc;

use rcgen::{
    BasicConstraints, CertificateParams, CertifiedIssuer, DnType, IsCa, KeyPair, date_time_ymd,
};
use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;
use tokio_rustls::TlsAcceptor;

/// Parameters for a certificate whose validity ended in 2001.
fn expired_params(name: &str) -> CertificateParams {
    let mut params = CertificateParams::new(vec![name.to_owned()]).unwrap();
    params.not_before = date_time_ymd(2000, 1, 1);
    params.not_after = date_time_ymd(2001, 1, 1);
    params
}

/// Hands every client the same certified key, whatever the hello says.
#[derive(Debug)]
struct Fixed(Arc<CertifiedKey>);

impl ResolvesServerCert for Fixed {
    fn resolve(&self, _hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(self.0.clone())
    }
}

/// A server certificate with its key.
pub struct Cert {
    pub der: Vec<u8>,
    key: Vec<u8>,
}

impl Cert {
    /// Self-signed for `name`, like a home server's.
    pub fn self_signed(name: &str) -> Self {
        let made = rcgen::generate_simple_self_signed(vec![name.to_owned()]).unwrap();
        Self {
            der: made.cert.der().to_vec(),
            key: made.signing_key.serialize_der(),
        }
    }

    pub fn acceptor(&self) -> TlsAcceptor {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.key.clone()));
        let config = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![CertificateDer::from(self.der.clone())], key)
            .unwrap();
        TlsAcceptor::from(Arc::new(config))
    }

    /// A certificate that expired in 2001, self-signed for `name`.
    pub fn self_signed_expired(name: &str) -> Self {
        let key = KeyPair::generate().unwrap();
        let cert = expired_params(name).self_signed(&key).unwrap();
        Self {
            der: cert.der().to_vec(),
            key: key.serialize_der(),
        }
    }

    /// Presents this certificate but signs the handshake with `other`'s
    /// key, so the CertificateVerify signature cannot match the certificate.
    pub fn acceptor_with_key_of(&self, other: &Cert) -> TlsAcceptor {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(other.key.clone()));
        let signing = provider.key_provider.load_private_key(key).unwrap();
        let certified = CertifiedKey::new(vec![CertificateDer::from(self.der.clone())], signing);
        let config = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_cert_resolver(Arc::new(Fixed(Arc::new(certified))));
        TlsAcceptor::from(Arc::new(config))
    }
}

/// A private CA that signs server certificates.
pub struct Ca {
    issuer: CertifiedIssuer<'static, KeyPair>,
}

impl Ca {
    pub fn new() -> Self {
        let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        // rcgen gives every certificate the same default name. Without a
        // name of its own, this CA would match a self-signed server's issuer
        // name, and an unknown-issuer case would fail as a bad signature.
        params
            .distinguished_name
            .push(DnType::CommonName, "envio test ca");
        let issuer = CertifiedIssuer::self_signed(params, KeyPair::generate().unwrap()).unwrap();
        Self { issuer }
    }

    pub fn der(&self) -> Vec<u8> {
        self.issuer.der().to_vec()
    }

    pub fn sign(&self, name: &str) -> Cert {
        let key = KeyPair::generate().unwrap();
        let params = CertificateParams::new(vec![name.to_owned()]).unwrap();
        let cert = params.signed_by(&key, &self.issuer).unwrap();
        Cert {
            der: cert.der().to_vec(),
            key: key.serialize_der(),
        }
    }

    /// Signed by this CA, but expired in 2001.
    pub fn sign_expired(&self, name: &str) -> Cert {
        let key = KeyPair::generate().unwrap();
        let cert = expired_params(name).signed_by(&key, &self.issuer).unwrap();
        Cert {
            der: cert.der().to_vec(),
            key: key.serialize_der(),
        }
    }
}
