//! The HTTPS fetch behind autoconfig, against an in-process TLS server
//! that answers per request path.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use envio_net::Roots;
use envio_net::autoconfig::{Internet, Lookups};
use support::certs::Ca;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// The body cap in `autoconfig::http` (MAX_BODY). That module is private,
/// so the value is repeated here.
const CAP: usize = 256 * 1024;

const CONFIG: &str = "<clientConfig version=\"1.1\"><emailProvider id=\"x\">\
     <incomingServer type=\"imap\"><hostname>x.example.org</hostname><port>993</port>\
     <socketType>SSL</socketType></incomingServer></emailProvider></clientConfig>";

enum Action {
    /// Write the bytes, then close.
    Send(Vec<u8>),
    /// Write the bytes, then drop the socket without close_notify.
    Drop(Vec<u8>),
    /// Write the bytes, then keep the connection open until the client goes.
    Hold(Vec<u8>),
    /// Never answer; keep the connection open until the client goes.
    Stall,
}

fn ok(body: &str) -> Action {
    Action::Send(
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .into_bytes(),
    )
}

fn redirect(to: &str) -> Action {
    Action::Send(format!("HTTP/1.1 301 Moved\r\nLocation: {to}\r\n\r\n").into_bytes())
}

fn not_found() -> Action {
    Action::Send(b"HTTP/1.1 404 Not Found\r\n\r\n".to_vec())
}

/// Binds an HTTPS listener on localhost. Pass it to `serve`; the port is
/// known first so handlers can name themselves in redirects.
async fn bind() -> (TcpListener, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    (listener, port)
}

/// Serves `listener` with a certificate for localhost. `handler` gets the
/// request path and picks the action.
fn serve(listener: TcpListener, ca: &Ca, handler: impl Fn(&str) -> Action + Send + Sync + 'static) {
    let acceptor = ca.sign("localhost").acceptor();
    let handler = Arc::new(handler);
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let acceptor = acceptor.clone();
            let handler = handler.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(tcp).await else {
                    return;
                };
                let mut buf = vec![0; 4096];
                let n = tls.read(&mut buf).await.unwrap_or(0);
                let path = String::from_utf8_lossy(&buf[..n])
                    .split(' ')
                    .nth(1)
                    .unwrap_or("")
                    .to_owned();
                match handler(&path) {
                    Action::Send(bytes) => {
                        let _ = tls.write_all(&bytes).await;
                        let _ = tls.shutdown().await;
                    }
                    Action::Drop(bytes) => {
                        let _ = tls.write_all(&bytes).await;
                        let _ = tls.flush().await;
                        drop(tls);
                    }
                    Action::Hold(bytes) => {
                        let _ = tls.write_all(&bytes).await;
                        until_closed(&mut tls).await;
                    }
                    Action::Stall => until_closed(&mut tls).await,
                }
            });
        }
    });
}

async fn until_closed(tls: &mut (impl AsyncRead + Unpin)) {
    let mut sink = [0; 64];
    while matches!(tls.read(&mut sink).await, Ok(n) if n > 0) {}
}

#[tokio::test]
async fn the_body_comes_back_from_a_trusted_host_only() {
    let ca = Ca::new();
    let (listener, port) = bind().await;
    serve(listener, &ca, |path| match path {
        "/config" => ok(CONFIG),
        _ => not_found(),
    });
    let trusted = Internet::new(Roots::from_der([ca.der()]));
    let base = format!("https://localhost:{port}");
    assert_eq!(
        trusted.fetch(&format!("{base}/config")).await.as_deref(),
        Some(CONFIG)
    );
    assert_eq!(trusted.fetch(&format!("{base}/missing")).await, None);

    // Autoconfig has no trust prompt: an unknown certificate is no answer.
    let untrusted = Internet::new(Roots::empty());
    assert_eq!(untrusted.fetch(&format!("{base}/config")).await, None);
}

#[tokio::test]
async fn a_redirect_to_localhost_is_refused() {
    let ca = Ca::new();
    let (listener, port) = bind().await;
    let config_hits = Arc::new(AtomicUsize::new(0));
    let hits = config_hits.clone();
    serve(listener, &ca, move |path| match path {
        "/moved" => redirect(&format!("https://localhost:{port}/config")),
        "/config" => {
            hits.fetch_add(1, Ordering::SeqCst);
            ok(CONFIG)
        }
        _ => not_found(),
    });
    let trusted = Internet::new(Roots::from_der([ca.der()]));
    let moved = format!("https://localhost:{port}/moved");
    assert_eq!(trusted.fetch(&moved).await, None);
    assert_eq!(config_hits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_redirect_to_plain_http_is_refused() {
    let ca = Ca::new();
    let (listener, port) = bind().await;
    serve(listener, &ca, |path| match path {
        "/moved" => redirect("http://autoconfig.example.org/config"),
        _ => not_found(),
    });
    let trusted = Internet::new(Roots::from_der([ca.der()]));
    assert_eq!(
        trusted
            .fetch(&format!("https://localhost:{port}/moved"))
            .await,
        None
    );
}

#[tokio::test]
async fn a_body_is_capped_at_256_kib() {
    let ca = Ca::new();
    let (listener, port) = bind().await;
    serve(listener, &ca, |path| match path {
        "/exact" => ok(&"a".repeat(CAP)),
        "/over" => ok(&"a".repeat(CAP + 1)),
        "/over-without-length" => {
            Action::Send(format!("HTTP/1.0 200 OK\r\n\r\n{}", "a".repeat(CAP + 1)).into_bytes())
        }
        _ => not_found(),
    });
    let trusted = Internet::new(Roots::from_der([ca.der()]));
    let base = format!("https://localhost:{port}");
    assert_eq!(
        trusted
            .fetch(&format!("{base}/exact"))
            .await
            .map(|b| b.len()),
        Some(CAP)
    );
    assert_eq!(trusted.fetch(&format!("{base}/over")).await, None);
    assert_eq!(
        trusted.fetch(&format!("{base}/over-without-length")).await,
        None
    );
}

#[tokio::test]
async fn a_chunked_reply_is_refused() {
    let ca = Ca::new();
    let (listener, port) = bind().await;
    serve(listener, &ca, |path| {
        match path {
        "/chunked" => Action::Send(
            format!(
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{CONFIG}\r\n0\r\n\r\n",
                CONFIG.len()
            )
            .into_bytes(),
        ),
        _ => not_found(),
    }
    });
    let trusted = Internet::new(Roots::from_der([ca.der()]));
    assert_eq!(
        trusted
            .fetch(&format!("https://localhost:{port}/chunked"))
            .await,
        None
    );
}

#[tokio::test]
async fn a_reply_ends_at_its_declared_length() {
    // The server keeps the connection open after the whole reply.
    let ca = Ca::new();
    let (listener, port) = bind().await;
    serve(listener, &ca, |path| match path {
        "/held" => Action::Hold(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{CONFIG}",
                CONFIG.len()
            )
            .into_bytes(),
        ),
        _ => not_found(),
    });
    let trusted = Internet::with_fetch_limit(Roots::from_der([ca.der()]), Duration::from_secs(2));
    let started = Instant::now();
    let body = trusted
        .fetch(&format!("https://localhost:{port}/held"))
        .await;
    assert_eq!(body.as_deref(), Some(CONFIG));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn a_stalled_server_is_cut_off_by_the_fetch_limit() {
    let ca = Ca::new();
    let (listener, port) = bind().await;
    serve(listener, &ca, |path| match path {
        "/stall" => Action::Stall,
        _ => not_found(),
    });
    let trusted =
        Internet::with_fetch_limit(Roots::from_der([ca.der()]), Duration::from_millis(300));
    let started = Instant::now();
    assert_eq!(
        trusted
            .fetch(&format!("https://localhost:{port}/stall"))
            .await,
        None
    );
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn a_body_cut_off_without_close_notify_is_refused() {
    // No Content-Length, so only a clean end of stream says the body is
    // whole; the socket dropped mid-body is not one.
    let ca = Ca::new();
    let (listener, port) = bind().await;
    serve(listener, &ca, |path| match path {
        "/cut" => Action::Drop(format!("HTTP/1.0 200 OK\r\n\r\n{}", &CONFIG[..40]).into_bytes()),
        "/whole" => Action::Send(format!("HTTP/1.0 200 OK\r\n\r\n{CONFIG}").into_bytes()),
        _ => not_found(),
    });
    let trusted = Internet::with_fetch_limit(Roots::from_der([ca.der()]), Duration::from_secs(2));
    assert_eq!(
        trusted
            .fetch(&format!("https://localhost:{port}/cut"))
            .await,
        None
    );
    // A clean close still delivers a length-less body.
    let whole = trusted
        .fetch(&format!("https://localhost:{port}/whole"))
        .await;
    assert_eq!(whole.as_deref(), Some(CONFIG));
}
