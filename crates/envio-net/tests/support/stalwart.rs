//! The Stalwart started by `tests/stalwart/up.sh`: where it is, its pinned
//! certificate, and the two test users. Tests skip when it isn't running,
//! unless ENVIO_REQUIRE_STALWART=1 (CI and `cargo make net-it`).

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use envio_net::imap::ImapSession;
use envio_net::{Credentials, Endpoint, Fingerprint, NetError, Roots, Security};

pub struct Stalwart {
    pub host: String,
    pub imaps: u16,
    pub imap: u16,
    pub submissions: u16,
    pub submission: u16,
    /// Stalwart's self-signed certificate, trusted on first use.
    pub pin: Fingerprint,
}

pub const ALICE: &str = "alice@envio.test";
pub const BOB: &str = "bob@envio.test";

pub fn alice() -> Credentials {
    Credentials::new("alice", "alice-secret-1")
}

pub fn bob() -> Credentials {
    Credentials::new("bob", "bob-secret-1")
}

fn port(name: &str) -> u16 {
    std::env::var(name)
        .unwrap_or_else(|_| panic!("{name} is not set; run tests/stalwart/up.sh"))
        .parse()
        .unwrap()
}

impl Stalwart {
    /// None (and a note on stderr) when up.sh hasn't been run.
    pub async fn find() -> Option<Self> {
        let Ok(host) = std::env::var("STALWART_HOST") else {
            assert!(
                std::env::var("ENVIO_REQUIRE_STALWART").as_deref() != Ok("1"),
                "ENVIO_REQUIRE_STALWART=1 but STALWART_HOST is not set"
            );
            eprintln!("skipped: no Stalwart (eval \"$(crates/envio-net/tests/stalwart/up.sh)\")");
            return None;
        };
        let imaps = port("STALWART_IMAPS_PORT");
        // Trust on first use, the way the app does it: connect unpinned,
        // take the fingerprint from the prompt.
        let unpinned = Endpoint::new(&host, imaps, Security::Tls);
        let pin = match envio_net::tls::connect(&unpinned, &Roots::empty()).await {
            Err(NetError::Untrusted { presented, .. }) => presented,
            other => panic!("expected the trust prompt, got {:?}", other.map(|_| ())),
        };
        Some(Self {
            host,
            imaps,
            imap: port("STALWART_IMAP_PORT"),
            submissions: port("STALWART_SUBMISSIONS_PORT"),
            submission: port("STALWART_SUBMISSION_PORT"),
            pin,
        })
    }

    pub fn endpoint(&self, port: u16, security: Security) -> Endpoint {
        Endpoint {
            pin: Some(self.pin),
            ..Endpoint::new(&self.host, port, security)
        }
    }

    pub async fn imap(&self, credentials: &Credentials) -> ImapSession {
        ImapSession::open(
            &self.endpoint(self.imaps, Security::Tls),
            &Roots::empty(),
            credentials,
        )
        .await
        .unwrap()
    }
}

/// A Message-ID no other test or run uses.
pub fn message_id() -> String {
    static COUNT: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let n = COUNT.fetch_add(1, Ordering::Relaxed);
    format!("<{nanos}.{}.{n}@envio.test>", std::process::id())
}

/// A plain-text message of about `body_len` bytes.
pub fn message(message_id: &str, subject: &str, body_len: usize) -> Vec<u8> {
    let mut eml = format!(
        "From: Alice <{ALICE}>\r\nTo: Bob <{BOB}>\r\nSubject: {subject}\r\n\
         Message-ID: {message_id}\r\nDate: Sun, 08 Nov 2026 10:00:00 +0000\r\n\
         MIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n"
    )
    .into_bytes();
    let line = b"The quick brown fox jumps over the lazy dog, again and again.\r\n";
    while eml.len() < body_len {
        eml.extend_from_slice(line);
    }
    eml
}
