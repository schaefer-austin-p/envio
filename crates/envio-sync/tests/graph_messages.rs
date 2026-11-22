//! `GraphServer` messages: summaries, flags and categories, moves and
//! deletes, drafts, and the searches. Example.org addresses only.

mod support;

use envio_net::NetError;
use envio_net::graph::mock::Action;
use envio_net::imap::Flag;
use envio_sync::newmail::{NewRow, count};
use envio_sync::server::MailServer;
use support::graph::{
    Synced, batch, categories, list, msg, page, requests_with, sub_gone, sub_ok, synced,
};

const HIGH: u32 = 1 << 31;
const T1: &str = "2026-11-21T10:00:00Z";
const T2: &str = "2026-11-21T11:00:00Z";

async fn two() -> Synced {
    synced(&[msg("m1", T1, false), msg("m2", T2, true)]).await
}

fn patches(s: &Synced) -> Vec<serde_json::Value> {
    s.server
        .requests()
        .into_iter()
        .filter(|r| r.method == "PATCH")
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect()
}

#[tokio::test]
async fn searches_answer_from_the_map() {
    let mut s = two().await;
    assert_eq!(s.gs.search_all().await.unwrap(), [HIGH, HIGH + 1]);
    // 10:30 UTC
    assert_eq!(s.gs.search_since(1_795_257_000).await.unwrap(), [HIGH + 1]);
}

#[tokio::test]
async fn summaries_rebuild_headers_that_envio_mime_parses() {
    let mut s = two().await;
    s.server.push(batch(&[
        sub_ok(1, "m1", T1, 2285, "Subject 1", "<fake-1@example.org>"),
        sub_gone(2),
    ]));
    let got = s.gs.fetch_summaries(&[HIGH, HIGH + 1]).await.unwrap();
    assert_eq!(got.len(), 1, "the gone message is left out");
    let one = &got[0];
    assert_eq!(
        (one.uid, one.size, one.internal_date),
        (HIGH, 2285, Some(1_795_255_200))
    );
    assert!(!one.flags.seen && one.modseq.is_some());
    assert!(one.header.ends_with(b"\r\n\r\n"));
    let env = envio_mime::envelope(&one.header).unwrap();
    assert_eq!(env.subject, "Subject 1");
    assert_eq!(env.message_id.as_deref(), Some("fake-1@example.org"));
    assert_eq!(env.from[0].email, "alice@example.org");
    assert!(env.date.is_some());
}

#[tokio::test]
async fn flags_come_from_the_map_and_changed_since_filters_by_modseq() {
    let mut s = two().await;
    let all = s.gs.fetch_flags(&[HIGH, HIGH + 1], None).await.unwrap();
    assert_eq!(
        all.iter()
            .map(|u| (u.uid, u.flags.seen, u.flags.answered))
            .collect::<Vec<_>>(),
        [(HIGH, false, false), (HIGH + 1, true, false)]
    );
    let seen = s.map.row(s.inbox, HIGH + 1).unwrap().unwrap().modseq;
    s.server
        .push(page(&[r#"{"id":"m1","isRead":true}"#.into()], false, "d2"));
    s.gs.refresh("INBOX").await.unwrap();
    let changed =
        s.gs.fetch_flags(&[HIGH, HIGH + 1], Some(seen))
            .await
            .unwrap();
    assert_eq!(changed.len(), 1);
    assert!(changed[0].uid == HIGH && changed[0].flags.seen);
}

#[tokio::test]
async fn a_body_downloads_whole() {
    let mut s = two().await;
    s.server.push(Action::json(200, "hello"));
    assert_eq!(
        s.gs.fetch_body(HIGH, 5).await.unwrap().as_deref(),
        Some(&b"hello"[..])
    );
    assert!(s.gs.fetch_partial(HIGH).await.is_err());
}

#[tokio::test]
async fn seen_and_flagged_are_one_patch() {
    let mut s = two().await;
    s.server.push(Action::json(200, "{}"));
    s.gs.store_flags(&[HIGH], true, &[Flag::Seen, Flag::Flagged])
        .await
        .unwrap();
    let p = patches(&s);
    assert_eq!(p.len(), 1);
    assert_eq!(p[0]["isRead"], true);
    assert_eq!(p[0]["flag"]["flagStatus"], "flagged");
    assert!(p[0].get("categories").is_none());
}

#[tokio::test]
async fn keywords_are_categories_read_merged_and_written() {
    let mut s = two().await;
    s.server.push(categories(&["A", "B"]));
    s.server.push(Action::json(200, "{}"));
    s.gs.store_flags(&[HIGH], true, &[Flag::Keyword("C".into())])
        .await
        .unwrap();
    s.server.push(categories(&["A", "B", "C"]));
    s.server.push(Action::json(200, "{}"));
    s.gs.store_flags(&[HIGH], false, &[Flag::Keyword("B".into())])
        .await
        .unwrap();
    let p = patches(&s);
    assert_eq!(p[0]["categories"], serde_json::json!(["A", "B", "C"]));
    assert_eq!(p[1]["categories"], serde_json::json!(["A", "C"]));
    assert!(p[0].get("isRead").is_none());
}

#[tokio::test]
async fn answered_and_draft_are_never_written() {
    let mut s = two().await;
    let before = s.server.requests().len();
    for _ in 0..2 {
        s.gs.store_flags(&[HIGH], true, &[Flag::Answered, Flag::Draft])
            .await
            .unwrap();
    }
    assert_eq!(s.server.requests().len(), before, "no request at all");
}

#[tokio::test]
async fn move_to_removes_the_source_row_and_names_the_target_folder() {
    let mut s = two().await;
    s.server.push(Action::json(201, r#"{"id":"NEW"}"#));
    s.gs.move_to(&[HIGH], "Sent Items").await.unwrap();
    let mv = requests_with(&s.server, "/move");
    assert_eq!(mv.len(), 1);
    assert!(mv[0].path.contains("/messages/m1/move"));
    assert!(String::from_utf8_lossy(&mv[0].body).contains("F-SENT"));
    assert_eq!(s.map.uid_of(s.inbox, "m1").unwrap(), None);
    assert_eq!(s.map.count(s.inbox).unwrap(), 1);
}

#[tokio::test]
async fn deleting_a_gone_message_is_ok_and_drops_its_row() {
    let mut s = two().await;
    s.server.push(Action::json(
        404,
        r#"{"error":{"code":"ErrorItemNotFound"}}"#,
    ));
    s.gs.delete(&[HIGH]).await.unwrap();
    assert_eq!(requests_with(&s.server, "permanentDelete").len(), 1);
    assert_eq!(s.map.uid_of(s.inbox, "m1").unwrap(), None);
}

#[tokio::test]
async fn search_hits_get_low_uids_that_a_refresh_keeps() {
    let mut s = two().await;
    let next = s.gs.select("INBOX").await.unwrap().uid_next;
    s.server
        .push(list(&[msg("old1", "2026-01-01T00:00:00Z", true)]));
    let hits =
        s.gs.search_message_id("<fake-9@example.org>")
            .await
            .unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0] < HIGH && hits[0] > 0);
    let find = requests_with(&s.server, "internetMessageId");
    assert!(find[0].path.contains("mailFolders/F-INBOX/messages"));
    assert_eq!(s.gs.select("INBOX").await.unwrap().uid_next, next);
    // the hit's flags are in the map at once
    assert_eq!(s.map.row(s.inbox, hits[0]).unwrap().unwrap().flags, 1);
    s.server.push(page(
        &[msg("old1", "2026-01-01T00:00:00Z", true)],
        false,
        "d2",
    ));
    s.gs.refresh("INBOX").await.unwrap();
    assert_eq!(s.map.uid_of(s.inbox, "old1").unwrap(), Some(hits[0]));
    assert_eq!(s.gs.select("INBOX").await.unwrap().uid_next, next);
}

#[tokio::test]
async fn a_low_uid_row_is_not_new_mail() {
    let mut s = two().await;
    let prev = s.gs.select("INBOX").await.unwrap().uid_next;
    s.server
        .push(list(&[msg("old1", "2026-01-01T00:00:00Z", false)]));
    let low = s.gs.search_text("hello").await.unwrap()[0];
    s.server.push(page(
        &[msg("m3", "2026-11-21T12:00:00Z", false)],
        false,
        "d2",
    ));
    s.gs.refresh("INBOX").await.unwrap();
    let high = s.map.uid_of(s.inbox, "m3").unwrap().unwrap();
    let from = envio_mime::Address {
        name: None,
        email: "alice@example.org".into(),
    };
    let rows: Vec<NewRow> = [low, high]
        .iter()
        .map(|&uid| NewRow {
            uid,
            seen: false,
            from: from.clone(),
            subject: "s".into(),
        })
        .collect();
    let own = [envio_mime::Address {
        name: None,
        email: "me@example.org".into(),
    }];
    assert_eq!(count(Some(prev), false, &rows, &own).count, 1);
}

#[tokio::test]
async fn searching_without_a_selected_folder_is_refused() {
    let server = envio_net::graph::mock::Server::start(vec![]).await;
    let store = support::local::Store::on_file();
    let mut gs = support::graph::graph_server(&server, &store.db_path);
    let err = gs.search_text("x").await.unwrap_err();
    assert!(matches!(err, NetError::Refused(_)), "{err:?}");
}

#[tokio::test]
async fn drafts_search_stays_in_drafts_and_a_sent_copy_survives_the_delete() {
    let mut s = two().await;
    s.gs.select("Drafts").await.unwrap();
    s.server.push(list(&[msg("d-draft", T1, true)]));
    let uids =
        s.gs.search_message_id_deep("<fake-7@example.org>")
            .await
            .unwrap();
    s.server.push(Action::json(204, "{}"));
    s.gs.delete(&uids).await.unwrap();
    let find = requests_with(&s.server, "internetMessageId");
    assert_eq!(find.len(), 1);
    assert!(find[0].path.contains("mailFolders/F-DRAFTS/messages"));
    assert!(
        s.server
            .requests()
            .iter()
            .all(|r| !r.path.contains("F-SENT"))
    );
    let del = requests_with(&s.server, "permanentDelete");
    assert_eq!(del.len(), 1);
    assert!(del[0].path.contains("d-draft"));
}

#[tokio::test]
async fn append_only_files_into_drafts() {
    let mut s = two().await;
    let before = s.server.requests().len();
    let err =
        s.gs.append("INBOX", &[Flag::Seen], b"Subject: x\r\n\r\n")
            .await
            .unwrap_err();
    assert!(matches!(err, NetError::Refused(_)), "{err:?}");
    assert_eq!(s.server.requests().len(), before);
    s.server.push(Action::json(201, r#"{"id":"D1"}"#));
    s.gs.append("Drafts", &[Flag::Draft, Flag::Seen], b"Subject: x\r\n\r\n")
        .await
        .unwrap();
    let post = requests_with(&s.server, "mailFolders/F-DRAFTS/messages");
    assert_eq!(post.len(), 1);
    assert_eq!(post[0].method, "POST");
}
