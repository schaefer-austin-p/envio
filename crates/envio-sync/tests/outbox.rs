//! The Outbox sends once, files in Sent once, and never resends on its own.

mod support;

use envio_net::NetError;
use envio_net::imap::Role;
use envio_net::smtp::SendError;
use envio_store::{NewOutboxItem, SendState};
use envio_sync::ErrorKind;
use envio_sync::driver::{file_sent, send_outbox, sync_folders};
use support::fake::{FakeSender, FakeServer, NOW_MS, eml};
use support::local::{ACCOUNT, Store};

async fn store_with_sent(server: &mut FakeServer) -> Store {
    server.add_folder("Sent", Some(Role::Sent));
    let mut store = Store::new();
    sync_folders(server, &mut store.local(), ACCOUNT)
        .await
        .unwrap();
    store
}

fn queue(store: &Store, id: &str) -> String {
    let item = NewOutboxItem {
        account: ACCOUNT.into(),
        message_id: format!("<{id}@example.org>"),
        queued_at_ms: NOW_MS,
        from: "carol@example.org".into(),
        recipients: vec!["dave@example.org".into(), "erin@example.org".into()],
    };
    store.outbox.enqueue(item, &eml(id)).unwrap().id
}

fn state(store: &Store, id: &str) -> Option<SendState> {
    let items = store.outbox.items().unwrap();
    let item = items.into_iter().find(|item| item.id == id)?;
    Some(item.meta.state)
}

fn failing(error: NetError, unconfirmed: bool) -> FakeSender {
    FakeSender {
        results: vec![Err(SendError { error, unconfirmed })],
        ..FakeSender::default()
    }
}

#[tokio::test]
async fn a_queued_message_is_sent_then_filed_in_sent_once() {
    let mut server = FakeServer::new();
    let mut store = store_with_sent(&mut server).await;
    let id = queue(&store, "out");
    let mut sender = FakeSender::default();
    let sent = send_outbox(&mut sender, &mut store.local(), ACCOUNT, NOW_MS)
        .await
        .unwrap();
    assert_eq!(sent.accepted, [id.as_str()]);
    let (from, to, body) = &sender.attempts[0];
    assert_eq!(from, "carol@example.org");
    assert_eq!(to, &["dave@example.org", "erin@example.org"]);
    assert_eq!(body, &eml("out"));
    assert_eq!(state(&store, &id), Some(SendState::Accepted));
    let filed = file_sent(&mut server, &mut store.local(), ACCOUNT, false)
        .await
        .unwrap();
    assert_eq!(filed.removed, [id.as_str()]);
    assert_eq!(server.ids("Sent"), ["out@example.org"]);
    assert!(server.message("Sent", 1).unwrap().flags.seen);
    assert_eq!(state(&store, &id), None);
}

#[tokio::test]
async fn an_accepted_message_already_in_sent_is_not_filed_twice() {
    let mut server = FakeServer::new();
    let mut store = store_with_sent(&mut server).await;
    let id = queue(&store, "out");
    store.outbox.set_state(&id, SendState::Accepted).unwrap();
    server.deliver_id("Sent", "out");
    file_sent(&mut server, &mut store.local(), ACCOUNT, false)
        .await
        .unwrap();
    assert_eq!(server.ids("Sent").len(), 1);
    assert_eq!(state(&store, &id), None);
}

#[tokio::test]
async fn gmail_files_sent_mail_itself() {
    let mut server = FakeServer::new();
    let mut store = store_with_sent(&mut server).await;
    let id = queue(&store, "out");
    store.outbox.set_state(&id, SendState::Accepted).unwrap();
    file_sent(&mut server, &mut store.local(), ACCOUNT, true)
        .await
        .unwrap();
    assert!(server.ids("Sent").is_empty());
    assert_eq!(state(&store, &id), None);
}

#[tokio::test]
async fn a_message_that_may_have_been_sent_is_never_resent() {
    let mut server = FakeServer::new();
    let mut store = store_with_sent(&mut server).await;
    let id = queue(&store, "out");
    let mut sender = failing(NetError::Timeout, true);
    let sent = send_outbox(&mut sender, &mut store.local(), ACCOUNT, NOW_MS)
        .await
        .unwrap();
    assert_eq!(sent.unconfirmed, [id.as_str()]);
    assert_eq!(sent.stopped, None);
    send_outbox(&mut sender, &mut store.local(), ACCOUNT, NOW_MS + 3_600_000)
        .await
        .unwrap();
    assert_eq!(sender.attempts.len(), 1);
    let filed = file_sent(&mut server, &mut store.local(), ACCOUNT, false)
        .await
        .unwrap();
    assert_eq!(filed.ask, [id.as_str()], "not in Sent: the user decides");
    assert_eq!(state(&store, &id), Some(SendState::MayHaveBeenSent));
    server.deliver_id("Sent", "out");
    let filed = file_sent(&mut server, &mut store.local(), ACCOUNT, false)
        .await
        .unwrap();
    assert_eq!(filed.removed, [id]);
    assert_eq!(server.ids("Sent").len(), 1, "found, not appended");
}

#[tokio::test]
async fn a_refused_message_fails_with_the_servers_reason() {
    let mut server = FakeServer::new();
    let mut store = store_with_sent(&mut server).await;
    let id = queue(&store, "out");
    let mut sender = failing(NetError::Refused("550 no such user".into()), false);
    let sent = send_outbox(&mut sender, &mut store.local(), ACCOUNT, NOW_MS)
        .await
        .unwrap();
    assert_eq!(sent.failed, [id.as_str()]);
    let reason = "550 no such user".to_owned();
    assert_eq!(state(&store, &id), Some(SendState::Failed { reason }));
    send_outbox(&mut sender, &mut store.local(), ACCOUNT, NOW_MS + 3_600_000)
        .await
        .unwrap();
    assert_eq!(sender.attempts.len(), 1);
}

#[tokio::test]
async fn a_busy_server_means_retry_after_a_backoff() {
    let mut server = FakeServer::new();
    let mut store = store_with_sent(&mut server).await;
    let id = queue(&store, "out");
    let mut sender = failing(NetError::Busy("421 try later".into()), false);
    let sent = send_outbox(&mut sender, &mut store.local(), ACCOUNT, NOW_MS)
        .await
        .unwrap();
    assert_eq!(sent.stopped, Some(ErrorKind::Temporary));
    assert_eq!(state(&store, &id), Some(SendState::Queued));
    let retry = store.schedule.next_due().unwrap();
    send_outbox(&mut sender, &mut store.local(), ACCOUNT, retry - 1)
        .await
        .unwrap();
    assert_eq!(sender.attempts.len(), 1);
    let sent = send_outbox(&mut sender, &mut store.local(), ACCOUNT, retry)
        .await
        .unwrap();
    assert_eq!(sent.accepted, [id]);
}

#[tokio::test]
async fn a_rejected_sign_in_pauses_and_keeps_the_message_queued() {
    let mut server = FakeServer::new();
    let mut store = store_with_sent(&mut server).await;
    let first = queue(&store, "one");
    queue(&store, "two");
    let mut sender = failing(NetError::SignIn("535 bad password".into()), false);
    let sent = send_outbox(&mut sender, &mut store.local(), ACCOUNT, NOW_MS)
        .await
        .unwrap();
    assert_eq!(sent.stopped, Some(ErrorKind::SignIn));
    assert_eq!(sender.attempts.len(), 1, "the second waits too");
    assert_eq!(state(&store, &first), Some(SendState::Queued));
}

#[tokio::test]
async fn an_unusable_message_id_never_matches_anything_in_sent() {
    let mut server = FakeServer::new();
    let mut store = store_with_sent(&mut server).await;
    let item = NewOutboxItem {
        account: ACCOUNT.into(),
        message_id: String::new(),
        queued_at_ms: NOW_MS,
        from: "carol@example.org".into(),
        recipients: vec!["dave@example.org".into()],
    };
    let id = store.outbox.enqueue(item, &eml("out")).unwrap().id;
    store
        .outbox
        .set_state(&id, SendState::MayHaveBeenSent)
        .unwrap();
    server.deliver_id("Sent", "unrelated");
    let filed = file_sent(&mut server, &mut store.local(), ACCOUNT, false)
        .await
        .unwrap();
    assert_eq!(filed.ask, [id.as_str()]);
    assert!(filed.removed.is_empty());
    let searched = server
        .calls
        .iter()
        .any(|c| c.starts_with("search_message_id"));
    assert!(!searched, "a substring search would match anything");
    assert_eq!(state(&store, &id), Some(SendState::MayHaveBeenSent));
}

#[tokio::test]
async fn a_refused_item_does_not_block_filing_the_others() {
    let mut server = FakeServer::new();
    let mut store = store_with_sent(&mut server).await;
    let first = queue(&store, "one");
    let second = queue(&store, "two");
    for id in [&first, &second] {
        store.outbox.set_state(id, SendState::Accepted).unwrap();
    }
    server.fail_on("append", NetError::Refused("over quota".into()));
    let filed = file_sent(&mut server, &mut store.local(), ACCOUNT, false)
        .await
        .unwrap();
    assert_eq!(server.ids("Sent").len(), 1);
    assert_eq!(filed.removed.len(), 1);
    assert!(filed.ask.is_empty());
    let left = store.outbox.items().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].meta.state, SendState::Accepted);
    assert_ne!(left[0].id, filed.removed[0]);
    // The refused one is reported, not skipped silently.
    assert_eq!(filed.unfiled, [left[0].id.clone()]);
}

#[tokio::test]
async fn a_refused_sent_select_leaves_items_in_place_and_reports_them() {
    let mut server = FakeServer::new();
    let mut store = store_with_sent(&mut server).await;
    let accepted = queue(&store, "one");
    store
        .outbox
        .set_state(&accepted, SendState::Accepted)
        .unwrap();
    let unsure = queue(&store, "two");
    store
        .outbox
        .set_state(&unsure, SendState::MayHaveBeenSent)
        .unwrap();
    server.fail_on_detail("select", "Sent", NetError::Refused("no such folder".into()));
    let filed = file_sent(&mut server, &mut store.local(), ACCOUNT, false)
        .await
        .unwrap();
    assert!(filed.removed.is_empty());
    assert!(filed.ask.is_empty(), "Sent was not searched: not asked");
    assert_eq!(filed.unfiled, std::slice::from_ref(&accepted));
    assert_eq!(state(&store, &accepted), Some(SendState::Accepted));
    assert_eq!(state(&store, &unsure), Some(SendState::MayHaveBeenSent));
}

#[tokio::test]
async fn a_refused_sent_select_keeps_what_was_already_filed() {
    // The server files sent mail itself, so the accepted item needs no
    // Sent folder; only the May have been sent one is looked for there.
    let mut server = FakeServer::new();
    let mut store = store_with_sent(&mut server).await;
    let accepted = queue(&store, "one");
    store
        .outbox
        .set_state(&accepted, SendState::Accepted)
        .unwrap();
    let unsure = queue(&store, "two");
    store
        .outbox
        .set_state(&unsure, SendState::MayHaveBeenSent)
        .unwrap();
    server.fail_on_detail("select", "Sent", NetError::Refused("no such folder".into()));
    let filed = file_sent(&mut server, &mut store.local(), ACCOUNT, true)
        .await
        .unwrap();
    assert_eq!(filed.removed, std::slice::from_ref(&accepted));
    assert!(filed.unfiled.is_empty(), "it needed no Sent folder");
    assert!(filed.ask.is_empty(), "Sent was not searched: not asked");
    assert_eq!(state(&store, &accepted), None);
    assert_eq!(state(&store, &unsure), Some(SendState::MayHaveBeenSent));
}

#[tokio::test]
async fn with_no_folders_cached_an_accepted_item_is_left_in_the_outbox() {
    let mut server = FakeServer::new();
    let mut store = Store::new();
    let id = queue(&store, "one");
    store.outbox.set_state(&id, SendState::Accepted).unwrap();
    let filed = file_sent(&mut server, &mut store.local(), ACCOUNT, false)
        .await
        .unwrap();
    assert!(filed.removed.is_empty());
    assert!(filed.ask.is_empty());
    assert_eq!(state(&store, &id), Some(SendState::Accepted));
}

#[tokio::test]
async fn a_maybe_sent_item_whose_sent_search_is_refused_is_left_for_the_next_try() {
    let mut server = FakeServer::new();
    let mut store = store_with_sent(&mut server).await;
    let unsure = queue(&store, "two");
    store
        .outbox
        .set_state(&unsure, SendState::MayHaveBeenSent)
        .unwrap();
    server.fail_on("search_message_id", NetError::Refused("no search".into()));
    let filed = file_sent(&mut server, &mut store.local(), ACCOUNT, false)
        .await
        .unwrap();
    assert!(filed.ask.is_empty(), "the search failed: not asked");
    assert!(filed.removed.is_empty());
    assert!(filed.unfiled.is_empty());
    assert_eq!(state(&store, &unsure), Some(SendState::MayHaveBeenSent));
}

#[tokio::test]
async fn a_refused_sent_select_leaves_maybe_sent_items_unasked() {
    let mut server = FakeServer::new();
    let mut store = store_with_sent(&mut server).await;
    let unsure = queue(&store, "two");
    store
        .outbox
        .set_state(&unsure, SendState::MayHaveBeenSent)
        .unwrap();
    server.fail_on_detail("select", "Sent", NetError::Refused("no such folder".into()));
    let filed = file_sent(&mut server, &mut store.local(), ACCOUNT, false)
        .await
        .unwrap();
    assert!(filed.ask.is_empty(), "the select failed: not asked");
    assert!(filed.unfiled.is_empty());
    assert_eq!(state(&store, &unsure), Some(SendState::MayHaveBeenSent));
}

#[tokio::test]
async fn a_maybe_sent_item_is_asked_only_when_sent_was_searched_and_lacks_it() {
    let mut server = FakeServer::new();
    let mut store = store_with_sent(&mut server).await;
    let unsure = queue(&store, "two");
    store
        .outbox
        .set_state(&unsure, SendState::MayHaveBeenSent)
        .unwrap();
    let filed = file_sent(&mut server, &mut store.local(), ACCOUNT, false)
        .await
        .unwrap();
    assert_eq!(filed.ask, std::slice::from_ref(&unsure));
}

#[tokio::test]
async fn a_padded_message_id_is_searched_trimmed() {
    let mut server = FakeServer::new();
    let mut store = store_with_sent(&mut server).await;
    let item = NewOutboxItem {
        account: ACCOUNT.into(),
        message_id: " <out@example.org> ".into(),
        queued_at_ms: NOW_MS,
        from: "carol@example.org".into(),
        recipients: vec!["dave@example.org".into()],
    };
    let id = store.outbox.enqueue(item, &eml("out")).unwrap().id;
    store
        .outbox
        .set_state(&id, SendState::MayHaveBeenSent)
        .unwrap();
    server.deliver_id("Sent", "out");
    let filed = file_sent(&mut server, &mut store.local(), ACCOUNT, false)
        .await
        .unwrap();
    assert_eq!(filed.removed, [id.as_str()]);
    let trimmed = "search_message_id <out@example.org>".to_owned();
    assert!(server.calls.contains(&trimmed));
}
