//! The trust rules end to end, against an in-process TLS server.

mod support;

use envio_net::{Endpoint, Fingerprint, NetError, Roots, Security, Untrusted};
use support::certs::{Ca, Cert};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

/// Serves TLS with `cert` on a free port; each connection gets "ok".
async fn serve(cert: &Cert) -> u16 {
    serve_with(cert.acceptor()).await
}

/// Serves TLS with `acceptor` on a free port; each connection gets "ok".
async fn serve_with(acceptor: TlsAcceptor) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                if let Ok(mut tls) = acceptor.accept(tcp).await {
                    let _ = tls.write_all(b"ok").await;
                    let _ = tls.shutdown().await;
                }
            });
        }
    });
    port
}

fn endpoint(port: u16, pin: Option<Fingerprint>) -> Endpoint {
    Endpoint {
        pin,
        ..Endpoint::new("localhost", port, Security::Tls)
    }
}

async fn read_all(endpoint: &Endpoint, roots: &Roots) -> Result<String, NetError> {
    let mut tls = envio_net::tls::connect(endpoint, roots).await?;
    let mut text = String::new();
    tls.read_to_string(&mut text).await?;
    Ok(text)
}

#[tokio::test]
async fn a_self_signed_server_asks_the_user_with_its_fingerprint() {
    let cert = Cert::self_signed("localhost");
    let port = serve(&cert).await;
    let err = read_all(&endpoint(port, None), &Roots::empty())
        .await
        .unwrap_err();
    match err {
        NetError::Untrusted { presented, reason } => {
            assert_eq!(presented, Fingerprint::of(&cert.der));
            assert_eq!(reason, Untrusted::UnknownIssuer);
        }
        other => panic!("expected Untrusted, got {other:?}"),
    }
}

#[tokio::test]
async fn an_accepted_fingerprint_connects() {
    let cert = Cert::self_signed("localhost");
    let port = serve(&cert).await;
    let pin = Some(Fingerprint::of(&cert.der));
    let text = read_all(&endpoint(port, pin), &Roots::empty())
        .await
        .unwrap();
    assert_eq!(text, "ok");
}

#[tokio::test]
async fn a_pin_ignores_the_host_name() {
    // The user accepted this exact certificate; its name doesn't matter.
    let cert = Cert::self_signed("mail.example.invalid");
    let port = serve(&cert).await;
    let pin = Some(Fingerprint::of(&cert.der));
    let text = read_all(&endpoint(port, pin), &Roots::empty())
        .await
        .unwrap();
    assert_eq!(text, "ok");
}

#[tokio::test]
async fn a_changed_certificate_on_a_pinned_server_stops() {
    let old = Cert::self_signed("localhost");
    let new = Cert::self_signed("localhost");
    let port = serve(&new).await;
    let pin = Fingerprint::of(&old.der);
    let err = read_all(&endpoint(port, Some(pin)), &Roots::empty())
        .await
        .unwrap_err();
    match err {
        NetError::PinChanged { pinned, presented } => {
            assert_eq!(pinned, pin);
            assert_eq!(presented, Fingerprint::of(&new.der));
        }
        other => panic!("expected PinChanged, got {other:?}"),
    }
}

#[tokio::test]
async fn a_pin_wins_even_over_a_certificate_the_roots_trust() {
    let ca = Ca::new();
    let cert = ca.sign("localhost");
    let port = serve(&cert).await;
    let stale = Fingerprint::from_bytes([0; 32]);
    let err = read_all(&endpoint(port, Some(stale)), &Roots::from_der([ca.der()]))
        .await
        .unwrap_err();
    assert!(matches!(err, NetError::PinChanged { .. }), "{err:?}");
}

#[tokio::test]
async fn a_certificate_from_a_trusted_ca_connects_without_a_pin() {
    let ca = Ca::new();
    let port = serve(&ca.sign("localhost")).await;
    let text = read_all(&endpoint(port, None), &Roots::from_der([ca.der()]))
        .await
        .unwrap();
    assert_eq!(text, "ok");
}

#[tokio::test]
async fn a_trusted_certificate_for_another_name_asks_the_user() {
    let ca = Ca::new();
    let port = serve(&ca.sign("other.example.invalid")).await;
    let err = read_all(&endpoint(port, None), &Roots::from_der([ca.der()]))
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            NetError::Untrusted {
                reason: Untrusted::WrongName,
                ..
            }
        ),
        "{err:?}"
    );
}

#[tokio::test]
async fn the_system_roots_load_or_come_back_empty() {
    // Loaded or empty (under confinement), a self-signed server still goes
    // to the prompt rather than failing some other way.
    let roots = Roots::system();
    let cert = Cert::self_signed("localhost");
    let port = serve(&cert).await;
    let err = read_all(&endpoint(port, None), &roots).await.unwrap_err();
    assert!(matches!(err, NetError::Untrusted { .. }), "{err:?}");
}

#[tokio::test]
async fn a_pinned_certificate_with_a_forged_signature_fails_the_handshake() {
    // The server shows the pinned certificate but signs with another key.
    // The pin matches, so only the signature check can catch this.
    let cert = Cert::self_signed("localhost");
    let donor = Cert::self_signed("localhost");
    let port = serve_with(cert.acceptor_with_key_of(&donor)).await;
    let pin = Some(Fingerprint::of(&cert.der));
    let err = read_all(&endpoint(port, pin), &Roots::empty())
        .await
        .unwrap_err();
    assert!(
        !matches!(
            err,
            NetError::PinChanged { .. } | NetError::Untrusted { .. }
        ),
        "{err:?}"
    );
    assert!(matches!(err, NetError::Tls(_)), "{err:?}");
}

#[tokio::test]
async fn a_trusted_chain_with_a_forged_signature_fails_the_handshake() {
    // The chain checks out, but the server cannot prove it holds the key.
    let ca = Ca::new();
    let cert = ca.sign("localhost");
    let donor = Cert::self_signed("localhost");
    let port = serve_with(cert.acceptor_with_key_of(&donor)).await;
    let err = read_all(&endpoint(port, None), &Roots::from_der([ca.der()]))
        .await
        .unwrap_err();
    assert!(matches!(err, NetError::Tls(_)), "{err:?}");
}

#[tokio::test]
async fn a_self_signed_server_under_another_ca_is_an_unknown_issuer() {
    let other = Ca::new();
    let port = serve(&Cert::self_signed("localhost")).await;
    let err = read_all(&endpoint(port, None), &Roots::from_der([other.der()]))
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            NetError::Untrusted {
                reason: Untrusted::UnknownIssuer,
                ..
            }
        ),
        "{err:?}"
    );
}

#[tokio::test]
async fn an_expired_certificate_connects_when_its_pin_matches() {
    // Decision 3: the user accepted this exact certificate, so its dates
    // don't matter.
    let cert = Cert::self_signed_expired("localhost");
    let port = serve(&cert).await;
    let pin = Some(Fingerprint::of(&cert.der));
    let text = read_all(&endpoint(port, pin), &Roots::empty())
        .await
        .unwrap();
    assert_eq!(text, "ok");
}

#[tokio::test]
async fn an_expired_ca_signed_certificate_without_a_pin_asks_the_user() {
    let ca = Ca::new();
    let port = serve(&ca.sign_expired("localhost")).await;
    let err = read_all(&endpoint(port, None), &Roots::from_der([ca.der()]))
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            NetError::Untrusted {
                reason: Untrusted::Expired,
                ..
            }
        ),
        "{err:?}"
    );
}
