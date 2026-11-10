//! The fake server keeps the IMAP rules the driver tests rely on.

mod support;

use envio_net::NetError;
use envio_net::imap::{Caps, Flag, MessageFlags, Role};
use envio_sync::server::MailServer;
use support::fake::{FakeServer, NOW_SECS};

#[tokio::test]
async fn uids_only_grow_and_a_renumber_changes_uidvalidity() {
    let mut server = FakeServer::new();
    let first = server.deliver_id("INBOX", "a");
    let second = server.deliver_id("INBOX", "b");
    assert!(second > first);
    let before = server.select("INBOX").await.unwrap();
    server.renumber("INBOX");
    let after = server.select("INBOX").await.unwrap();
    assert_eq!(after.uid_validity, before.uid_validity + 1);
    assert_eq!(server.ids("INBOX"), ["a@example.org", "b@example.org"]);
}

#[tokio::test]
async fn search_since_counts_whole_days() {
    let mut server = FakeServer::new();
    let old = server.deliver("INBOX", &support::fake::eml("old"), NOW_SECS - 40 * 86_400);
    let new = server.deliver_id("INBOX", "new");
    server.select("INBOX").await.unwrap();
    let found = server.search_since(NOW_SECS - 30 * 86_400).await.unwrap();
    assert_eq!(found, [new]);
    assert!(!found.contains(&old));
}

#[tokio::test]
async fn without_move_a_failed_delete_leaves_the_copy_and_the_original() {
    let mut server = FakeServer::new();
    server.caps = Caps::from_names(["UIDPLUS"]);
    server.add_folder("Archive", Some(Role::Archive));
    let uid = server.deliver_id("INBOX", "a");
    server.select("INBOX").await.unwrap();
    server.fail_on("delete", NetError::Timeout);
    assert!(server.move_to(&[uid], "Archive").await.is_err());
    assert_eq!(server.uids("INBOX"), [uid]);
    assert_eq!(server.ids("Archive"), ["a@example.org"]);
}

#[tokio::test]
async fn changed_since_returns_only_newer_flag_changes() {
    let mut server = FakeServer::new();
    let a = server.deliver_id("INBOX", "a");
    let b = server.deliver_id("INBOX", "b");
    let state = server.select("INBOX").await.unwrap();
    server.set_flags("INBOX", b, MessageFlags::from_flags([Flag::Seen]));
    let changed = server
        .fetch_flags(&[a, b], state.highest_modseq)
        .await
        .unwrap();
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].uid, b);
    assert!(changed[0].flags.seen);
}

#[tokio::test]
async fn scripted_search_and_fetch_failures_come_back_as_errors() {
    let mut server = FakeServer::new();
    server.deliver_id("INBOX", "a");
    server.select("INBOX").await.unwrap();
    server.fail_on("search_since", NetError::Timeout);
    assert!(server.search_since(NOW_SECS).await.is_err());
    server.fail_on("search_message_id", NetError::Timeout);
    assert!(server.search_message_id("a@example.org").await.is_err());
    server.fail_on("fetch_summaries", NetError::Timeout);
    assert!(server.fetch_summaries(&[1]).await.is_err());
    server.fail_on("fetch_body", NetError::Timeout);
    assert!(server.fetch_body(1, 10).await.is_err());
}

#[tokio::test]
async fn without_uidplus_a_move_leaves_the_original_flagged_deleted() {
    let mut server = FakeServer::new();
    server.caps = Caps::from_names(["CONDSTORE"]);
    server.add_folder("Archive", Some(Role::Archive));
    let uid = server.deliver_id("INBOX", "a");
    server.select("INBOX").await.unwrap();
    server.move_to(&[uid], "Archive").await.unwrap();
    assert_eq!(server.ids("Archive"), ["a@example.org"]);
    assert_eq!(server.uids("INBOX"), [uid]);
    assert!(server.message("INBOX", uid).unwrap().flags.deleted);
}

#[tokio::test]
async fn without_uidplus_a_plain_delete_only_flags_deleted() {
    let mut server = FakeServer::new();
    server.caps = Caps::from_names(["CONDSTORE"]);
    let uid = server.deliver_id("INBOX", "a");
    let state = server.select("INBOX").await.unwrap();
    server.delete(&[uid]).await.unwrap();
    let message = server.message("INBOX", uid).unwrap();
    assert!(message.flags.deleted);
    assert!(message.modseq > state.highest_modseq.unwrap());
}

#[tokio::test]
async fn a_cut_between_copy_and_delete_keeps_the_copy_and_the_original() {
    let mut server = FakeServer::new();
    server.caps = Caps::from_names(["UIDPLUS"]);
    server.add_folder("Archive", Some(Role::Archive));
    let uid = server.deliver_id("INBOX", "a");
    server.select("INBOX").await.unwrap();
    server.fail_on("delete", NetError::Timeout);
    assert!(server.move_to(&[uid], "Archive").await.is_err());
    assert_eq!(server.ids("Archive"), ["a@example.org"]);
    assert_eq!(server.uids("INBOX"), [uid]);
    assert!(!server.message("INBOX", uid).unwrap().flags.deleted);
}
