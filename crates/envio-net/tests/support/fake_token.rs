//! A fake OAuth token endpoint: a TLS listener that answers each connection
//! from a script (a queue of steps) and records every request, with its form
//! body decoded.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::certs::Cert;

/// What the endpoint does with the next connection.
pub enum Step {
    /// `HTTP/1.0 <status>` with a JSON body, then a clean close.
    Reply(u16, String),
    /// Any bytes, then a clean close (HTML, an oversized body, a redirect).
    Raw(Vec<u8>),
    /// Read the request, then drop the socket with no reply.
    Cut,
    /// Read the request, then never answer; keep the socket open.
    Stall,
}

/// One request the endpoint received.
#[derive(Clone, Debug)]
pub struct Request {
    /// The request line without the version, e.g. `POST /token`.
    pub line: String,
    /// The Content-Type header's value.
    pub content_type: String,
    /// The form body, decoded, in order.
    pub form: Vec<(String, String)>,
}

impl Request {
    pub fn get(&self, name: &str) -> Option<&str> {
        self.form
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

#[derive(Clone, Default)]
pub struct Transcript(Arc<Mutex<Vec<Request>>>);

impl Transcript {
    pub fn requests(&self) -> Vec<Request> {
        self.0.lock().unwrap().clone()
    }
}

/// Serves `script` (one step per connection; when it runs out, `500`) on a
/// localhost port with `cert`. Returns the port and what it records.
pub async fn serve(cert: &Cert, script: Vec<Step>) -> (u16, Transcript) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let acceptor = cert.acceptor();
    let transcript = Transcript::default();
    let seen = transcript.clone();
    let script = Arc::new(Mutex::new(VecDeque::from(script)));
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let (acceptor, seen, script) = (acceptor.clone(), seen.clone(), script.clone());
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(tcp).await else {
                    return;
                };
                let Some(request) = read_request(&mut tls).await else {
                    return;
                };
                seen.0.lock().unwrap().push(request);
                let step = script.lock().unwrap().pop_front();
                match step.unwrap_or(Step::Reply(500, "{}".into())) {
                    Step::Reply(status, body) => {
                        let head = format!(
                            "HTTP/1.0 {status} X\r\nContent-Type: application/json\r\n\
                             Content-Length: {}\r\n\r\n",
                            body.len()
                        );
                        let _ = tls.write_all(head.as_bytes()).await;
                        let _ = tls.write_all(body.as_bytes()).await;
                        let _ = tls.shutdown().await;
                    }
                    Step::Raw(bytes) => {
                        let _ = tls.write_all(&bytes).await;
                        let _ = tls.shutdown().await;
                    }
                    Step::Cut => drop(tls),
                    Step::Stall => {
                        let mut sink = [0; 64];
                        while matches!(tls.read(&mut sink).await, Ok(n) if n > 0) {}
                    }
                }
            });
        }
    });
    (port, transcript)
}

async fn read_request(tls: &mut (impl AsyncRead + Unpin)) -> Option<Request> {
    let mut raw = Vec::new();
    let mut chunk = [0; 4096];
    let (head_end, length) = loop {
        let n = tls.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        raw.extend_from_slice(&chunk[..n]);
        if let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&raw[..end]).to_string();
            let length = header(&head, "content-length")?.parse::<usize>().ok()?;
            break (end + 4, length);
        }
    };
    while raw.len() < head_end + length {
        let n = tls.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        raw.extend_from_slice(&chunk[..n]);
    }
    let head = String::from_utf8_lossy(&raw[..head_end]).to_string();
    let body = String::from_utf8_lossy(&raw[head_end..head_end + length]).to_string();
    Some(Request {
        line: head.lines().next()?.rsplit_once(' ')?.0.to_owned(),
        content_type: header(&head, "content-type").unwrap_or_default(),
        form: body
            .split('&')
            .filter(|p| !p.is_empty())
            .map(|pair| {
                let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
                (decode(k), decode(v))
            })
            .collect(),
    })
}

fn header(head: &str, name: &str) -> Option<String> {
    head.lines().skip(1).find_map(|l| {
        let (k, v) = l.split_once(':')?;
        k.trim()
            .eq_ignore_ascii_case(name)
            .then(|| v.trim().to_owned())
    })
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' => {
                out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap());
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).unwrap()
}
