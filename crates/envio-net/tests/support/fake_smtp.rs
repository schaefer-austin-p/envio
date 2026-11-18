//! A scripted SMTP server, in process, that records what the client says.

use std::sync::{Arc, Mutex};

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

use super::certs::Cert;
use super::fake_imap::{base64_decode, base64_encode};

#[derive(Clone, Copy)]
pub struct Script {
    /// Implicit TLS (465) instead of plaintext with STARTTLS (587).
    pub implicit: bool,
    /// Offer STARTTLS in the plaintext EHLO reply.
    pub offer_starttls: bool,
    /// Send extra bytes right after "220 go ahead" (STARTTLS injection).
    pub inject: bool,
    /// The reply to AUTH.
    pub auth_reply: &'static str,
    /// Send the start of the greeting with no line end, then close.
    pub cut_greeting: bool,
    /// The reply to MAIL; None closes the connection instead.
    pub mail_reply: Option<&'static str>,
    /// The reply to every RCPT.
    pub rcpt_reply: &'static str,
    /// The reply after the message's final dot; None closes the
    /// connection instead (the message is in, its fate unknown).
    pub data_reply: Option<&'static str>,
    /// Replaces the EHLO reply sent over TLS (whole reply, CRLF included).
    pub ehlo_reply: Option<&'static str>,
    /// The reply to DATA.
    pub data_go: &'static str,
    /// The decoded AUTH XOAUTH2 initial response the server accepts; when
    /// not empty, XOAUTH2 is advertised in the EHLO sent over TLS.
    pub xoauth2_accepts: &'static str,
    /// A refused XOAUTH2 first sends `334 <base64 json>`, then `535 5.7.3`.
    pub xoauth2_challenge: bool,
}

impl Default for Script {
    fn default() -> Self {
        Self {
            implicit: false,
            offer_starttls: true,
            inject: false,
            auth_reply: "235 2.7.0 ok",
            cut_greeting: false,
            mail_reply: Some("250 ok"),
            rcpt_reply: "250 ok",
            data_reply: Some("250 queued"),
            ehlo_reply: None,
            data_go: "354 go",
            xoauth2_accepts: "",
            xoauth2_challenge: false,
        }
    }
}

/// Every line the client sent, in order; `tls` marks lines sent over TLS.
#[derive(Clone, Default)]
pub struct Transcript(Arc<Mutex<Vec<(bool, String)>>>);

impl Transcript {
    pub fn lines(&self) -> Vec<(bool, String)> {
        self.0.lock().unwrap().clone()
    }

    pub fn push(&self, tls: bool, line: &str) {
        self.0
            .lock()
            .unwrap()
            .push((tls, line.trim_end().to_owned()));
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
        if script.implicit {
            let tls = acceptor.accept(tcp).await.unwrap();
            let mut io = BufReader::new(tls);
            io.write_all(b"220 fake ESMTP\r\n").await.unwrap();
            secure_part(&mut io, script, &log).await;
            return;
        }
        let mut io = BufReader::new(tcp);
        if script.cut_greeting {
            io.write_all(b"220 smtp.examp").await.unwrap();
            return;
        }
        io.write_all(b"220 fake ESMTP\r\n").await.unwrap();
        let Some(_ehlo) = line(&mut io, false, &log).await else {
            return;
        };
        if script.offer_starttls {
            io.write_all(b"250-fake\r\n250-PIPELINING\r\n250 STARTTLS\r\n")
                .await
                .unwrap();
        } else {
            io.write_all(b"250-fake\r\n250 AUTH PLAIN\r\n")
                .await
                .unwrap();
        }
        let Some(next) = line(&mut io, false, &log).await else {
            return;
        };
        if next != "STARTTLS" {
            return;
        }
        let ready: &[u8] = if script.inject {
            b"220 go ahead\r\n250 injected\r\n"
        } else {
            b"220 go ahead\r\n"
        };
        io.write_all(ready).await.unwrap();
        let Ok(tls) = acceptor.accept(io.into_inner()).await else {
            return;
        };
        secure_part(&mut BufReader::new(tls), script, &log).await;
    });
    (port, transcript)
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

async fn secure_part<S: AsyncRead + AsyncWrite + Unpin>(
    io: &mut BufReader<S>,
    script: Script,
    log: &Transcript,
) {
    while let Some(command) = line(io, true, log).await {
        let verb = command.split(' ').next().unwrap_or("").to_ascii_uppercase();
        let reply: String = match verb.as_str() {
            "EHLO" => script.ehlo_reply.map_or_else(
                || {
                    let more = if script.xoauth2_accepts.is_empty() {
                        ""
                    } else {
                        " XOAUTH2"
                    };
                    format!("250-fake\r\n250-8BITMIME\r\n250 AUTH PLAIN LOGIN{more}\r\n")
                },
                str::to_owned,
            ),
            "AUTH" if command.to_ascii_uppercase().starts_with("AUTH XOAUTH2") => {
                let sent = command.split(' ').nth(2).unwrap_or("");
                if !script.xoauth2_accepts.is_empty()
                    && base64_decode(sent) == Some(script.xoauth2_accepts.as_bytes().to_vec())
                {
                    "235 2.7.0 Authentication successful\r\n".to_owned()
                } else if script.xoauth2_challenge {
                    let json = base64_encode(br#"{"status":"401"}"#);
                    format!("334 {json}\r\n535 5.7.3 Authentication unsuccessful\r\n")
                } else {
                    "535 5.7.3 Authentication unsuccessful\r\n".to_owned()
                }
            }
            "AUTH" => format!("{}\r\n", script.auth_reply),
            "MAIL" => match script.mail_reply {
                Some(reply) => format!("{reply}\r\n"),
                None => return,
            },
            "RCPT" => format!("{}\r\n", script.rcpt_reply),
            "DATA" => {
                io.write_all(format!("{}\r\n", script.data_go).as_bytes())
                    .await
                    .unwrap();
                while let Some(body) = line(io, true, log).await {
                    if body == "." {
                        break;
                    }
                }
                match script.data_reply {
                    Some(reply) => format!("{reply}\r\n"),
                    None => return,
                }
            }
            "QUIT" => {
                let _ = io.write_all(b"221 bye\r\n").await;
                return;
            }
            _ => "500 what\r\n".into(),
        };
        if io.write_all(reply.as_bytes()).await.is_err() {
            return;
        }
    }
}
