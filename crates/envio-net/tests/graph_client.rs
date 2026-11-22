//! The Graph client against the scripted TLS server, on the recorded
//! fixtures plus hand-written replies for what the recording lacks.

use std::cell::RefCell;
use std::rc::Rc;

use envio_net::NetError;
use envio_net::graph::mock::{Action, HOST, Server};
use envio_net::graph::model::DeltaNext;
use envio_net::graph::model::DeltaPage;
use envio_net::graph::{Bearer, GraphClient, Patch};
use futures::future::LocalBoxFuture;
use secrecy::SecretString;

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/graph/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(path).unwrap()
}

/// Hands out `tok-1`, `tok-2`, ... and notes which calls were forced.
#[derive(Clone, Default)]
struct Tokens(Rc<RefCell<Vec<bool>>>);

impl Bearer for Tokens {
    fn bearer(&self, force: bool) -> LocalBoxFuture<'_, envio_net::Result<SecretString>> {
        let n = {
            let mut calls = self.0.borrow_mut();
            calls.push(force);
            calls.len()
        };
        Box::pin(async move { Ok(SecretString::from(format!("tok-{n}"))) })
    }
}

fn client(server: &Server) -> (GraphClient, Tokens) {
    let tokens = Tokens::default();
    let c = GraphClient::with_host(
        HOST,
        server.port(),
        server.roots(),
        Box::new(tokens.clone()),
    );
    (c, tokens)
}

fn auth_of(server: &Server, n: usize) -> String {
    server.requests()[n]
        .header("authorization")
        .unwrap()
        .to_owned()
}

const NONE: &str = r#"{"value":[]}"#;

#[tokio::test]
async fn folders_walk_children_and_carry_the_token() {
    let server = Server::start(vec![
        Action::json(200, &fixture("folders.json")),
        Action::json(200, &fixture("folders_children.json")),
    ])
    .await;
    let (mut c, _) = client(&server);
    let list = c.folders().await.unwrap();
    assert!(
        list.iter()
            .any(|f| f.well_known.as_deref() == Some("inbox"))
    );
    let seen = server.requests();
    assert!(
        seen[0]
            .path
            .starts_with("/v1.0/me/mailFolders?includeHiddenFolders=true")
    );
    assert!(seen[1].path.contains("/childFolders?"), "{}", seen[1].path);
    assert_eq!(auth_of(&server, 0), "Bearer tok-1");
}

#[tokio::test]
async fn delta_pages_follow_the_link_and_the_first_asks_for_200() {
    let server = Server::start(vec![
        Action::json(200, &fixture("delta_first.json")),
        Action::json(200, &fixture("delta_final.json")),
    ])
    .await;
    let (mut c, _) = client(&server);
    let first = c.delta("AAMk-7", None).await.unwrap();
    let DeltaPage::Page {
        next: DeltaNext::More(link),
        ..
    } = first
    else {
        panic!("{first:?}");
    };
    let last = c.delta("AAMk-7", Some(&link)).await.unwrap();
    assert!(matches!(
        last,
        DeltaPage::Page {
            next: DeltaNext::Done(_),
            ..
        }
    ));
    let seen = server.requests();
    let prefer = seen[0].header("prefer").unwrap();
    assert!(prefer.contains("ImmutableId") && prefer.contains("odata.maxpagesize=200"));
    assert!(
        seen[1].path.contains("$skiptoken=fake-1"),
        "{}",
        seen[1].path
    );
    assert!(
        seen[1].path.starts_with("/v1.0/"),
        "the link's host is dropped"
    );
}

#[tokio::test]
async fn an_expired_delta_link_is_expired_for_410_and_for_the_code() {
    let gone = r#"{"error":{"code":"syncStateNotFound","message":"x"}}"#;
    let server = Server::start(vec![
        Action::json(410, gone),
        Action::json(400, gone),
        Action::json(400, &fixture("error_400.json")),
    ])
    .await;
    let (mut c, _) = client(&server);
    let l = "https://graph.microsoft.com/v1.0/me/mailFolders('a')/messages/delta?$deltatoken=x";
    assert_eq!(c.delta("a", Some(l)).await.unwrap(), DeltaPage::Expired);
    assert_eq!(c.delta("a", Some(l)).await.unwrap(), DeltaPage::Expired);
    let e = c.delta("a", Some(l)).await.unwrap_err();
    assert!(matches!(e, NetError::Refused(_)), "{e:?}");
}

#[tokio::test]
async fn a_401_gets_one_fresh_token_and_a_second_is_sign_in() {
    let server = Server::start(vec![
        Action::json(401, "{}"),
        Action::json(200, NONE),
        Action::json(401, "{}"),
        Action::json(401, "{}"),
    ])
    .await;
    let (mut c, tokens) = client(&server);
    c.search("f", "x").await.unwrap();
    assert_eq!(*tokens.0.borrow(), vec![false, true]);
    assert_eq!(auth_of(&server, 1), "Bearer tok-2");
    let e = c.search("f", "x").await.unwrap_err();
    assert!(matches!(e, NetError::SignIn(_)), "{e:?}");
    assert_eq!(*tokens.0.borrow(), vec![false, true, false, true]);
}

#[tokio::test]
async fn summaries_from_the_recorded_batch() {
    let server = Server::start(vec![Action::json(200, &fixture("batch.json"))]).await;
    let (mut c, _) = client(&server);
    let ids: Vec<String> = (1..=3).map(|n| format!("id{n}")).collect();
    let got = c.summaries(&ids).await.unwrap();
    assert_eq!(got.len(), 3);
    assert!(got.iter().all(|s| s.size > 0));
    let seen = server.requests();
    assert_eq!(seen[0].path, "/v1.0/$batch");
    let body = String::from_utf8(seen[0].body.clone()).unwrap();
    assert!(body.contains("Integer 0x0E08") && body.contains("ImmutableId"));
}

fn sub(id: &str, status: u16, headers: &str, body: &str) -> String {
    format!(r#"{{"id":"{id}","status":{status},"headers":{{{headers}}},"body":{body}}}"#)
}

fn msg(id: &str) -> String {
    format!(
        r#"{{"id":"{id}","receivedDateTime":"2026-11-21T22:39:44Z","singleValueExtendedProperties":[{{"id":"Integer 0xe08","value":"7"}}]}}"#
    )
}

fn batch_of(subs: &[String]) -> String {
    format!(r#"{{"responses":[{}]}}"#, subs.join(","))
}

#[tokio::test]
async fn summaries_match_by_id_and_drop_a_sub_404() {
    let replies = batch_of(&[
        sub("3", 200, "", &msg("c")),
        sub("2", 404, "", r#"{"error":{"code":"ErrorItemNotFound"}}"#),
        sub("1", 200, "", &msg("a")),
    ]);
    let server = Server::start(vec![Action::json(200, &replies)]).await;
    let (mut c, _) = client(&server);
    let ids = ["a", "b", "c"].map(String::from);
    let got = c.summaries(&ids).await.unwrap();
    let names: Vec<_> = got.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(names, ["a", "c"], "in request order, the 404 dropped");
}

#[tokio::test]
async fn a_sub_429_is_busy_for_the_whole_call() {
    let replies = batch_of(&[
        sub("1", 200, "", &msg("a")),
        sub("2", 429, r#""Retry-After":"3""#, "{}"),
    ]);
    let server = Server::start(vec![Action::json(200, &replies)]).await;
    let (mut c, _) = client(&server);
    let ids = ["a", "b"].map(String::from);
    let e = c.summaries(&ids).await.unwrap_err();
    assert!(matches!(e, NetError::Busy(_)), "{e:?}");
}

#[tokio::test]
async fn a_sub_401_gets_a_fresh_token_once() {
    let stale = batch_of(&[sub("1", 401, "", "{}")]);
    let good = batch_of(&[sub("1", 200, "", &msg("a"))]);
    let server = Server::start(vec![Action::json(200, &stale), Action::json(200, &good)]).await;
    let (mut c, tokens) = client(&server);
    let got = c.summaries(&["a".to_owned()]).await.unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(*tokens.0.borrow(), vec![false, true]);
}

#[tokio::test]
async fn summaries_go_out_twenty_at_a_time() {
    let one = |n: usize| sub(&n.to_string(), 200, "", &msg("m"));
    let first = batch_of(&(1..=20).map(one).collect::<Vec<_>>());
    let second = batch_of(&(1..=5).map(one).collect::<Vec<_>>());
    let server = Server::start(vec![Action::json(200, &first), Action::json(200, &second)]).await;
    let (mut c, _) = client(&server);
    let ids: Vec<String> = (0..25).map(|n| format!("m{n}")).collect();
    assert_eq!(c.summaries(&ids).await.unwrap().len(), 25);
    let sent = |n: usize| {
        let v: serde_json::Value = serde_json::from_slice(&server.requests()[n].body).unwrap();
        v["requests"].as_array().unwrap().len()
    };
    assert_eq!((sent(0), sent(1)), (20, 5));
}

#[tokio::test]
async fn send_mime_does_not_retry_a_504_and_calls_it_unconfirmed() {
    let server = Server::start(vec![
        Action::reply(504, &[("Retry-After", "1")], "{}"),
        Action::json(202, ""),
    ])
    .await;
    let (mut c, _) = client(&server);
    let e = c.send_mime(b"mime").await.unwrap_err();
    assert!(matches!(e, NetError::Unconfirmed(_)), "{e:?}");
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn send_mime_retries_a_503_with_a_short_retry_after_once() {
    let wait = [("Retry-After", "1")];
    let server = Server::start(vec![
        Action::reply(503, &wait, "{}"),
        Action::reply(503, &wait, "{}"),
        Action::json(202, ""),
    ])
    .await;
    let (mut c, _) = client(&server);
    let e = c.send_mime(b"mime").await.unwrap_err();
    assert!(matches!(e, NetError::Busy(_)), "{e:?}");
    assert_eq!(server.requests().len(), 2, "one retry, no more");
    assert!(
        server
            .requests()
            .iter()
            .all(|r| r.header("content-type") == Some("text/plain"))
    );
    assert_ne!(
        server.requests()[0].conn,
        server.requests()[1].conn,
        "fresh connections"
    );
}

#[tokio::test]
async fn send_mime_with_a_lost_reply_is_unconfirmed() {
    let server = Server::start(vec![Action::drop_after_request()]).await;
    let (mut c, _) = client(&server);
    let e = c.send_mime(b"mime").await.unwrap_err();
    assert!(matches!(e, NetError::Unconfirmed(_)), "{e:?}");
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn send_mime_is_base64_text_plain() {
    let server = Server::start(vec![Action::json(202, "")]).await;
    let (mut c, _) = client(&server);
    c.send_mime(b"Subject: x\r\n").await.unwrap();
    let seen = &server.requests()[0];
    assert_eq!(seen.path, "/v1.0/me/sendMail");
    assert_eq!(seen.body, b"U3ViamVjdDogeA0K");
}

#[tokio::test]
async fn create_mime_503_is_busy_with_no_retry() {
    let server = Server::start(vec![
        Action::reply(503, &[("Retry-After", "1")], "{}"),
        Action::json(201, r#"{"id":"new"}"#),
    ])
    .await;
    let (mut c, _) = client(&server);
    let e = c.create_mime("drafts", b"mime").await.unwrap_err();
    assert!(matches!(e, NetError::Busy(_)), "{e:?}");
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn create_mime_returns_the_new_id() {
    let server = Server::start(vec![Action::json(201, r#"{"id":"new-1"}"#)]).await;
    let (mut c, _) = client(&server);
    assert_eq!(c.create_mime("drafts", b"m").await.unwrap(), "new-1");
    assert_eq!(
        server.requests()[0].path,
        "/v1.0/me/mailFolders/drafts/messages"
    );
}

#[tokio::test]
async fn move_to_on_a_dead_reused_connection_is_busy_and_not_repeated() {
    let server = Server::start(vec![
        Action::json(200, r#"{"categories":["a"]}"#).then_close(),
        Action::json(201, r#"{"id":"m2"}"#),
    ])
    .await;
    let (mut c, _) = client(&server);
    c.get_categories("m1").await.unwrap();
    let e = c.move_to("m1", "archive").await.unwrap_err();
    assert!(e.is_temporary(), "{e:?}");
    assert_eq!(server.requests().len(), 1, "the move was never sent twice");
}

#[tokio::test]
async fn a_get_on_a_dead_reused_connection_is_repeated() {
    let server = Server::start(vec![
        Action::json(200, r#"{"categories":["a"]}"#).then_close(),
        Action::json(200, r#"{"categories":["b"]}"#),
    ])
    .await;
    let (mut c, _) = client(&server);
    c.get_categories("m1").await.unwrap();
    assert_eq!(
        c.get_categories("m1").await.unwrap(),
        Some(vec!["b".to_owned()])
    );
}

#[tokio::test]
async fn find_message_id_doubles_quotes_in_the_filter() {
    let server = Server::start(vec![Action::json(
        200,
        &fixture("find_message_id_inbox.json"),
    )])
    .await;
    let (mut c, _) = client(&server);
    let found = c
        .find_message_id("inbox", "<a'b@example.org>")
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
    let path = &server.requests()[0].path;
    assert!(
        path.contains("$filter=internetMessageId%20eq%20%27%3Ca%27%27b%40example.org%3E%27"),
        "{path}"
    );
}

#[tokio::test]
async fn find_message_id_finds_a_sent_copy() {
    let sent = r#"{"value":[{"id":"s1","receivedDateTime":"2026-11-21T22:39:44Z","isRead":true,"isDraft":false}]}"#;
    let server = Server::start(vec![Action::json(200, sent)]).await;
    let (mut c, _) = client(&server);
    let found = c
        .find_message_id("sentitems", "<m@example.org>")
        .await
        .unwrap();
    assert_eq!(found[0].id, "s1");
    assert!(found[0].is_read);
}

#[tokio::test]
async fn search_follows_all_pages_and_escapes_quotes() {
    let server = Server::start(vec![
        Action::json(200, &fixture("search_page1.json")),
        Action::json(200, &fixture("search_page2.json")),
    ])
    .await;
    let (mut c, _) = client(&server);
    let got = c.search("inbox", "say \"hi\"").await.unwrap();
    assert_eq!(got.len(), 2);
    let first = &server.requests()[0].path;
    assert!(
        first.contains("$search=%22say%20%5C%22hi%5C%22%22"),
        "{first}"
    );
    assert_eq!(server.requests().len(), 2);
}

#[tokio::test]
async fn a_message_that_is_gone_is_none_or_ok() {
    let gone = r#"{"error":{"code":"ErrorItemNotFound"}}"#;
    let server = Server::start(vec![
        Action::json(404, gone),
        Action::json(404, gone),
        Action::json(404, gone),
        Action::json(404, gone),
    ])
    .await;
    let (mut c, _) = client(&server);
    assert_eq!(c.message_mime("m").await.unwrap(), None);
    c.patch(
        "m",
        &Patch {
            read: Some(true),
            ..Patch::default()
        },
    )
    .await
    .unwrap();
    c.permanent_delete("m").await.unwrap();
    assert_eq!(c.move_to("m", "f").await.unwrap(), None);
}

#[tokio::test]
async fn patch_sends_only_what_changes() {
    let server = Server::start(vec![Action::json(200, "{}")]).await;
    let (mut c, _) = client(&server);
    let patch = Patch {
        read: Some(true),
        flagged: Some(false),
        categories: None,
    };
    c.patch("m 1", &patch).await.unwrap();
    let seen = &server.requests()[0];
    assert_eq!(
        (seen.method.as_str(), seen.path.as_str()),
        ("PATCH", "/v1.0/me/messages/m%201")
    );
    let v: serde_json::Value = serde_json::from_slice(&seen.body).unwrap();
    assert_eq!(
        v,
        serde_json::json!({"isRead": true, "flag": {"flagStatus": "notFlagged"}})
    );
}

#[tokio::test]
async fn a_get_retries_a_short_retry_after_once_and_a_long_one_not_at_all() {
    let server = Server::start(vec![
        Action::reply(429, &[("Retry-After", "1")], "{}"),
        Action::json(200, r#"{"categories":[]}"#),
        Action::reply(429, &[("Retry-After", "60")], "{}"),
    ])
    .await;
    let (mut c, _) = client(&server);
    assert_eq!(c.get_categories("m").await.unwrap(), Some(vec![]));
    let e = c.get_categories("m").await.unwrap_err();
    assert!(matches!(e, NetError::Busy(_)), "{e:?}");
    assert_eq!(server.requests().len(), 3);
}

#[tokio::test]
async fn summaries_repeat_on_a_dead_reused_connection() {
    let server = Server::start(vec![
        Action::json(200, r#"{"categories":["a"]}"#).then_close(),
        Action::json(200, &batch_of(&[sub("1", 200, "", &msg("m1"))])),
    ])
    .await;
    let (mut c, _) = client(&server);
    c.get_categories("m0").await.unwrap();
    let got = c.summaries(&["m1".to_owned()]).await.unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(server.requests().len(), 2);
    assert_eq!(server.requests()[1].path, "/v1.0/$batch");
    assert_eq!(server.accepts(), 2);
}

#[tokio::test]
async fn permanent_delete_repeats_on_a_dead_reused_connection() {
    let server = Server::start(vec![
        Action::json(200, r#"{"categories":["a"]}"#).then_close(),
        Action::json(204, ""),
    ])
    .await;
    let (mut c, _) = client(&server);
    c.get_categories("m0").await.unwrap();
    c.permanent_delete("m1").await.unwrap();
    assert_eq!(server.accepts(), 2);
}

#[tokio::test]
async fn move_to_is_not_repeated_after_a_5xx() {
    for status in [500, 502, 504] {
        let server = Server::start(vec![
            Action::reply(status, &[("Retry-After", "1")], "{}"),
            Action::json(404, "{}"),
        ])
        .await;
        let (mut c, _) = client(&server);
        let e = c.move_to("m1", "archive").await.unwrap_err();
        assert!(e.is_temporary(), "{status}: {e:?}");
        assert_eq!(server.requests().len(), 1, "{status}: no repeat");
    }
}

#[tokio::test]
async fn a_self_referencing_folder_is_listed_once() {
    let folder = r#"{"id":"loop","displayName":"Loop","childFolderCount":1,"parentFolderId":"p","totalItemCount":0,"unreadItemCount":0}"#;
    let page = format!(r#"{{"value":[{folder}]}}"#);
    let server = Server::start(vec![
        Action::json(200, &page),
        Action::json(200, &page),
        Action::json(200, &page),
    ])
    .await;
    let (mut c, _) = client(&server);
    let all = c.folders().await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(server.requests().len(), 2, "its children were read once");
}

fn folder_json(id: &str, kids: u32) -> String {
    format!(
        r#"{{"id":"{id}","displayName":"F","childFolderCount":{kids},"parentFolderId":"p","totalItemCount":0,"unreadItemCount":0}}"#
    )
}

fn folder_page(ids: impl Iterator<Item = String>, first_kids: u32) -> String {
    let list: Vec<String> = ids
        .enumerate()
        .map(|(i, id)| folder_json(&id, if i == 0 { first_kids } else { 0 }))
        .collect();
    format!(r#"{{"value":[{}]}}"#, list.join(","))
}

#[tokio::test]
async fn a_duplicate_child_at_the_folder_cap_is_not_an_error() {
    let top = folder_page((0..5000).map(|i| format!("f{i}")), 1);
    let kids = folder_page(std::iter::once("f1".to_owned()), 0);
    let server = Server::start(vec![Action::json(200, &top), Action::json(200, &kids)]).await;
    let (mut c, _) = client(&server);
    assert_eq!(c.folders().await.unwrap().len(), 5000);
}

#[tokio::test]
async fn top_level_folders_count_against_the_folder_cap() {
    let top = folder_page((0..5001).map(|i| format!("f{i}")), 0);
    let server = Server::start(vec![Action::json(200, &top)]).await;
    let (mut c, _) = client(&server);
    let e = c.folders().await.unwrap_err();
    assert!(matches!(e, NetError::Protocol(_)), "{e:?}");
}
