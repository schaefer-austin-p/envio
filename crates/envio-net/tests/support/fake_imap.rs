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
    /// The decoded AUTHENTICATE PLAIN answer the server accepts.
    pub plain_accepts: &'static str,
    /// IDLE is answered with `+ idling` and then `* OK Still here` every
    /// 100 ms until DONE.
    pub keepalive: bool,
    /// Untagged lines (each ending in CRLF) sent after the client's DONE,
    /// before IDLE's tagged OK.
    pub before_idle_ok: &'static str,
    /// UID FETCH is answered with `* n FETCH (UID u FLAGS (\Seen))` for
    /// every UID in the set.
    pub fetch_flags: bool,
    /// With `fetch_flags`: the 1-based UID FETCH from which the
    /// connection is dropped mid-command; 0 for never.
    pub fetch_fails_from: usize,
    /// With `fetch_flags`: the 1-based UID FETCH that is answered with
    /// `fetch_refusal` (a tagged status, such as `NO nope`) instead of OK.
    pub fetch_refused_at: usize,
    pub fetch_refusal: &'static str,
    /// When not empty, a UID SEARCH is answered with this tagged status
    /// (such as `NO nope`); empty leaves it to the default reply.
    pub search_refusal: &'static str,
    /// Untagged lines (each ending in CRLF) sent before a UID SEARCH's
    /// tagged reply, which is OK unless `search_refusal` says otherwise.
    pub search_untagged: &'static str,
    /// The untagged lines for a `UID SEARCH UNDELETED` (the folder's live
    /// UIDs); empty means `search_untagged`.
    pub search_all_untagged: &'static str,
    /// When not empty, the untagged lines for every HEADER search after
    /// the first (which gets `search_untagged`).
    pub search_later_untagged: &'static str,
    /// With `fetch_flags`: when not empty, the nth UID FETCH is answered
    /// with the nth of these (untagged lines, each ending in CRLF) instead
    /// of the flags; the commands after the last get no untagged line.
    pub fetch_canned: &'static [&'static str],
    /// When not zero, SELECT is answered with this many messages (and
    /// UIDVALIDITY 1), so the session knows the folder's size.
    pub select_exists: u32,
    /// When not empty, a UID EXPUNGE is answered with this tagged status
    /// (such as `NO nope`) instead of OK.
    pub expunge_refusal: &'static str,
    /// Untagged lines (each ending in CRLF) sent before a UID EXPUNGE's
    /// tagged reply.
    pub expunge_untagged: &'static str,
    /// When not empty, a UID STORE is answered with this tagged status.
    pub store_refusal: &'static str,
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
            plain_accepts: "\0alice\0pässwort",
            keepalive: false,
            before_idle_ok: "",
            fetch_flags: false,
            fetch_fails_from: 0,
            fetch_refused_at: 0,
            fetch_refusal: "NO refused",
            search_refusal: "",
            search_untagged: "",
            search_all_untagged: "",
            search_later_untagged: "",
            fetch_canned: &[],
            select_exists: 0,
            expunge_refusal: "",
            expunge_untagged: "",
            store_refusal: "",
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
    let mut fetches = 0;
    let mut searches = 0;
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
                signed_in =
                    base64_decode(&answer) == Some(script.plain_accepts.as_bytes().to_vec());
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
            "UID" | "FETCH" | "SELECT" if answers(&script, &verb, &sub_verb) => {
                let Some(reply) =
                    scripted_reply(&command, &tag, &script, (&mut fetches, &mut searches))
                else {
                    return;
                };
                reply
            }
            "IDLE" => idle_reply(io, log, &tag, &script).await,
            // Accepted and recorded; the tests read the transcript.
            _ if accepted(&verb, &sub_verb) => format!("{tag} OK done\r\n"),
            "APPEND" => {
                if !literal(io, &command).await {
                    return;
                }
                format!("{}{tag} OK stored\r\n", script.before_append_ok)
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

/// Whether the script has its own answer to this command.
fn answers(script: &Script, verb: &str, sub_verb: &str) -> bool {
    match verb {
        "FETCH" => script.fetch_flags,
        "SELECT" => script.select_exists != 0,
        _ => match sub_verb {
            "FETCH" => script.fetch_flags,
            "SEARCH" => {
                !(script.search_refusal.is_empty()
                    && script.search_untagged.is_empty()
                    && script.search_all_untagged.is_empty())
            }
            "EXPUNGE" => !(script.expunge_refusal.is_empty() && script.expunge_untagged.is_empty()),
            "STORE" => !script.store_refusal.is_empty(),
            _ => false,
        },
    }
}

/// The reply to a SELECT, a FETCH (by number or UID), a UID SEARCH or a
/// refused UID EXPUNGE; None from the fetch that is to drop the connection.
fn scripted_reply(
    command: &str,
    tag: &str,
    script: &Script,
    (fetches, searches): (&mut usize, &mut usize),
) -> Option<String> {
    let word = |n| command.split(' ').nth(n).unwrap_or("").to_ascii_uppercase();
    match (word(1).as_str(), word(2).as_str()) {
        ("SELECT", _) => Some(format!(
            "* {} EXISTS\r\n* OK [UIDVALIDITY 1] ok\r\n{tag} OK [READ-WRITE] done\r\n",
            script.select_exists
        )),
        ("UID", "SEARCH") => {
            let status = if script.search_refusal.is_empty() {
                "OK done"
            } else {
                script.search_refusal
            };
            let untagged =
                if command.ends_with(" UNDELETED") && !script.search_all_untagged.is_empty() {
                    script.search_all_untagged
                } else {
                    *searches += 1;
                    if *searches > 1 && !script.search_later_untagged.is_empty() {
                        script.search_later_untagged
                    } else {
                        script.search_untagged
                    }
                };
            Some(format!("{untagged}{tag} {status}\r\n"))
        }
        ("UID", "EXPUNGE") => {
            let status = match script.expunge_refusal {
                "" => "OK done",
                refusal => refusal,
            };
            Some(format!("{}{tag} {status}\r\n", script.expunge_untagged))
        }
        ("UID", "STORE") => Some(format!("{tag} {}\r\n", script.store_refusal)),
        _ => fetch_reply(command, tag, script, fetches),
    }
}

/// The flags of every UID in the command's set; None from the fetch
/// that is to drop the connection.
fn fetch_reply(command: &str, tag: &str, script: &Script, fetches: &mut usize) -> Option<String> {
    *fetches += 1;
    let nth = *fetches;
    if script.fetch_fails_from != 0 && nth >= script.fetch_fails_from {
        return None;
    }
    if nth == script.fetch_refused_at {
        return Some(format!("{tag} {}\r\n", script.fetch_refusal));
    }
    if !script.fetch_canned.is_empty() {
        let canned = script.fetch_canned.get(nth - 1).copied().unwrap_or("");
        return Some(format!("{canned}{tag} OK done\r\n"));
    }
    let set = command.split(' ').nth(3).unwrap_or("");
    let body: String = uids_in(set)
        .iter()
        .enumerate()
        .map(|(n, uid)| format!("* {} FETCH (UID {uid} FLAGS (\\Seen))\r\n", n + 1))
        .collect();
    Some(format!("{body}{tag} OK done\r\n"))
}

/// The UIDs a set like `1:3,7` names.
fn uids_in(set: &str) -> Vec<u32> {
    set.split(',')
        .flat_map(|run| match run.split_once(':') {
            Some((a, b)) => a.parse().unwrap_or(1)..=b.parse().unwrap_or(0),
            None => {
                let n = run.parse().unwrap_or(0);
                n..=n
            }
        })
        .collect()
}

/// The commands that are accepted and recorded, nothing more.
fn accepted(verb: &str, sub_verb: &str) -> bool {
    matches!(verb, "NOOP" | "EXPUNGE")
        || (verb == "UID" && matches!(sub_verb, "COPY" | "STORE" | "EXPUNGE"))
}

/// Answers IDLE, keeps it alive until DONE and returns the tagged reply;
/// empty if the connection ended first (the caller's next read then ends
/// the session).
async fn idle_reply<S: AsyncRead + AsyncWrite + Unpin>(
    io: &mut BufReader<S>,
    log: &Transcript,
    tag: &str,
    script: &Script,
) -> String {
    if !script.keepalive {
        return format!("{tag} BAD what\r\n");
    }
    if io.write_all(b"+ idling\r\n").await.is_ok() && keepalives_until_done(io, log).await {
        format!("{}{tag} OK idle done\r\n", script.before_idle_ok)
    } else {
        String::new()
    }
}

/// Sends `* OK Still here` every 100 ms until the client's DONE; false if
/// the connection ended first.
async fn keepalives_until_done<S: AsyncRead + AsyncWrite + Unpin>(
    io: &mut BufReader<S>,
    log: &Transcript,
) -> bool {
    loop {
        // fill_buf is cancel safe, unlike read_line.
        tokio::select! {
            buffered = io.fill_buf() => match buffered {
                Ok(b) if !b.is_empty() => break,
                _ => return false,
            },
            () = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                if io.write_all(b"* OK Still here\r\n").await.is_err() {
                    return false;
                }
            }
        }
    }
    line(io, true, log).await.is_some()
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
