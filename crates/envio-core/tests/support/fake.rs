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
    /// Like `Mailbox`, with one message in the INBOX (UID 1, no flags)
    /// whose whole text is [`MESSAGE`]. UID SEARCH answers `1` for any
    /// criteria (it ignores SINCE and UID ranges), so a test of a different
    /// window must not use this kind.
    Message,
    /// Like `Mailbox`, with an INBOX of UIDs 1 to 10 (UIDNEXT 11): UID 10
    /// is new, 1 to 9 are dated 2020. SEARCH, FETCH of summaries and flags
    /// answer for them; every command line is kept in `log`.
    Older,
    /// Greets, then accepts any AUTH.
    Smtp,
}

/// What a `Message` server returns for the body of UID 1: text/plain with
/// one real attachment.
pub const MESSAGE: &str = "From: Ada <ada@example.org>\r\nSubject: file\r\n\
MIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=XX\r\n\r\n\
--XX\r\nContent-Type: text/plain\r\n\r\nsee attached\r\n\
--XX\r\nContent-Type: text/plain; name=\"x.txt\"\r\n\
Content-Disposition: attachment; filename=\"x.txt\"\r\n\
Content-Transfer-Encoding: base64\r\n\r\naGVsbG8=\r\n--XX--\r\n";

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
            talk(tls, imap_reply, true, log)
        }
        Kind::Mailbox => {
            tls.write_all(b"* OK fake ready\r\n")?;
            talk(tls, mailbox_reply, true, log)
        }
        Kind::Message => {
            tls.write_all(b"* OK fake ready\r\n")?;
            talk(tls, message_reply, true, log)
        }
        Kind::Older => {
            tls.write_all(b"* OK fake ready\r\n")?;
            talk(tls, older_reply, true, log)
        }
        Kind::Smtp => {
            tls.write_all(b"220 fake ESMTP\r\n")?;
            talk(tls, smtp_reply, false, log)
        }
    }
}

type Stream = StreamOwned<ServerConnection, TcpStream>;

/// Answers each line with `reply`'s text; `None` text ends the talk.
fn talk(
    tls: Stream,
    reply: fn(&str) -> (String, bool),
    imap: bool,
    log: &Mutex<Vec<String>>,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(tls);
    let mut line = String::new();
    // The tag of an IDLE the client has not ended with DONE yet.
    let mut idling: Option<String> = None;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        log.lock().unwrap().push(line.trim_end().to_owned());
        if let Some(tag) = idling.take_if(|_| imap && line.trim_end().eq_ignore_ascii_case("DONE"))
        {
            reader
                .get_mut()
                .write_all(format!("{tag} OK idle done\r\n").as_bytes())?;
            continue;
        }
        if imap
            && let Some((tag, command)) = line.trim_end().split_once(' ')
            && command.eq_ignore_ascii_case("IDLE")
        {
            idling = Some(tag.to_owned());
            reader.get_mut().write_all(b"+ idling\r\n")?;
            continue;
        }
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

fn message_reply(line: &str) -> (String, bool) {
    let (tag, command) = line.split_once(' ').unwrap_or((line, ""));
    let upper = command.to_ascii_uppercase();
    let text = if upper.starts_with("SELECT") {
        format!(
            "* 1 EXISTS\r\n* OK [UIDVALIDITY 1] ok\r\n* OK [UIDNEXT 2] ok\r\n{tag} OK [READ-WRITE] done\r\n"
        )
    } else if upper.starts_with("UID SEARCH") {
        // The one message is on the server: a sync must not expunge it.
        format!("* SEARCH 1\r\n{tag} OK done\r\n")
    } else if upper.starts_with("UID FETCH") && upper.contains("BODY.PEEK[]") {
        format!(
            "* 1 FETCH (UID 1 BODY[] {{{}}}\r\n{MESSAGE})\r\n{tag} OK done\r\n",
            MESSAGE.len()
        )
    } else if upper.starts_with("UID FETCH") && upper.contains("BODY.PEEK[HEADER]") {
        // The summary items envio-net sends: UID FLAGS RFC822.SIZE
        // INTERNALDATE [MODSEQ] BODY.PEEK[HEADER]. The header is MESSAGE's.
        let header = &MESSAGE[..MESSAGE.find("\r\n\r\n").unwrap() + 4];
        format!(
            "* 1 FETCH (UID 1 FLAGS () RFC822.SIZE {} INTERNALDATE \"01-Jan-2020 00:00:00 +0000\" BODY[HEADER] {{{}}}\r\n{header})\r\n{tag} OK done\r\n",
            MESSAGE.len(),
            header.len()
        )
    } else if upper.starts_with("UID FETCH") {
        format!("* 1 FETCH (UID 1 FLAGS ())\r\n{tag} OK done\r\n")
    } else {
        return mailbox_reply(line);
    };
    (text, false)
}

/// The UIDs a set like `9:1,10` names.
fn uids_of(set: &str) -> Vec<u32> {
    let mut out = Vec::new();
    for part in set.split(',') {
        let (a, b) = part.split_once(':').unwrap_or((part, part));
        let (a, b): (u32, u32) = (a.parse().unwrap(), b.parse().unwrap());
        out.extend(a.min(b)..=a.max(b));
    }
    out
}

fn older_reply(line: &str) -> (String, bool) {
    let (tag, command) = line.split_once(' ').unwrap_or((line, ""));
    let upper = command.to_ascii_uppercase();
    let words: Vec<&str> = command.split(' ').collect();
    let text = if upper.starts_with("SELECT") {
        format!(
            "* 10 EXISTS\r\n* OK [UIDVALIDITY 1] ok\r\n* OK [UIDNEXT 11] ok\r\n{tag} OK [READ-WRITE] done\r\n"
        )
    } else if upper.starts_with("UID SEARCH") {
        let found: Vec<u32> = if upper.contains("SINCE") {
            vec![10]
        } else if let Some(range) = words
            .get(3)
            .filter(|_| upper.starts_with("UID SEARCH UID "))
        {
            uids_of(range)
        } else {
            (1..=10).collect()
        };
        let list: String = found.iter().map(|u| format!(" {u}")).collect();
        format!("* SEARCH{list}\r\n{tag} OK done\r\n")
    } else if upper.starts_with("UID FETCH") {
        let mut out = String::new();
        for uid in uids_of(words[2]) {
            if upper.contains("BODY.PEEK[HEADER]") {
                let (date, header) = if uid == 10 {
                    (
                        "01-Jan-2099 00:00:00 +0000",
                        "Subject: new\r\n\r\n".to_owned(),
                    )
                } else {
                    (
                        "01-Jan-2020 00:00:00 +0000",
                        format!("Subject: old {uid}\r\n\r\n"),
                    )
                };
                out += &format!(
                    "* {uid} FETCH (UID {uid} FLAGS () RFC822.SIZE 50 INTERNALDATE \"{date}\" BODY[HEADER] {{{}}}\r\n{header})\r\n",
                    header.len()
                );
            } else {
                out += &format!("* {uid} FETCH (UID {uid} FLAGS ())\r\n");
            }
        }
        format!("{out}{tag} OK done\r\n")
    } else {
        return mailbox_reply(line);
    };
    (text, false)
}
