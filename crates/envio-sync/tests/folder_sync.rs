//! Folder list and folder sync against the fake server.

mod support;

use envio_net::NetError;
use envio_net::imap::{Caps, Flag, MessageFlags, Role};
use envio_store::{BodyState, FolderRole};
use envio_sync::driver::{Synced, fetch_body, sync_folder, sync_folders};
use support::fake::{FakeServer, NOW_MS, NOW_SECS, eml};
use support::local::{ACCOUNT, Store};

async fn sync_inbox(
    server: &mut FakeServer,
    store: &mut Store,
) -> Result<Synced, envio_sync::SyncError> {
    sync_folders(server, &mut store.local(), ACCOUNT).await?;
    let inbox = store.folder("INBOX").id;
    sync_folder(server, &mut store.local(), inbox, NOW_MS, Some(30), None).await
}

#[tokio::test]
async fn the_folder_list_follows_the_server() {
    let mut server = FakeServer::new();
    server.add_folder("Archive", Some(Role::Archive));
    server.add_folder("Old", None);
    let mut store = Store::new();
    sync_folders(&mut server, &mut store.local(), ACCOUNT)
        .await
        .unwrap();
    assert_eq!(store.folder("Archive").role, FolderRole::Archive);
    server.folders.remove("Old");
    let folders = sync_folders(&mut server, &mut store.local(), ACCOUNT)
        .await
        .unwrap();
    let paths: Vec<&str> = folders.iter().map(|folder| folder.path.as_str()).collect();
    assert_eq!(paths, ["Archive", "INBOX"]);
}

#[tokio::test]
async fn a_first_sync_caches_the_last_thirty_days_only() {
    let mut server = FakeServer::new();
    server.deliver("INBOX", &eml("old"), NOW_SECS - 40 * 86_400);
    server.deliver_id("INBOX", "a");
    server.deliver_id("INBOX", "b");
    let mut store = Store::new();
    let synced = sync_inbox(&mut server, &mut store).await.unwrap();
    assert_eq!(synced.fetched, 2);
    let mut subjects = store.subjects("INBOX");
    subjects.sort();
    assert_eq!(subjects, ["About a", "About b"]);
    let state = store.folder("INBOX").sync.unwrap();
    assert_eq!((state.uidvalidity, state.uidnext), (7, 4));
}

#[tokio::test]
async fn a_cut_sync_resumes_without_fetching_anything_twice() {
    let mut server = FakeServer::new();
    for n in 0..60 {
        server.deliver_id("INBOX", &format!("m{n}"));
    }
    server.fail_after("fetch_summaries", 1, NetError::Timeout);
    let mut store = Store::new();
    assert!(sync_inbox(&mut server, &mut store).await.is_err());
    let first: Vec<u32> = store.uids("INBOX");
    assert_eq!(
        first,
        (11..=60).collect::<Vec<_>>(),
        "the newest batch is kept"
    );
    assert_eq!(
        store.folder("INBOX").sync.unwrap().uidnext,
        0,
        "not finished"
    );
    server.calls.clear();
    let synced = sync_inbox(&mut server, &mut store).await.unwrap();
    assert_eq!(synced.fetched, 10);
    assert_eq!(store.uids("INBOX"), (1..=60).collect::<Vec<_>>());
    let fetches: Vec<&String> = server
        .calls
        .iter()
        .filter(|call| call.starts_with("fetch_summaries"))
        .collect();
    assert_eq!(fetches.len(), 1);
    assert_eq!(store.folder("INBOX").sync.unwrap().uidnext, 61);
}

#[tokio::test]
async fn a_new_uidvalidity_drops_the_cache_and_its_bodies() {
    let mut server = FakeServer::new();
    server.deliver_id("INBOX", "a");
    let mut store = Store::new();
    sync_inbox(&mut server, &mut store).await.unwrap();
    let inbox = store.folder("INBOX").id;
    let message = store.db.message_at(inbox, 1).unwrap().unwrap();
    fetch_body(&mut server, &mut store.local(), message.id)
        .await
        .unwrap();
    let body_dir = store.dir.path().join("bodies").join(inbox.0.to_string());
    assert!(body_dir.exists());
    server.deliver_id("INBOX", "b");
    server.renumber("INBOX");
    server.folders.get_mut("INBOX").unwrap().messages.remove(&1);
    sync_inbox(&mut server, &mut store).await.unwrap();
    assert_eq!(store.subjects("INBOX"), ["About b"]);
    assert_eq!(store.folder("INBOX").sync.unwrap().uidvalidity, 8);
    assert!(!body_dir.exists());
}

#[tokio::test]
async fn flag_changes_from_another_device_arrive_with_and_without_condstore() {
    for caps in [Caps::from_names(["CONDSTORE"]), Caps::default()] {
        let mut server = FakeServer::new();
        server.caps = caps;
        let a = server.deliver_id("INBOX", "a");
        let b = server.deliver_id("INBOX", "b");
        let c = server.deliver_id("INBOX", "c");
        let mut store = Store::new();
        sync_inbox(&mut server, &mut store).await.unwrap();
        server.set_flags("INBOX", a, MessageFlags::from_flags([Flag::Seen]));
        server.set_flags("INBOX", c, MessageFlags::from_flags([Flag::Deleted]));
        let synced = sync_inbox(&mut server, &mut store).await.unwrap();
        assert_eq!(synced.flags_changed, 2, "{caps:?}");
        let inbox = store.folder("INBOX").id;
        let flags = store.db.flags_by_uid(inbox).unwrap();
        assert_eq!(flags.len(), 2, "a \\Deleted message isn't shown");
        assert!(flags[0].1.contains(envio_store::Flags::SEEN));
        assert_eq!(flags[1].0, b);
    }
}

#[tokio::test]
async fn an_unchanged_folder_costs_no_fetches_with_condstore() {
    let mut server = FakeServer::new();
    server.deliver_id("INBOX", "a");
    let mut store = Store::new();
    sync_inbox(&mut server, &mut store).await.unwrap();
    server.calls.clear();
    let synced = sync_inbox(&mut server, &mut store).await.unwrap();
    assert_eq!(synced, Synced::default());
    assert!(server.calls.iter().all(|call| !call.starts_with("fetch")));
}

#[tokio::test]
async fn messages_gone_from_the_server_leave_the_cache() {
    let mut server = FakeServer::new();
    let a = server.deliver_id("INBOX", "a");
    server.deliver_id("INBOX", "b");
    let mut store = Store::new();
    sync_inbox(&mut server, &mut store).await.unwrap();
    server.folders.get_mut("INBOX").unwrap().messages.remove(&a);
    let synced = sync_inbox(&mut server, &mut store).await.unwrap();
    assert_eq!(synced.removed, 1);
    assert_eq!(store.subjects("INBOX"), ["About b"]);
}

#[tokio::test]
async fn opening_a_message_downloads_its_body_once() {
    let mut server = FakeServer::new();
    let uid = server.deliver_id("INBOX", "a");
    let mut store = Store::new();
    sync_inbox(&mut server, &mut store).await.unwrap();
    let inbox = store.folder("INBOX").id;
    let id = store.db.message_at(inbox, uid).unwrap().unwrap().id;
    let eml = fetch_body(&mut server, &mut store.local(), id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(eml, support::fake::eml("a"));
    assert_eq!(store.bodies.read(inbox, uid).unwrap(), eml);
    let message = store.db.message(id).unwrap();
    assert_eq!(message.body, BodyState::Downloaded);
    assert_eq!(message.preview, "Hello about a.");
}

#[tokio::test]
async fn opening_a_message_that_is_gone_drops_its_row() {
    let mut server = FakeServer::new();
    let uid = server.deliver_id("INBOX", "a");
    let mut store = Store::new();
    sync_inbox(&mut server, &mut store).await.unwrap();
    let inbox = store.folder("INBOX").id;
    let id = store.db.message_at(inbox, uid).unwrap().unwrap().id;
    server
        .folders
        .get_mut("INBOX")
        .unwrap()
        .messages
        .remove(&uid);
    let body = fetch_body(&mut server, &mut store.local(), id)
        .await
        .unwrap();
    assert_eq!(body, None);
    assert!(store.subjects("INBOX").is_empty());
}

#[tokio::test]
async fn a_failed_window_search_keeps_the_cache() {
    let mut server = FakeServer::new();
    server.deliver_id("INBOX", "a");
    server.deliver_id("INBOX", "b");
    let mut store = Store::new();
    sync_inbox(&mut server, &mut store).await.unwrap();
    assert_eq!(store.uids("INBOX"), [1, 2]);
    server.fail_on("search_since", NetError::Timeout);
    assert!(sync_inbox(&mut server, &mut store).await.is_err());
    assert_eq!(store.uids("INBOX"), [1, 2], "nothing was dropped");
    assert_eq!(store.subjects("INBOX").len(), 2);
}

#[tokio::test]
async fn a_sync_remembers_who_wrote_and_who_was_written_to() {
    let mut server = FakeServer::new();
    server.deliver_id("INBOX", "a");
    let mut store = Store::new();
    sync_inbox(&mut server, &mut store).await.unwrap();
    let found = store.db.suggest("ad", 8).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].address, "ada@example.org");
    assert_eq!(found[0].name, "Ada");
    assert_eq!(store.db.suggest("bob", 8).unwrap().len(), 1);
}

fn with_keywords(flags: &[Flag], words: &[&str]) -> MessageFlags {
    MessageFlags::from_flags(
        flags
            .iter()
            .cloned()
            .chain(words.iter().map(|word| Flag::Keyword((*word).into()))),
    )
}

#[tokio::test]
async fn a_first_sync_stores_keywords_for_every_row() {
    let mut server = FakeServer::new();
    let a = server.deliver_id("INBOX", "a");
    let b = server.deliver_id("INBOX", "b");
    server.set_flags("INBOX", a, with_keywords(&[], &["$Forwarded", "$Label1"]));
    let mut store = Store::new();
    sync_inbox(&mut server, &mut store).await.unwrap();
    let inbox = store.folder("INBOX").id;
    assert_eq!(
        store.db.keywords_by_uid(inbox).unwrap(),
        [
            (a, Some(vec!["$Forwarded".to_owned(), "$Label1".to_owned()])),
            (b, Some(vec![])),
        ]
    );
}

#[tokio::test]
async fn a_server_side_keyword_change_arrives_without_counting_as_a_flag_change() {
    for caps in [Caps::from_names(["CONDSTORE"]), Caps::default()] {
        let mut server = FakeServer::new();
        server.caps = caps;
        let a = server.deliver_id("INBOX", "a");
        let b = server.deliver_id("INBOX", "b");
        let mut store = Store::new();
        sync_inbox(&mut server, &mut store).await.unwrap();
        server.set_flags("INBOX", a, with_keywords(&[], &["$Forwarded"]));
        let synced = sync_inbox(&mut server, &mut store).await.unwrap();
        assert_eq!(synced.flags_changed, 0, "{caps:?}");
        let inbox = store.folder("INBOX").id;
        assert_eq!(
            store.db.keywords_of(inbox, a).unwrap(),
            Some(vec!["$Forwarded".to_owned()])
        );
        assert_eq!(store.db.keywords_of(inbox, b).unwrap(), Some(vec![]));
        // A bit change still counts.
        server.set_flags("INBOX", b, with_keywords(&[Flag::Seen], &[]));
        let synced = sync_inbox(&mut server, &mut store).await.unwrap();
        assert_eq!(synced.flags_changed, 1, "{caps:?}");
    }
}

#[tokio::test]
async fn a_deleted_message_is_removed_and_a_pending_flag_does_not_hide_keywords() {
    let mut server = FakeServer::new();
    let a = server.deliver_id("INBOX", "a");
    let b = server.deliver_id("INBOX", "b");
    let mut store = Store::new();
    sync_inbox(&mut server, &mut store).await.unwrap();
    let inbox = store.folder("INBOX").id;
    // A pending local "read" on `a`; the server gains a keyword meanwhile.
    let row = store.db.message_at(inbox, a).unwrap().unwrap().id;
    envio_sync::driver::record(
        &mut store.local(),
        row,
        envio_store::ActionKind::SetFlags {
            add: envio_store::Flags::SEEN,
            remove: envio_store::Flags::empty(),
        },
        NOW_MS,
    )
    .unwrap();
    server.set_flags("INBOX", a, with_keywords(&[], &["$Forwarded"]));
    server.set_flags("INBOX", b, with_keywords(&[Flag::Deleted], &["$x"]));
    sync_inbox(&mut server, &mut store).await.unwrap();
    assert_eq!(
        store.db.keywords_by_uid(inbox).unwrap(),
        [(a, Some(vec!["$Forwarded".to_owned()]))]
    );
}
