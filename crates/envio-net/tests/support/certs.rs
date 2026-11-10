//! Test certificates and an in-process TLS acceptor.

use std::sync::Arc;

use rcgen::{BasicConstraints, CertificateParams, CertifiedIssuer, IsCa, KeyPair};
use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_rustls::TlsAcceptor;

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
}

/// A private CA that signs server certificates.
pub struct Ca {
    issuer: CertifiedIssuer<'static, KeyPair>,
}

impl Ca {
    pub fn new() -> Self {
        let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
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
}
