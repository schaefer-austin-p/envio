//! How to reach and trust a server: host, port, TLS mode and the pinned
//! certificate, if the user accepted a self-signed one.

use std::fmt;

use secrecy::SecretString;

/// How the connection becomes encrypted. There is no plaintext mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Security {
    /// TLS from the first byte (IMAP 993, SMTP 465). Preferred.
    Tls,
    /// Plain connect, then STARTTLS before anything else (IMAP 143,
    /// SMTP 587). A server that doesn't offer it is an error.
    StartTls,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    pub security: Security,
    /// The accepted certificate. When set, only this certificate is
    /// trusted for this server, whatever the system roots say.
    pub pin: Option<Fingerprint>,
}

impl Endpoint {
    pub fn new(host: &str, port: u16, security: Security) -> Self {
        Self {
            host: host.to_owned(),
            port,
            security,
            pin: None,
        }
    }
}

/// The SHA-256 of a certificate's DER bytes.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fingerprint([u8; 32]);

impl Fingerprint {
    pub fn of(der: &[u8]) -> Self {
        let digest = ring::digest::digest(&ring::digest::SHA256, der);
        let mut bytes = [0; 32];
        bytes.copy_from_slice(digest.as_ref());
        Self(bytes)
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// 64 lowercase hex digits, the form settings files keep.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Reads [`Fingerprint::to_hex`]'s form; any case, no separators.
    pub fn from_hex(text: &str) -> Option<Self> {
        // from_str_radix alone would take a "+" sign.
        if text.len() != 64 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let mut bytes = [0; 32];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).ok()?;
        }
        Some(Self(bytes))
    }

    /// Uppercase pairs joined by colons, as the certificate prompt shows it.
    pub fn display(&self) -> String {
        self.0
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(":")
    }
}

impl fmt::Debug for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fingerprint({})", self.to_hex())
    }
}

/// A sign-in. The password stays in a `SecretString`, which is wiped when
/// dropped and never printed by `Debug`.
#[derive(Debug, Clone)]
pub struct Credentials {
    pub username: String,
    pub password: SecretString,
}

impl Credentials {
    pub fn new(username: &str, password: &str) -> Self {
        Self {
            username: username.to_owned(),
            password: SecretString::from(password.to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fingerprint_is_the_sha256_of_the_der() {
        // SHA-256 of the empty input.
        let fp = Fingerprint::of(b"");
        assert_eq!(
            fp.to_hex(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert!(fp.display().starts_with("E3:B0:C4:42:"));
        assert_eq!(fp.display().len(), 32 * 3 - 1);
    }

    #[test]
    fn hex_round_trips_in_either_case() {
        let fp = Fingerprint::from_bytes([0xab; 32]);
        assert_eq!(Fingerprint::from_hex(&fp.to_hex()), Some(fp));
        assert_eq!(Fingerprint::from_hex(&fp.to_hex().to_uppercase()), Some(fp));
    }

    #[test]
    fn malformed_hex_is_refused() {
        assert_eq!(Fingerprint::from_hex("ab"), None);
        assert_eq!(Fingerprint::from_hex(&"g".repeat(64)), None);
        // u8::from_str_radix alone would read "+f" as 15.
        assert_eq!(Fingerprint::from_hex(&"+f".repeat(32)), None);
    }

    #[test]
    fn debug_never_shows_the_password() {
        let creds = Credentials::new("alice", "hunter2");
        assert!(!format!("{creds:?}").contains("hunter2"));
    }
}
