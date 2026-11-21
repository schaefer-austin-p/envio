//! A scripted TLS server on 127.0.0.1 for Graph tests (feature `test-mock`).
//! Each request takes the next scripted [`Action`], whichever connection it
//! arrives on; the server counts accepted connections and records requests.
//! Its certificate is signed by a private CA that [`Server::roots`] trusts.
//! When the script runs out, the connection is closed.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use rcgen::{BasicConstraints, CertificateParams, CertifiedIssuer, DnType, IsCa, KeyPair};
use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_rustls::TlsAcceptor;

use crate::tls::Roots;

/// What the server does with the next request.
pub struct Action {
    kind: Kind,
}

enum Kind {
    Reply {
        bytes: Vec<u8>,
        close_after: bool,
    },
    /// Read the whole request, then close without a reply.
    DropAfterRequest,
}

impl Action {
    /// A Content-Length reply.
    pub fn json(status: u16, body: &str) -> Self {
        Self::reply(status, &[], body)
    }

    /// A Content-Length reply with extra headers.
    pub fn reply(status: u16, headers: &[(&str, &str)], body: &str) -> Self {
        let mut head = format!("HTTP/1.1 {status} X\r\nContent-Length: {}\r\n", body.len());
        for (name, value) in headers {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
        head.push_str("\r\n");
        head.push_str(body);
        Self::raw(head.into_bytes())
    }

    /// A chunked reply, one chunk per part.
    pub fn chunked(status: u16, parts: &[&str]) -> Self {
        let mut out = format!("HTTP/1.1 {status} X\r\nTransfer-Encoding: chunked\r\n\r\n");
        for part in parts {
            out.push_str(&format!("{:x}\r\n{part}\r\n", part.len()));
        }
        out.push_str("0\r\n\r\n");
        Self::raw(out.into_bytes())
    }

    /// These bytes, as they are.
    pub fn raw(bytes: Vec<u8>) -> Self {
        Self {
            kind: Kind::Reply {
                bytes,
                close_after: false,
            },
        }
    }

    /// Reads the request in full, then closes with no reply.
    pub fn drop_after_request() -> Self {
        Self {
            kind: Kind::DropAfterRequest,
        }
    }

    /// Closes the connection after this reply without having said so: the
    /// client's next request on it finds a dead socket.
    pub fn then_close(mut self) -> Self {
        if let Kind::Reply { close_after, .. } = &mut self.kind {
            *close_after = true;
        }
        self
    }
}

/// A request the server read.
#[derive(Debug, Clone)]
pub struct Seen {
    /// Which accepted connection carried it (0 is the first).
    pub conn: usize,
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Seen {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

struct Shared {
    script: Mutex<VecDeque<Action>>,
    seen: Mutex<Vec<Seen>>,
    accepts: AtomicUsize,
}

pub struct Server {
    port: u16,
    ca_der: Vec<u8>,
    shared: Arc<Shared>,
    task: JoinHandle<()>,
}

/// The host name its certificate is for.
pub const HOST: &str = "localhost";

impl Server {
    /// Starts serving `script`. Needs a running tokio runtime.
    pub async fn start(script: Vec<Action>) -> Self {
        let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params
            .distinguished_name
            .push(DnType::CommonName, "envio graph mock ca");
        let issuer = CertifiedIssuer::self_signed(params, KeyPair::generate().unwrap()).unwrap();
        let key = KeyPair::generate().unwrap();
        let cert = CertificateParams::new(vec![HOST.to_owned()])
            .unwrap()
            .signed_by(&key, &issuer)
            .unwrap();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(cert.der().to_vec())],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der())),
            )
            .unwrap();
        let acceptor = TlsAcceptor::from(Arc::new(config));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let shared = Arc::new(Shared {
            script: Mutex::new(script.into()),
            seen: Mutex::new(Vec::new()),
            accepts: AtomicUsize::new(0),
        });
        let task = tokio::spawn(accept_loop(listener, acceptor, shared.clone()));
        Self {
            port,
            ca_der: issuer.der().to_vec(),
            shared,
            task,
        }
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Roots that trust this server's certificate.
    pub fn roots(&self) -> Roots {
        Roots::from_der([self.ca_der.clone()])
    }

    /// Connections accepted so far (TLS handshakes not counted separately).
    pub fn accepts(&self) -> usize {
        self.shared.accepts.load(Ordering::SeqCst)
    }

    /// The requests read so far, in order.
    pub fn requests(&self) -> Vec<Seen> {
        self.shared.seen.lock().unwrap().clone()
    }

    /// Adds more to the end of the script.
    pub fn push(&self, action: Action) {
        self.shared.script.lock().unwrap().push_back(action);
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn accept_loop(listener: TcpListener, acceptor: TlsAcceptor, shared: Arc<Shared>) {
    while let Ok((tcp, _)) = listener.accept().await {
        let conn = shared.accepts.fetch_add(1, Ordering::SeqCst);
        let (acceptor, shared) = (acceptor.clone(), shared.clone());
        tokio::spawn(async move {
            if let Ok(tls) = acceptor.accept(tcp).await {
                serve(tls, conn, &shared).await;
            }
        });
    }
}

async fn serve<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    mut tls: S,
    conn: usize,
    shared: &Shared,
) {
    let mut buf = Vec::new();
    while let Some(seen) = read_request(&mut tls, &mut buf, conn).await {
        shared.seen.lock().unwrap().push(seen);
        let next = shared.script.lock().unwrap().pop_front();
        match next.map(|a| a.kind) {
            Some(Kind::Reply { bytes, close_after }) => {
                if tls.write_all(&bytes).await.is_err() || tls.flush().await.is_err() {
                    return;
                }
                if close_after {
                    let _ = tls.shutdown().await;
                    return;
                }
            }
            Some(Kind::DropAfterRequest) | None => return,
        }
    }
}

/// One request off `tls`; `buf` holds bytes read past the previous one.
async fn read_request<S: tokio::io::AsyncRead + Unpin>(
    tls: &mut S,
    buf: &mut Vec<u8>,
    conn: usize,
) -> Option<Seen> {
    let head_end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        let mut chunk = [0u8; 8192];
        let n = tls.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let (method, path) = (first.next()?.to_owned(), first.next()?.to_owned());
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.to_owned(), v.trim().to_owned()))
        .collect();
    let length: usize = headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let body_start = head_end + 4;
    while buf.len() < body_start + length {
        let mut chunk = [0u8; 8192];
        let n = tls.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let body = buf[body_start..body_start + length].to_vec();
    buf.drain(..body_start + length);
    Some(Seen {
        conn,
        method,
        path,
        headers,
        body,
    })
}
