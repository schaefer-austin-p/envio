//! Fake peers of a Microsoft-style OAuth account: a token endpoint over TLS
//! and an IMAP and an SMTP server that accept XOAUTH2 with the tokens in
//! `Peers::valid`. Threads, any number of connections, everything recorded.
//! All three present certificates signed by one private CA (`Ca::roots`).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use rcgen::{BasicConstraints, CertificateParams, CertifiedIssuer, DnType, IsCa, KeyPair};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};

pub struct Ca {
    issuer: CertifiedIssuer<'static, KeyPair>,
}

impl Ca {
    pub fn new() -> Self {
        let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params
            .distinguished_name
            .push(DnType::CommonName, "envio test ca");
        let issuer = CertifiedIssuer::self_signed(params, KeyPair::generate().unwrap()).unwrap();
        Self { issuer }
    }

    /// Roots that trust the servers made here and nothing else.
    pub fn roots(&self) -> envio_net::Roots {
        envio_net::Roots::from_der([self.issuer.der().to_vec()])
    }

    fn config(&self) -> Arc<ServerConfig> {
        let key = KeyPair::generate().unwrap();
        let params = CertificateParams::new(vec!["127.0.0.1".to_owned()]).unwrap();
        let cert = params.signed_by(&key, &self.issuer).unwrap();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der()));
        Arc::new(
            ServerConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_no_client_auth()
                .with_single_cert(vec![CertificateDer::from(cert.der().to_vec())], der)
                .unwrap(),
        )
    }
}

/// What the mail servers accept and what they saw.
#[derive(Default)]
pub struct Peers {
    /// The access tokens IMAP and SMTP accept.
    pub valid: Mutex<Vec<String>>,
    /// The tokens presented to IMAP, in order.
    pub imap_tokens: Mutex<Vec<String>>,
    /// The tokens presented to SMTP, in order.
    pub smtp_tokens: Mutex<Vec<String>>,
    /// SMTP refuses every token (Authenticated SMTP is off for the
    /// mailbox), IMAP does not.
    pub smtp_denied: Mutex<bool>,
    /// The final reply of a refused SMTP AUTH.
    pub smtp_refusal: Mutex<String>,
    /// Milliseconds IMAP and SMTP wait before answering a sign-in.
    pub imap_delay_ms: Mutex<u64>,
    pub smtp_delay_ms: Mutex<u64>,
    /// Messages SMTP accepted.
    pub sent: Mutex<usize>,
}

impl Peers {
    pub fn new(valid: &[&str]) -> Arc<Self> {
        let peers = Self::default();
        *peers.valid.lock().unwrap() = valid.iter().map(|t| (*t).to_owned()).collect();
        *peers.smtp_refusal.lock().unwrap() = "535 5.7.3 Authentication unsuccessful".into();
        Arc::new(peers)
    }

    fn accepts(&self, user: &str, token: &str) -> bool {
        user == "me@outlook.com" && self.valid.lock().unwrap().iter().any(|v| v == token)
    }
}

type Stream = StreamOwned<ServerConnection, TcpStream>;

fn listen(ca: &Ca, handler: impl Fn(Stream) + Send + Sync + 'static) -> u16 {
    let config = ca.config();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let handler = Arc::new(handler);
    thread::spawn(move || {
        for tcp in listener.incoming().flatten() {
            let (config, handler) = (Arc::clone(&config), Arc::clone(&handler));
            thread::spawn(move || {
                if let Ok(conn) = ServerConnection::new(config) {
                    handler(StreamOwned::new(conn, tcp));
                }
            });
        }
    });
    port
}

/// One reply of the token endpoint: status and JSON body.
pub type Reply = (u16, String);

/// `{"access_token":..,"refresh_token":..,"expires_in":3599}`.
pub fn tokens_reply(access: &str, refresh: &str) -> Reply {
    (
        200,
        format!(r#"{{"access_token":"{access}","refresh_token":"{refresh}","expires_in":3599}}"#),
    )
}

/// A token reply that also carries an id token naming `email` and `upn`.
pub fn tokens_reply_for(
    access: &str,
    refresh: &str,
    email: Option<&str>,
    upn: Option<&str>,
) -> Reply {
    (
        200,
        format!(
            r#"{{"access_token":"{access}","refresh_token":"{refresh}","expires_in":3599,"id_token":"{}"}}"#,
            id_token(email, upn)
        ),
    )
}

/// An unsigned JWT with the two identity claims (the core does not verify it).
pub fn id_token(email: Option<&str>, upn: Option<&str>) -> String {
    let url = |bytes: &[u8]| {
        encode(bytes)
            .trim_end_matches('=')
            .replace('+', "-")
            .replace('/', "_")
    };
    let mut claims = Vec::new();
    if let Some(email) = email {
        claims.push(format!(r#""email":"{email}""#));
    }
    if let Some(upn) = upn {
        claims.push(format!(r#""preferred_username":"{upn}""#));
    }
    let payload = format!("{{{}}}", claims.join(","));
    format!(
        "{}.{}.sig",
        url(br#"{"alg":"none"}"#),
        url(payload.as_bytes())
    )
}

/// `invalid_grant`: the refresh token is revoked.
pub fn revoked_reply() -> Reply {
    (400, r#"{"error":"invalid_grant"}"#.to_owned())
}

/// A token endpoint: `reply(n, form)` answers the nth request (from 0)
/// after `delay`. Returns the port and every request's form body.
pub fn token_endpoint(
    ca: &Ca,
    delay: Duration,
    reply: impl Fn(usize, &str) -> Reply + Send + Sync + 'static,
) -> (u16, Arc<Mutex<Vec<String>>>) {
    let bodies = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&bodies);
    let port = listen(ca, move |tls| {
        let mut reader = BufReader::new(tls);
        let mut length = 0;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                return;
            }
            let line = line.trim_end().to_ascii_lowercase();
            if line.is_empty() {
                break;
            }
            if let Some(n) = line.strip_prefix("content-length:") {
                length = n.trim().parse().unwrap_or(0);
            }
        }
        let mut body = vec![0; length];
        if reader.read_exact(&mut body).is_err() {
            return;
        }
        let body = String::from_utf8_lossy(&body).to_string();
        let n = {
            let mut seen = seen.lock().unwrap();
            seen.push(body.clone());
            seen.len() - 1
        };
        thread::sleep(delay);
        let (status, json) = reply(n, &body);
        let text = format!(
            "HTTP/1.0 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{json}",
            json.len()
        );
        let _ = reader.get_mut().write_all(text.as_bytes());
        let _ = reader.get_mut().flush();
    });
    (port, bodies)
}

/// The `refresh_token` value of a token request's form body.
pub fn refresh_token_of(form: &str) -> Option<String> {
    form.split('&')
        .find_map(|pair| pair.strip_prefix("refresh_token="))
        .map(str::to_owned)
}

/// An IMAP server (implicit TLS) with XOAUTH2 and an empty INBOX.
pub fn imap_server(ca: &Ca, peers: &Arc<Peers>) -> u16 {
    let peers = Arc::clone(peers);
    listen(ca, move |tls| {
        let _ = imap_talk(tls, &peers);
    })
}

fn imap_talk(tls: Stream, peers: &Peers) -> std::io::Result<()> {
    let mut io = BufReader::new(tls);
    io.get_mut().write_all(b"* OK fake ready\r\n")?;
    let mut signed_in = false;
    let mut idling: Option<String> = None;
    let mut line = String::new();
    loop {
        line.clear();
        if io.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let text = line.trim_end().to_owned();
        if let Some(tag) = idling.take_if(|_| text.eq_ignore_ascii_case("DONE")) {
            io.get_mut()
                .write_all(format!("{tag} OK idle done\r\n").as_bytes())?;
            continue;
        }
        let (tag, command) = text.split_once(' ').unwrap_or((&text, ""));
        let upper = command.to_ascii_uppercase();
        let word = upper.split(' ').next().unwrap_or("");
        let reply = match word {
            "CAPABILITY" if signed_in => {
                format!("* CAPABILITY IMAP4rev1 IDLE\r\n{tag} OK done\r\n")
            }
            "CAPABILITY" => format!("* CAPABILITY IMAP4rev1 AUTH=XOAUTH2\r\n{tag} OK done\r\n"),
            "AUTHENTICATE" => {
                io.get_mut().write_all(b"+ \r\n")?;
                line.clear();
                io.read_line(&mut line)?;
                let (user, token) = sign_in_of(line.trim_end());
                peers.imap_tokens.lock().unwrap().push(token.clone());
                let delay = *peers.imap_delay_ms.lock().unwrap();
                std::thread::sleep(std::time::Duration::from_millis(delay));
                if peers.accepts(&user, &token) {
                    signed_in = true;
                    format!("{tag} OK signed in\r\n")
                } else {
                    io.get_mut().write_all(
                        format!("+ {}\r\n", encode(br#"{"status":"401"}"#)).as_bytes(),
                    )?;
                    line.clear();
                    io.read_line(&mut line)?;
                    format!("{tag} NO AUTHENTICATE failed.\r\n")
                }
            }
            "LOGOUT" => {
                io.get_mut()
                    .write_all(format!("* BYE\r\n{tag} OK bye\r\n").as_bytes())?;
                return Ok(());
            }
            "IDLE" => {
                idling = Some(tag.to_owned());
                "+ idling\r\n".to_owned()
            }
            "LIST" => format!("* LIST (\\HasNoChildren) \"/\" \"INBOX\"\r\n{tag} OK done\r\n"),
            "SELECT" => format!(
                "* 0 EXISTS\r\n* OK [UIDVALIDITY 1] ok\r\n* OK [UIDNEXT 1] ok\r\n{tag} OK [READ-WRITE] done\r\n"
            ),
            "UID" if upper.starts_with("UID SEARCH") => {
                format!("* SEARCH\r\n{tag} OK done\r\n")
            }
            _ => format!("{tag} OK done\r\n"),
        };
        io.get_mut().write_all(reply.as_bytes())?;
    }
}

/// An SMTP server (implicit TLS) with XOAUTH2 that takes any message.
pub fn smtp_server(ca: &Ca, peers: &Arc<Peers>) -> u16 {
    let peers = Arc::clone(peers);
    listen(ca, move |tls| {
        let _ = smtp_talk(tls, &peers);
    })
}

fn smtp_talk(tls: Stream, peers: &Peers) -> std::io::Result<()> {
    let mut io = BufReader::new(tls);
    io.get_mut().write_all(b"220 fake ESMTP\r\n")?;
    let mut line = String::new();
    loop {
        line.clear();
        if io.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let text = line.trim_end().to_owned();
        let word = text.split(' ').next().unwrap_or("").to_ascii_uppercase();
        let reply = match word.as_str() {
            "EHLO" => "250-fake\r\n250-8BITMIME\r\n250 AUTH XOAUTH2\r\n".to_owned(),
            "AUTH" => {
                let (user, token) = sign_in_of(text.split(' ').nth(2).unwrap_or(""));
                peers.smtp_tokens.lock().unwrap().push(token.clone());
                let delay = *peers.smtp_delay_ms.lock().unwrap();
                std::thread::sleep(std::time::Duration::from_millis(delay));
                if peers.accepts(&user, &token) && !*peers.smtp_denied.lock().unwrap() {
                    "235 2.7.0 Authentication successful\r\n".to_owned()
                } else {
                    let refusal = peers.smtp_refusal.lock().unwrap().clone();
                    // Microsoft answers a disabled mailbox at once; any
                    // other refusal comes after the 334 error challenge.
                    if !refusal.contains("5.7.139") {
                        io.get_mut().write_all(
                            format!("334 {}\r\n", encode(br#"{"status":"401"}"#)).as_bytes(),
                        )?;
                        line.clear();
                        io.read_line(&mut line)?;
                    }
                    format!("{refusal}\r\n")
                }
            }
            "DATA" => {
                io.get_mut().write_all(b"354 go\r\n")?;
                loop {
                    line.clear();
                    if io.read_line(&mut line)? == 0 || line.trim_end() == "." {
                        break;
                    }
                }
                *peers.sent.lock().unwrap() += 1;
                "250 queued\r\n".to_owned()
            }
            "QUIT" => {
                io.get_mut().write_all(b"221 bye\r\n")?;
                return Ok(());
            }
            _ => "250 ok\r\n".to_owned(),
        };
        io.get_mut().write_all(reply.as_bytes())?;
    }
}

/// The user and the access token in a base64 XOAUTH2 initial response
/// (`user=<user>\x01auth=Bearer <token>\x01\x01`).
fn sign_in_of(b64: &str) -> (String, String) {
    let raw = String::from_utf8_lossy(&decode(b64)).to_string();
    let field = |name: &str| {
        raw.split('\u{1}')
            .find_map(|part| part.strip_prefix(name))
            .unwrap_or("")
            .to_owned()
    };
    (field("user="), field("auth=Bearer "))
}

fn decode(text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let (mut bits, mut have) = (0u32, 0);
    for c in text.trim_end_matches('=').bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Vec::new(),
        };
        bits = (bits << 6) | u32::from(v);
        have += 6;
        if have >= 8 {
            have -= 8;
            out.push((bits >> have) as u8);
            bits &= (1 << have) - 1;
        }
    }
    out
}

fn encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = |i: usize| u32::from(*chunk.get(i).unwrap_or(&0));
        let n = (b(0) << 16) | (b(1) << 8) | b(2);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}
