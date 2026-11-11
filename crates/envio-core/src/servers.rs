//! Store servers (plain data) to net endpoints and back.

use envio_net::{Credentials, Endpoint, Fingerprint, Security};
use secrecy::{ExposeSecret, SecretString};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    Imap,
    Smtp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerAddr {
    pub host: String,
    pub port: u16,
    pub security: Security,
}

/// A stored pin that isn't a fingerprint. Never treated as "no pin".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadPin;

fn net_security(s: envio_store::Security) -> Security {
    match s {
        envio_store::Security::Tls => Security::Tls,
        envio_store::Security::StartTls => Security::StartTls,
    }
}

fn store_security(s: Security) -> envio_store::Security {
    match s {
        Security::Tls => envio_store::Security::Tls,
        Security::StartTls => envio_store::Security::StartTls,
    }
}

pub fn endpoint(server: &envio_store::Server) -> Result<Endpoint, BadPin> {
    let pin = match &server.pin {
        None => None,
        Some(hex) => Some(Fingerprint::from_hex(hex).ok_or(BadPin)?),
    };
    Ok(Endpoint {
        host: server.host.clone(),
        port: server.port,
        security: net_security(server.security),
        pin,
    })
}

pub fn store_server(addr: &ServerAddr, pin: Option<Fingerprint>) -> envio_store::Server {
    envio_store::Server {
        host: addr.host.clone(),
        port: addr.port,
        security: store_security(addr.security),
        pin: pin.map(|p| p.to_hex()),
    }
}

pub fn addr_of(server: &envio_store::Server) -> ServerAddr {
    ServerAddr {
        host: server.host.clone(),
        port: server.port,
        security: net_security(server.security),
    }
}

pub fn credentials(username: &str, password: &SecretString) -> Credentials {
    Credentials::new(username, password.expose_secret())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEX: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    fn server(security: envio_store::Security, pin: Option<&str>) -> envio_store::Server {
        envio_store::Server {
            host: "imap.example.org".into(),
            port: 993,
            security,
            pin: pin.map(str::to_owned),
        }
    }

    #[test]
    fn a_store_server_becomes_an_endpoint_with_its_pin() {
        let ep = endpoint(&server(envio_store::Security::Tls, Some(HEX))).unwrap();
        assert_eq!(ep.host, "imap.example.org");
        assert_eq!(ep.port, 993);
        assert_eq!(ep.security, Security::Tls);
        assert_eq!(ep.pin, Fingerprint::from_hex(HEX));
        assert!(ep.pin.is_some());
        let none = endpoint(&server(envio_store::Security::Tls, None)).unwrap();
        assert!(none.pin.is_none());
    }

    #[test]
    fn starttls_maps_both_ways() {
        let s = server(envio_store::Security::StartTls, None);
        assert_eq!(endpoint(&s).unwrap().security, Security::StartTls);
        assert_eq!(addr_of(&s).security, Security::StartTls);
        let addr = ServerAddr {
            host: "h".into(),
            port: 587,
            security: Security::StartTls,
        };
        assert_eq!(
            store_server(&addr, None).security,
            envio_store::Security::StartTls
        );
        let addr = ServerAddr {
            security: Security::Tls,
            ..addr
        };
        assert_eq!(
            store_server(&addr, None).security,
            envio_store::Security::Tls
        );
    }

    #[test]
    fn a_malformed_pin_is_refused_not_dropped() {
        let s = server(envio_store::Security::Tls, Some("not a fingerprint"));
        assert_eq!(endpoint(&s), Err(BadPin));
    }

    #[test]
    fn store_server_writes_the_pin_as_hex() {
        let addr = ServerAddr {
            host: "h".into(),
            port: 993,
            security: Security::Tls,
        };
        let pin = Fingerprint::from_hex(HEX).unwrap();
        assert_eq!(store_server(&addr, Some(pin)).pin.as_deref(), Some(HEX));
        assert_eq!(store_server(&addr, None).pin, None);
    }
}
