//! The Graph HTTP/1.1 client against the scripted TLS server: keep-alive,
//! the retry rules, framing and the reply caps.

use envio_net::NetError;
use envio_net::graph::http::{Http, Request, Sent};
use envio_net::graph::mock::{Action, HOST, Server};
use envio_net::tls::Roots;

fn get(path: &str) -> Request<'_> {
    Request {
        method: "GET",
        path,
        headers: vec![("Authorization", "Bearer t".into())],
        body: None,
    }
}

fn post(body: &[u8]) -> Request<'static> {
    Request {
        method: "POST",
        path: "/v1.0/me/sendMail",
        headers: vec![],
        body: Some(("text/plain", body.to_vec())),
    }
}

async fn client(server: &Server) -> Http {
    Http::new(HOST, server.port(), server.roots())
}

#[tokio::test]
async fn two_gets_share_one_connection() {
    let server = Server::start(vec![Action::json(200, "a"), Action::json(200, "b")]).await;
    let mut http = client(&server).await;
    assert_eq!(http.send(&get("/x"), false, true).await.unwrap().body, b"a");
    assert_eq!(http.send(&get("/y"), false, true).await.unwrap().body, b"b");
    assert_eq!(server.accepts(), 1);
    assert!(server.requests().iter().all(|r| r.conn == 0));
}

#[tokio::test]
async fn an_idle_connection_closed_by_the_server_is_retried_for_a_get() {
    let server = Server::start(vec![
        Action::json(200, "a").then_close(),
        Action::json(200, "b"),
    ])
    .await;
    let mut http = client(&server).await;
    http.send(&get("/x"), false, true).await.unwrap();
    let second = http.send(&get("/y"), false, true).await.unwrap();
    assert_eq!(second.body, b"b");
    assert_eq!(server.accepts(), 2);
    let seen = server.requests();
    assert_eq!(seen.len(), 2, "the dead connection read no second request");
    assert_eq!(seen[1].conn, 1);
}

#[tokio::test]
async fn a_post_on_a_dead_reused_connection_is_before_and_not_retried() {
    let server = Server::start(vec![
        Action::json(200, "a").then_close(),
        Action::json(200, "never used"),
    ])
    .await;
    let mut http = client(&server).await;
    http.send(&get("/x"), false, true).await.unwrap();
    let err = http.send(&post(b"mail"), false, false).await.unwrap_err();
    assert!(matches!(err, Sent::Before(_)), "{err:?}");
    assert_eq!(server.accepts(), 1);
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn fresh_opens_a_new_connection_even_when_one_is_idle() {
    let server = Server::start(vec![Action::json(200, "a"), Action::json(200, "b")]).await;
    let mut http = client(&server).await;
    http.send(&get("/x"), false, true).await.unwrap();
    assert!(http.is_connected());
    http.send(&post(b"m"), true, false).await.unwrap();
    assert_eq!(server.accepts(), 2);
    assert_eq!(server.requests()[1].conn, 1);
}

#[tokio::test]
async fn a_reply_lost_after_the_body_is_after() {
    let server = Server::start(vec![Action::drop_after_request()]).await;
    let mut http = client(&server).await;
    let err = http.send(&post(b"mail"), true, false).await.unwrap_err();
    assert!(matches!(err, Sent::After(_)), "{err:?}");
    assert_eq!(server.requests()[0].body, b"mail");
    assert!(!http.is_connected());
}

#[tokio::test]
async fn a_lost_reply_on_a_fresh_connection_is_not_retried_even_for_a_get() {
    // On a fresh connection a GET's loss is After too; nothing retries it.
    let server = Server::start(vec![Action::drop_after_request(), Action::json(200, "b")]).await;
    let mut http = client(&server).await;
    let err = http.send(&get("/x"), true, true).await.unwrap_err();
    assert!(matches!(err, Sent::After(_)), "{err:?}");
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn content_length_and_chunked_replies_follow_each_other() {
    let server = Server::start(vec![
        Action::chunked(200, &["{\"a\":", "1}"]),
        Action::reply(404, &[("Retry-After", "7")], "{}"),
        Action::chunked(200, &["z"]),
    ])
    .await;
    let mut http = client(&server).await;
    let first = http.send(&get("/1"), false, true).await.unwrap();
    assert_eq!((first.status, first.body), (200, b"{\"a\":1}".to_vec()));
    let second = http.send(&get("/2"), false, true).await.unwrap();
    assert_eq!(second.status, 404);
    assert_eq!(second.header("retry-after"), Some("7"));
    let third = http.send(&get("/3"), false, true).await.unwrap();
    assert_eq!(third.body, b"z");
    assert_eq!(server.accepts(), 1);
}

#[tokio::test]
async fn connection_close_drops_the_connection() {
    let server = Server::start(vec![
        Action::reply(200, &[("Connection", "close")], "a"),
        Action::json(200, "b"),
    ])
    .await;
    let mut http = client(&server).await;
    http.send(&get("/x"), false, true).await.unwrap();
    assert!(!http.is_connected());
    http.send(&get("/y"), false, false).await.unwrap();
    assert_eq!(server.accepts(), 2);
}

#[tokio::test]
async fn a_json_reply_over_the_cap_is_protocol_and_a_value_over_its_cap_is_refused() {
    let big = |n: usize| {
        Action::raw(format!("HTTP/1.1 200 OK\r\nContent-Length: {n}\r\n\r\n").into_bytes())
    };
    let server = Server::start(vec![
        big(4 * 1024 * 1024 + 1),
        big(64 * 1024 * 1024 + 1),
        big(64 * 1024 * 1024).then_close(),
    ])
    .await;
    let mut http = client(&server).await;
    let err = http.send(&get("/v1.0/me/messages"), false, true).await;
    assert!(
        matches!(err, Err(Sent::After(NetError::Protocol(_)))),
        "{err:?}"
    );
    let err = http
        .send(&get("/v1.0/me/messages/x/$value"), false, true)
        .await;
    assert!(
        matches!(err, Err(Sent::After(NetError::Refused(_)))),
        "{err:?}"
    );
    // Exactly at the cap is within it: the (missing) body is then an EOF.
    let err = http
        .send(&get("/v1.0/me/messages/x/$value"), false, true)
        .await;
    assert!(matches!(err, Err(Sent::After(NetError::Io(_)))), "{err:?}");
}

#[tokio::test]
async fn a_certificate_the_roots_do_not_trust_is_a_tls_error() {
    let server = Server::start(vec![Action::json(200, "a")]).await;
    let mut http = Http::new(HOST, server.port(), Roots::empty());
    let err = http.send(&get("/x"), false, true).await.unwrap_err();
    assert!(matches!(err, Sent::Before(NetError::Tls(_))), "{err:?}");
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn every_request_carries_the_immutable_id_preference() {
    let server = Server::start(vec![
        Action::json(200, "a"),
        Action::json(200, "b"),
        Action::json(200, "c"),
    ])
    .await;
    let mut http = client(&server).await;
    let mut paged = get("/x");
    paged
        .headers
        .push(("Prefer", "odata.maxpagesize=200".into()));
    http.send(&paged, false, true).await.unwrap();
    http.send(&get("/y"), false, true).await.unwrap();
    http.send(&post(b"m"), false, false).await.unwrap();
    let seen = server.requests();
    assert_eq!(
        seen[0].header("prefer"),
        Some("IdType=\"ImmutableId\", odata.maxpagesize=200")
    );
    for r in &seen {
        assert!(
            r.header("prefer")
                .unwrap()
                .starts_with("IdType=\"ImmutableId\"")
        );
        assert_eq!(r.header("connection"), Some("keep-alive"));
        assert_eq!(r.header("accept"), Some("application/json"));
        assert!(r.header("host").unwrap().starts_with("localhost:"));
    }
}

#[tokio::test]
async fn a_closed_connection_with_no_reply_is_after() {
    // The script is empty: the mock reads the request and closes.
    let server = Server::start(vec![]).await;
    let mut http = client(&server).await;
    let err = http.send(&post(b"m"), true, false).await.unwrap_err();
    assert!(matches!(err, Sent::After(_)), "{err:?}");
}
