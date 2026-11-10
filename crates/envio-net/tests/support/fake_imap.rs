//! A scripted IMAP server, in process, that records what the client says.
//! Just enough for the connect path: greeting, STARTTLS, LOGIN,
//! CAPABILITY, AUTHENTICATE PLAIN, NOOP, APPEND, LOGOUT.

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

use super::certs::Cert;
pub use super::fake_smtp::Transcript;

#[derive(Clone, Copy)]
pub struct Script {
    /// Implicit TLS (993) instead of plaintext with STARTTLS (143).
    pub implicit: bool,
    /// The greeting line, without CRLF.
    pub greeting: &'static str,
    /// The status word STARTTLS gets: "OK", "NO" or "BAD".
    pub starttls: &'static str,
    /// Bytes sent right after the STARTTLS OK, in the same write.
    pub inject: &'static str,
    /// The status and text LOGIN gets.
    pub login: &'static str,
    /// The CAPABILITY words after IMAP4rev1, e.g. "IDLE UIDPLUS".
    pub capabilities: &'static str,
    /// Untagged lines (each ending in CRLF) sent after an APPEND's
    /// literal, before its tagged OK.
    pub before_append_ok: &'static str,
    /// The CAPABILITY words after IMAP4rev1 while still signed out, e.g.
    /// "AUTH=PLAIN".
    pub signed_out_capabilities: &'static str,
}

impl Default for Script {
    fn default() -> Self {
        Self {
            implicit: false,
            greeting: "* OK fake IMAP ready",
            starttls: "OK",
            inject: "",
            login: "OK signed in",
            capabilities: "IDLE UIDPLUS",
            before_append_ok: "",
            signed_out_capabilities: "",
        }
    }
}

/// Serves one connection; returns the port and the transcript.
pub async fn serve(cert: &Cert, script: Script) -> (u16, Transcript) {
    let acceptor = cert.acceptor();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let transcript = Transcript::default();
    let log = transcript.clone();
    tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let greeting = format!("{}\r\n", script.greeting);
        if script.implicit {
            let mut io = BufReader::new(acceptor.accept(tcp).await.unwrap());
            io.write_all(greeting.as_bytes()).await.unwrap();
            secure_part(&mut io, script, &log).await;
            return;
        }
        let mut io = BufReader::new(tcp);
        io.write_all(greeting.as_bytes()).await.unwrap();
        let Some(command) = line(&mut io, false, &log).await else {
            return;
        };
        let (tag, verb) = split(&command);
        if verb != "STARTTLS" {
            let _ = io
                .write_all(format!("{tag} BAD say STARTTLS first\r\n").as_bytes())
                .await;
            return;
        }
        let reply = format!("{tag} {} begin TLS\r\n{}", script.starttls, script.inject);
        io.write_all(reply.as_bytes()).await.unwrap();
        if script.starttls != "OK" {
            // Keep reading so anything sent after a refusal is recorded.
            while line(&mut io, false, &log).await.is_some() {}
            return;
        }
        let Ok(tls) = acceptor.accept(io.into_inner()).await else {
            return;
        };
        secure_part(&mut BufReader::new(tls), script, &log).await;
    });
    (port, transcript)
}

fn split(command: &str) -> (String, String) {
    let mut words = command.split(' ');
    let tag = words.next().unwrap_or("*").to_owned();
    let verb = words.next().unwrap_or("").to_ascii_uppercase();
    (tag, verb)
}

async fn line<S: AsyncRead + Unpin>(
    io: &mut BufReader<S>,
    tls: bool,
    log: &Transcript,
) -> Option<String> {
    let mut text = String::new();
    match io.read_line(&mut text).await {
        Ok(0) | Err(_) => None,
        Ok(_) => {
            log.push(tls, &text);
            Some(text.trim_end().to_owned())
        }
    }
}

/// Asks for an APPEND's `{n}` literal and reads it with its CRLF.
async fn literal<S: AsyncRead + AsyncWrite + Unpin>(io: &mut BufReader<S>, command: &str) -> bool {
    let size = command
        .rsplit('{')
        .next()
        .and_then(|rest| rest.strip_suffix('}'))
        .and_then(|n| n.parse::<usize>().ok());
    let Some(size) = size else {
        return false;
    };
    if io.write_all(b"+ go\r\n").await.is_err() {
        return false;
    }
    let mut body = vec![0; size + 2];
    io.read_exact(&mut body).await.is_ok()
}

async fn secure_part<S: AsyncRead + AsyncWrite + Unpin>(
    io: &mut BufReader<S>,
    script: Script,
    log: &Transcript,
) {
    let mut signed_in = false;
    while let Some(command) = line(io, true, log).await {
        let (tag, verb) = split(&command);
        // "UID COPY ...": the word after UID, for the commands that need it.
        let sub_verb = command.split(' ').nth(2).unwrap_or("").to_ascii_uppercase();
        let reply = match verb.as_str() {
            "LOGIN" => {
                signed_in = script.login.starts_with("OK");
                format!("{tag} {}\r\n", script.login)
            }
            "AUTHENTICATE" if command.to_ascii_uppercase().ends_with(" PLAIN") => {
                if io.write_all(b"+ \r\n").await.is_err() {
                    return;
                }
                let Some(answer) = line(io, true, log).await else {
                    return;
                };
                // RFC 4616: NUL, user name, NUL, password, in UTF-8.
                signed_in = base64_decode(&answer) == Some("\0alice\0pässwort".as_bytes().to_vec());
                if signed_in {
                    format!("{tag} OK signed in\r\n")
                } else {
                    format!("{tag} NO [AUTHENTICATIONFAILED] invalid credentials\r\n")
                }
            }
            "CAPABILITY" => format!(
                "* CAPABILITY IMAP4rev1 {}\r\n{tag} OK done\r\n",
                if signed_in {
                    script.capabilities
                } else {
                    script.signed_out_capabilities
                }
            ),
            "NOOP" => format!("{tag} OK done\r\n"),
            "APPEND" => {
                if !literal(io, &command).await {
                    return;
                }
                format!("{}{tag} OK stored\r\n", script.before_append_ok)
            }
            // Accepted and recorded; the tests read the transcript.
            "EXPUNGE" => format!("{tag} OK done\r\n"),
            "UID" if matches!(sub_verb.as_str(), "COPY" | "STORE" | "EXPUNGE") => {
                format!("{tag} OK done\r\n")
            }
            "LOGOUT" => {
                let _ = io
                    .write_all(format!("* BYE bye\r\n{tag} OK done\r\n").as_bytes())
                    .await;
                return;
            }
            _ => format!("{tag} BAD what\r\n"),
        };
        if io.write_all(reply.as_bytes()).await.is_err() {
            return;
        }
    }
}

/// Standard base64, enough to read one AUTHENTICATE answer.
fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut bits, mut have) = (0u32, 0);
    for c in text.trim_end_matches('=').bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        bits = (bits << 6) | u32::from(v);
        have += 6;
        if have >= 8 {
            have -= 8;
            out.push((bits >> have) as u8);
            bits &= (1 << have) - 1;
        }
    }
    Some(out)
}
