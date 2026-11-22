//! `GraphServer` as a `MailSender`: the Bcc header, and what each answer
//! means to the Outbox. Example.org addresses only.

mod support;

use base64::Engine;
use envio_net::NetError;
use envio_net::graph::mock::{Action, Server};
use envio_sync::server::MailSender;
use support::graph::graph_server;
use support::local::Store;

const EML: &str = "From: me@example.org\r\nTo: a@example.org\r\nSubject: hi\r\n\r\nbody\r\n";

fn to() -> Vec<String> {
    vec!["a@example.org".into(), "hidden@example.org".into()]
}

#[tokio::test]
async fn send_posts_the_mime_with_the_bcc_header() {
    let server = Server::start(vec![Action::json(202, "")]).await;
    let store = Store::on_file();
    let mut gs = graph_server(&server, &store.db_path);
    gs.send("me@example.org", &to(), EML.as_bytes())
        .await
        .unwrap();
    let seen = server.requests();
    assert_eq!(seen.len(), 1);
    assert_eq!(
        (seen[0].method.as_str(), seen[0].path.as_str()),
        ("POST", "/v1.0/me/sendMail")
    );
    let raw = base64::engine::general_purpose::STANDARD
        .decode(&seen[0].body)
        .unwrap();
    let mime = String::from_utf8(raw).unwrap();
    assert!(mime.contains("Subject: hi\r\nBcc: hidden@example.org\r\n\r\nbody"));
}

#[tokio::test]
async fn a_gateway_timeout_may_have_sent_it() {
    let server = Server::start(vec![Action::json(504, "")]).await;
    let store = Store::on_file();
    let mut gs = graph_server(&server, &store.db_path);
    let e = gs
        .send("me@example.org", &to(), EML.as_bytes())
        .await
        .unwrap_err();
    assert!(e.unconfirmed, "{:?}", e.error);
    assert_eq!(server.requests().len(), 1, "a send is never repeated");
}

#[tokio::test]
async fn a_refusal_is_confirmed_not_sent() {
    let body = r#"{"error":{"code":"ErrorInvalidRecipients","message":"bad"}}"#;
    let server = Server::start(vec![Action::json(400, body)]).await;
    let store = Store::on_file();
    let mut gs = graph_server(&server, &store.db_path);
    let e = gs
        .send("me@example.org", &to(), EML.as_bytes())
        .await
        .unwrap_err();
    assert!(!e.unconfirmed);
    assert!(matches!(e.error, NetError::Refused(_)), "{:?}", e.error);
}

#[tokio::test]
async fn throttling_with_a_long_wait_is_busy_and_queued() {
    let server = Server::start(vec![Action::reply(503, &[("Retry-After", "600")], "")]).await;
    let store = Store::on_file();
    let mut gs = graph_server(&server, &store.db_path);
    let e = gs
        .send("me@example.org", &to(), EML.as_bytes())
        .await
        .unwrap_err();
    assert!(!e.unconfirmed);
    assert!(matches!(e.error, NetError::Busy(_)), "{:?}", e.error);
    assert_eq!(server.requests().len(), 1);
}
