//! A tiny in-process TLS server with a self-signed certificate, for the
//! account setup tests. One thread per connection, any number of them.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use envio_net::Fingerprint;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};

#[derive(Clone, Copy)]
pub enum Kind {
    /// Only the handshake (the client refuses the certificate and leaves).
    Handshake,
    /// Greets, then accepts any LOGIN and answers every command OK.
    Imap,
    /// Like `Imap`, with an INBOX that holds no mail: LIST, SELECT and
    /// SEARCH answer for it, and every command line is kept in `log`.
    Mailbox,
    /// Greets, then accepts any AUTH.
    Smtp,
}

pub struct TlsServer {
    pub port: u16,
    /// The certificate's fingerprint as the core shows it in hex.
    pub hex: String,
    /// The command lines a `Mailbox` server was sent (tag included).
    pub log: Arc<Mutex<Vec<String>>>,
}

pub fn serve(kind: Kind) -> TlsServer {
    let made = rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_owned()]).unwrap();
    let der = made.cert.der().to_vec();
    let hex = Fingerprint::of(&der).to_hex();
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(made.signing_key.serialize_der()));
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![CertificateDer::from(der)], key)
        .unwrap();
    let config = Arc::new(config);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let log = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&log);
    thread::spawn(move || {
        for tcp in listener.incoming().flatten() {
            let config = Arc::clone(&config);
            let log = Arc::clone(&kept);
            thread::spawn(move || {
                let _ = handle(kind, config, tcp, &log);
            });
        }
    });
    TlsServer { port, hex, log }
}

fn handle(
    kind: Kind,
    config: Arc<ServerConfig>,
    tcp: TcpStream,
    log: &Mutex<Vec<String>>,
) -> std::io::Result<()> {
    let conn = ServerConnection::new(config).map_err(std::io::Error::other)?;
    let mut tls = StreamOwned::new(conn, tcp);
    match kind {
        Kind::Handshake => {
            // Finish the handshake (or fail it: the client may refuse the
            // certificate), then hang up without a greeting.
            tls.conn.complete_io(&mut tls.sock)?;
            Ok(())
        }
        Kind::Imap => {
            tls.write_all(b"* OK fake ready\r\n")?;
            talk(tls, imap_reply, log)
        }
        Kind::Mailbox => {
            tls.write_all(b"* OK fake ready\r\n")?;
            talk(tls, mailbox_reply, log)
        }
        Kind::Smtp => {
            tls.write_all(b"220 fake ESMTP\r\n")?;
            talk(tls, smtp_reply, log)
        }
    }
}

type Stream = StreamOwned<ServerConnection, TcpStream>;

/// Answers each line with `reply`'s text; `None` text ends the talk.
fn talk(
    tls: Stream,
    reply: fn(&str) -> (String, bool),
    log: &Mutex<Vec<String>>,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(tls);
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        log.lock().unwrap().push(line.trim_end().to_owned());
        let (text, last) = reply(line.trim_end());
        reader.get_mut().write_all(text.as_bytes())?;
        if last {
            return Ok(());
        }
    }
}

fn imap_reply(line: &str) -> (String, bool) {
    let (tag, command) = line.split_once(' ').unwrap_or((line, ""));
    let word = command.split(' ').next().unwrap_or("").to_ascii_uppercase();
    match word.as_str() {
        "CAPABILITY" => (
            format!("* CAPABILITY IMAP4rev1 IDLE\r\n{tag} OK done\r\n"),
            false,
        ),
        "LOGOUT" => (format!("* BYE\r\n{tag} OK bye\r\n"), true),
        _ => (format!("{tag} OK done\r\n"), false),
    }
}

fn smtp_reply(line: &str) -> (String, bool) {
    let word = line.split(' ').next().unwrap_or("").to_ascii_uppercase();
    match word.as_str() {
        "EHLO" => ("250-fake\r\n250 AUTH PLAIN LOGIN\r\n".to_owned(), false),
        "AUTH" => ("235 2.7.0 ok\r\n".to_owned(), false),
        "QUIT" => ("221 bye\r\n".to_owned(), true),
        _ => ("250 ok\r\n".to_owned(), false),
    }
}

fn mailbox_reply(line: &str) -> (String, bool) {
    let (tag, command) = line.split_once(' ').unwrap_or((line, ""));
    let word = command.split(' ').next().unwrap_or("").to_ascii_uppercase();
    let text = match word.as_str() {
        "LIST" => format!("* LIST (\\HasNoChildren) \"/\" \"INBOX\"\r\n{tag} OK done\r\n"),
        "SELECT" => format!(
            "* 0 EXISTS\r\n* OK [UIDVALIDITY 1] ok\r\n* OK [UIDNEXT 1] ok\r\n{tag} OK [READ-WRITE] done\r\n"
        ),
        "UID" if command.to_ascii_uppercase().starts_with("UID SEARCH") => {
            format!("* SEARCH\r\n{tag} OK done\r\n")
        }
        _ => return imap_reply(line),
    };
    (text, false)
}
