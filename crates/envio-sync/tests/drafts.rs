//! Drafts reach the server's Drafts folder once, a later save replaces the
//! copy, and a sent or discarded draft's copy goes. A failure never costs
//! the local draft.

mod support;

use std::collections::BTreeSet;

use envio_mime::Address;
use envio_net::NetError;
use envio_net::imap::{Caps, Role};
use envio_store::Draft;
use envio_sync::driver::{DraftRun, DraftsSynced, sync_drafts, sync_folders};
use support::fake::{FakeServer, NOW_MS};
use support::local::{ACCOUNT, Store};

async fn setup(with_drafts_folder: bool) -> (FakeServer, Store) {
    let mut server = FakeServer::new();
    if with_drafts_folder {
        server.add_folder("Drafts", Some(Role::Drafts));
    }
    let mut store = Store::new();
    sync_folders(&mut server, &mut store.local(), ACCOUNT)
        .await
        .unwrap();
    (server, store)
}

fn draft(body: &str, updated_at_ms: i64) -> Draft {
    Draft {
        account: ACCOUNT.into(),
        to: vec!["bob@example.org".into()],
        subject: "Plans".into(),
        body: body.into(),
        updated_at_ms,
        ..Draft::default()
    }
}

async fn run_with(
    server: &mut FakeServer,
    store: &Store,
    open: &BTreeSet<String>,
) -> envio_sync::Result<DraftsSynced> {
    let from = Address::new(Some("Carol"), "carol@example.org");
    let run = DraftRun {
        account: ACCOUNT,
        from: &from,
        open,
        now_ms: NOW_MS,
    };
    sync_drafts(server, &store.db, &store.drafts, &run).await
}

async fn run(server: &mut FakeServer, store: &Store) -> DraftsSynced {
    run_with(server, store, &BTreeSet::new()).await.unwrap()
}

/// The live messages in Drafts: body text, \Draft and \Seen set.
fn copies(server: &FakeServer) -> Vec<(String, bool, bool)> {
    server.folders["Drafts"]
        .messages
        .values()
        .filter(|m| !m.flags.deleted)
        .map(|m| {
            (
                String::from_utf8_lossy(&m.eml).into_owned(),
                m.flags.draft,
                m.flags.seen,
            )
        })
        .collect()
}

#[tokio::test]
async fn a_saved_draft_is_appended_as_a_seen_draft_and_named_in_its_file() {
    let (mut server, store) = setup(true).await;
    let id = store.drafts.create(&draft("first text", 1)).unwrap();
    let done = run(&mut server, &store).await;
    assert_eq!(done.uploaded, [id.clone()]);
    let found = copies(&server);
    assert_eq!(found.len(), 1);
    assert!(found[0].0.contains("first text"));
    assert!(found[0].1 && found[0].2, "flags \\Draft and \\Seen");
    let server_copy = store.drafts.load(&id).unwrap().server;
    let message_id = server_copy.message_id.expect("the copy is named");
    assert!(found[0].0.contains(&format!("Message-ID: {message_id}")));
    assert_eq!((server_copy.pending_id, server_copy.uploaded_ms), (None, 1));
}

#[tokio::test]
async fn an_unchanged_draft_is_not_uploaded_again() {
    let (mut server, store) = setup(true).await;
    store.drafts.create(&draft("text", 1)).unwrap();
    run(&mut server, &store).await;
    let again = run(&mut server, &store).await;
    assert_eq!(again, DraftsSynced::default());
    assert_eq!(copies(&server).len(), 1);
}

#[tokio::test]
async fn a_later_save_replaces_the_copy() {
    let (mut server, store) = setup(true).await;
    let id = store.drafts.create(&draft("first text", 1)).unwrap();
    run(&mut server, &store).await;
    store.drafts.save(&id, &draft("second text", 2)).unwrap();
    run(&mut server, &store).await;
    let found = copies(&server);
    assert_eq!(found.len(), 1, "one copy, not two");
    assert!(found[0].0.contains("second text"));
    assert_eq!(
        server.folders["Drafts"].messages.len(),
        1,
        "the old one is expunged"
    );
}

#[tokio::test]
async fn without_uid_expunge_the_old_copy_is_only_marked_deleted() {
    let (mut server, store) = setup(true).await;
    server.caps = Caps::from_names(["MOVE", "CONDSTORE", "IDLE"]);
    let id = store.drafts.create(&draft("first text", 1)).unwrap();
    run(&mut server, &store).await;
    store.drafts.save(&id, &draft("second text", 2)).unwrap();
    run(&mut server, &store).await;
    assert_eq!(server.folders["Drafts"].messages.len(), 2);
    assert_eq!(copies(&server).len(), 1);
}

#[tokio::test]
async fn a_draft_in_an_open_window_waits_for_the_close() {
    let (mut server, store) = setup(true).await;
    let id = store.drafts.create(&draft("text", 1)).unwrap();
    let open = BTreeSet::from([id]);
    let done = run_with(&mut server, &store, &open).await.unwrap();
    assert_eq!(done, DraftsSynced::default());
    assert!(copies(&server).is_empty());
}

#[tokio::test]
async fn an_account_with_no_drafts_folder_skips_silently_and_keeps_the_draft() {
    let (mut server, store) = setup(false).await;
    let id = store.drafts.create(&draft("text", 1)).unwrap();
    let done = run(&mut server, &store).await;
    assert!(done.no_folder && done.uploaded.is_empty());
    assert_eq!(store.drafts.load(&id).unwrap().server.message_id, None);
}

#[tokio::test]
async fn a_failed_upload_keeps_the_local_draft_and_the_next_run_uploads_once() {
    let (mut server, store) = setup(true).await;
    let id = store.drafts.create(&draft("text", 1)).unwrap();
    server.fail_on("append", NetError::Timeout);
    assert!(
        run_with(&mut server, &store, &BTreeSet::new())
            .await
            .is_err()
    );
    assert_eq!(store.drafts.load(&id).unwrap().body, "text");
    assert!(copies(&server).is_empty());
    let done = run(&mut server, &store).await;
    assert_eq!(done.uploaded, [id]);
    assert_eq!(copies(&server).len(), 1);
}

#[tokio::test]
async fn a_cut_after_the_append_never_leaves_two_copies() {
    let (mut server, store) = setup(true).await;
    let id = store.drafts.create(&draft("first text", 1)).unwrap();
    run(&mut server, &store).await;
    store.drafts.save(&id, &draft("second text", 2)).unwrap();
    // The new copy is appended, then deleting the old one is cut.
    server.fail_on("delete", NetError::Timeout);
    assert!(
        run_with(&mut server, &store, &BTreeSet::new())
            .await
            .is_err()
    );
    assert_eq!(copies(&server).len(), 2);
    run(&mut server, &store).await;
    let found = copies(&server);
    assert_eq!(found.len(), 1);
    assert!(found[0].0.contains("second text"));
    assert_eq!(store.drafts.load(&id).unwrap().server.pending_id, None);
}

#[tokio::test]
async fn a_deleted_draft_takes_its_server_copy_with_it_and_a_cut_waits() {
    let (mut server, store) = setup(true).await;
    let id = store.drafts.create(&draft("text", 1)).unwrap();
    run(&mut server, &store).await;
    store.drafts.delete(&id).unwrap();
    server.fail_on("delete", NetError::Timeout);
    assert!(
        run_with(&mut server, &store, &BTreeSet::new())
            .await
            .is_err()
    );
    assert_eq!(copies(&server).len(), 1, "still there, note kept");
    assert_eq!(store.drafts.deletes().unwrap().len(), 1);
    let done = run(&mut server, &store).await;
    assert_eq!(done.deleted, [id]);
    assert!(copies(&server).is_empty());
    assert!(store.drafts.deletes().unwrap().is_empty());
}

#[tokio::test]
async fn a_draft_deleted_during_its_upload_leaves_no_copy_behind() {
    let (mut server, store) = setup(true).await;
    let id = store.drafts.create(&draft("text", 1)).unwrap();
    // `mark_uploading` runs before the append: the user deletes the draft
    // between that and the end of the upload.
    store
        .drafts
        .mark_uploading(&id, "<cut@example.org>")
        .unwrap();
    server.deliver_id("Drafts", "cut");
    store.drafts.delete(&id).unwrap();
    let done = run(&mut server, &store).await;
    assert_eq!(done.deleted, [id]);
    assert!(copies(&server).is_empty());
}
