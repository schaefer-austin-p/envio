//! The network dropping out mid-command, against Stalwart through a
//! cutting proxy. Every cut must be a temporary error, and the retry the
//! caller makes must leave exactly one copy of the message.

mod support;

use std::time::{Duration, Instant};

use envio_net::imap::ImapSession;
use envio_net::smtp::SmtpSession;
use envio_net::{Credentials, Endpoint, NetError, Roots, Security};
use support::proxy::{Direction, Proxy};
use support::stalwart::{ALICE, BOB, Stalwart, alice, bob, message, message_id};

/// The IMAPS endpoint, reached through `proxy`.
fn through(server: &Stalwart, proxy: &Proxy) -> Endpoint {
    Endpoint {
        host: "127.0.0.1".into(),
        ..server.endpoint(proxy.port, Security::Tls)
    }
}

async fn count(server: &Stalwart, credentials: &Credentials, folder: &str, id: &str) -> usize {
    let mut session = server.imap(credentials).await;
    session.select(folder).await.unwrap();
    let n = session.search_message_id(id).await.unwrap().len();
    session.logout().await.unwrap();
    n
}

/// What sync does after a failed APPEND: look before uploading again.
async fn append_unless_stored(server: &Stalwart, id: &str, eml: &[u8]) {
    if count(server, &alice(), "Drafts", id).await == 0 {
        let mut retry = server.imap(&alice()).await;
        retry.append("Drafts", &[], eml).await.unwrap();
        retry.logout().await.unwrap();
    }
}

#[tokio::test]
async fn a_download_cut_halfway_is_temporary_and_the_refetch_is_whole() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let id = message_id();
    let eml = message(&id, "Big download", 200_000);
    let mut direct = server.imap(&alice()).await;
    direct.append("INBOX", &[], &eml).await.unwrap();
    direct.select("INBOX").await.unwrap();
    let uid = direct.search_message_id(&id).await.unwrap()[0];

    let proxy = Proxy::start(&server.host, server.imaps).await;
    let mut cut = ImapSession::open(&through(&server, &proxy), &Roots::empty(), &alice())
        .await
        .unwrap();
    cut.select("INBOX").await.unwrap();
    proxy.cut_after(Direction::Down, 50_000);
    let err = cut.fetch_body(uid, eml.len()).await.unwrap_err();
    assert!(
        err.is_temporary() && matches!(err, NetError::Io(_)),
        "{err:?}"
    );

    assert_eq!(
        direct.fetch_body(uid, eml.len()).await.unwrap().unwrap(),
        eml
    );
    direct.logout().await.unwrap();
}

#[tokio::test]
async fn an_upload_cut_halfway_is_checked_before_it_is_retried() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let id = message_id();
    let eml = message(&id, "Big upload", 200_000);
    let proxy = Proxy::start(&server.host, server.imaps).await;
    let mut cut = ImapSession::open(&through(&server, &proxy), &Roots::empty(), &alice())
        .await
        .unwrap();
    proxy.cut_after(Direction::Up, 50_000);
    let err = cut.append("Drafts", &[], &eml).await.unwrap_err();
    assert!(
        err.is_temporary() && matches!(err, NetError::Io(_)),
        "{err:?}"
    );

    // The cut came inside the literal, so the server never stored it.
    assert_eq!(count(&server, &alice(), "Drafts", &id).await, 0);
    append_unless_stored(&server, &id, &eml).await;
    assert_eq!(count(&server, &alice(), "Drafts", &id).await, 1);
}

#[tokio::test]
async fn an_upload_cut_after_the_server_stored_it_is_not_stored_twice() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let id = message_id();
    let eml = message(&id, "Stored, reply lost", 200_000);
    let proxy = Proxy::start(&server.host, server.imaps).await;
    let mut cut = ImapSession::open(&through(&server, &proxy), &Roots::empty(), &alice())
        .await
        .unwrap();
    // Once the whole message has gone up, the next thing the server sends
    // is its tagged OK, sent after it stored the message. Cut that: the
    // server has the message but the client never sees the OK. Anything
    // the server sent earlier (the "+", a TLS session ticket) came while
    // fewer bytes had gone up, so it passes and cannot shift the cut.
    proxy.cut_down_after_up(eml.len());
    let err = cut.append("Drafts", &[], &eml).await.unwrap_err();
    assert!(
        err.is_temporary() && matches!(err, NetError::Io(_)),
        "{err:?}"
    );

    // The server committed the message, so the retry must not upload it.
    assert_eq!(count(&server, &alice(), "Drafts", &id).await, 1);
    append_unless_stored(&server, &id, &eml).await;
    assert_eq!(count(&server, &alice(), "Drafts", &id).await, 1);
}

#[tokio::test]
async fn smtp_cut_during_data_sends_nothing_and_the_resend_lands_once() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let id = message_id();
    let eml = message(&id, "Cut in DATA", 200_000);
    let proxy = Proxy::start(&server.host, server.submissions).await;
    let endpoint = Endpoint {
        host: "127.0.0.1".into(),
        ..server.endpoint(proxy.port, Security::Tls)
    };
    let mut cut = SmtpSession::connect(&endpoint, &Roots::empty())
        .await
        .unwrap();
    cut.login(&alice()).await.unwrap();
    proxy.cut_after(Direction::Up, 50_000);
    let err = cut.send(ALICE, &[BOB.to_owned()], &eml).await.unwrap_err();
    assert!(
        err.is_temporary() && matches!(err, NetError::Io(_)),
        "{err:?}"
    );

    // The cut came before the end of DATA, so nothing may be delivered.
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(count(&server, &bob(), "INBOX", &id).await, 0);

    let direct = server.endpoint(server.submissions, Security::Tls);
    let mut smtp = SmtpSession::connect(&direct, &Roots::empty())
        .await
        .unwrap();
    smtp.login(&alice()).await.unwrap();
    smtp.send(ALICE, &[BOB.to_owned()], &eml).await.unwrap();
    smtp.quit().await.unwrap();

    // Delivery is asynchronous, so poll, bounded.
    let deadline = Instant::now() + Duration::from_secs(10);
    while count(&server, &bob(), "INBOX", &id).await == 0 && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    // A late duplicate from the cut attempt would show up now.
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(count(&server, &bob(), "INBOX", &id).await, 1);
}
