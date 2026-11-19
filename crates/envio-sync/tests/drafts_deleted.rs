//! A draft deleted (or moved to Trash) in the server's Drafts folder loses
//! its local file once the delete has reached the server, so Upload and
//! Edit cannot bring it back (#284). Until then Undo can still restore it.

mod support;

use std::collections::BTreeSet;

use envio_mime::Address;
use envio_net::imap::Role;
use envio_store::{ActionKind, Draft, Journal};
use envio_sync::UNDO_MS;
use envio_sync::driver::{
    DraftRun, forget_deleted_drafts, record, replay, sync_drafts, sync_folder, sync_folders,
};
use support::fake::{FakeServer, NOW_MS};
use support::local::{ACCOUNT, Store};

const LATER: i64 = NOW_MS + UNDO_MS;

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

/// A draft uploaded and cached in the Drafts folder: its id.
async fn uploaded(server: &mut FakeServer, store: &mut Store, body: &str) -> String {
    let id = store.drafts.create(&draft(body, 1)).unwrap();
    let from = Address::new(Some("Carol"), "carol@example.org");
    let open = BTreeSet::new();
    let run = DraftRun {
        account: ACCOUNT,
        from: &from,
        open: &open,
        now_ms: NOW_MS,
    };
    sync_drafts(server, &store.db, &store.drafts, &run)
        .await
        .unwrap();
    let drafts = store.folder("Drafts").id;
    sync_folder(server, &mut store.local(), drafts, NOW_MS, Some(30), None)
        .await
        .unwrap();
    id
}

async fn setup() -> (FakeServer, Store) {
    let mut server = FakeServer::new();
    server.add_folder("Drafts", Some(Role::Drafts));
    server.add_folder("Trash", Some(Role::Trash));
    let mut store = Store::new();
    sync_folders(&mut server, &mut store.local(), ACCOUNT)
        .await
        .unwrap();
    (server, store)
}

fn queue(store: &mut Store, kind: ActionKind) {
    let drafts = store.folder("Drafts").id;
    let row = store.db.message_at(drafts, 1).unwrap().unwrap().id;
    record(&mut store.local(), row, kind, NOW_MS).unwrap();
}

async fn replay_and_forget(
    server: &mut FakeServer,
    store: &mut Store,
    now_ms: i64,
    open: &BTreeSet<String>,
) -> Vec<String> {
    replay(server, &mut store.local(), ACCOUNT, now_ms)
        .await
        .unwrap();
    forget(store, open)
}

fn forget(store: &mut Store, open: &BTreeSet<String>) -> Vec<String> {
    forget_deleted_drafts(
        (&store.db, &store.drafts),
        &mut store.journal,
        ACCOUNT,
        open,
    )
    .unwrap()
}

/// What a restart does to the journal: it is read from disk again.
fn restart(store: &mut Store) {
    store.journal = Journal::open(&store.dir.path().join("journal")).unwrap();
}

#[tokio::test]
async fn a_delete_in_drafts_removes_the_local_draft_after_it_reached_the_server() {
    let (mut server, mut store) = setup().await;
    let id = uploaded(&mut server, &mut store, "text").await;
    queue(&mut store, ActionKind::Delete);
    // Inside the Undo window the draft is still there.
    let early = replay_and_forget(&mut server, &mut store, LATER - 1, &BTreeSet::new()).await;
    assert!(early.is_empty());
    assert!(store.drafts.load(&id).is_ok());
    let gone = replay_and_forget(&mut server, &mut store, LATER, &BTreeSet::new()).await;
    assert_eq!(gone, std::slice::from_ref(&id));
    assert!(store.drafts.load(&id).is_err());
    assert!(store.drafts.list().unwrap().is_empty());
    // No note: a copy the user restores later must not be expunged.
    assert!(store.drafts.deletes().unwrap().is_empty());
}

#[tokio::test]
async fn a_move_to_trash_removes_the_local_draft_too() {
    let (mut server, mut store) = setup().await;
    let id = uploaded(&mut server, &mut store, "text").await;
    queue(&mut store, ActionKind::Move { to: "Trash".into() });
    let gone = replay_and_forget(&mut server, &mut store, LATER, &BTreeSet::new()).await;
    assert_eq!(gone, std::slice::from_ref(&id));
    assert!(store.drafts.load(&id).is_err());
}

#[tokio::test]
async fn a_draft_with_newer_unsent_edits_survives_the_delete_of_its_old_copy() {
    let (mut server, mut store) = setup().await;
    let id = uploaded(&mut server, &mut store, "text").await;
    store.drafts.save(&id, &draft("newer text", 2)).unwrap();
    queue(&mut store, ActionKind::Delete);
    let gone = replay_and_forget(&mut server, &mut store, LATER, &BTreeSet::new()).await;
    assert!(gone.is_empty());
    assert_eq!(store.drafts.load(&id).unwrap().body, "newer text");
}

#[tokio::test]
async fn a_forget_cut_short_after_the_commit_runs_on_the_next_start() {
    let (mut server, mut store) = setup().await;
    let id = uploaded(&mut server, &mut store, "text").await;
    queue(&mut store, ActionKind::Delete);
    // The replay commits the delete; the process dies before the forget.
    replay(&mut server, &mut store.local(), ACCOUNT, LATER)
        .await
        .unwrap();
    assert!(store.drafts.load(&id).is_ok());
    restart(&mut store);
    let gone = forget(&mut store, &BTreeSet::new());
    assert_eq!(gone, std::slice::from_ref(&id));
    assert!(store.drafts.load(&id).is_err());
}

#[tokio::test]
async fn a_forget_settles_once_so_it_does_not_run_twice() {
    let (mut server, mut store) = setup().await;
    let id = uploaded(&mut server, &mut store, "text").await;
    queue(&mut store, ActionKind::Move { to: "Trash".into() });
    replay(&mut server, &mut store.local(), ACCOUNT, LATER)
        .await
        .unwrap();
    assert_eq!(forget(&mut store, &BTreeSet::new()), [id]);
    restart(&mut store);
    assert!(store.journal.to_forget(ACCOUNT).is_empty());
}

#[tokio::test]
async fn an_action_still_pending_does_not_forget_its_draft() {
    let (mut server, mut store) = setup().await;
    let id = uploaded(&mut server, &mut store, "text").await;
    queue(&mut store, ActionKind::Delete);
    // Inside the Undo window: nothing is committed, nothing is owed.
    replay(&mut server, &mut store.local(), ACCOUNT, LATER - 1)
        .await
        .unwrap();
    assert!(forget(&mut store, &BTreeSet::new()).is_empty());
    assert!(store.drafts.load(&id).is_ok());
}

#[tokio::test]
async fn an_open_draft_keeps_its_file_and_the_intent_until_the_window_closes() {
    let (mut server, mut store) = setup().await;
    let id = uploaded(&mut server, &mut store, "text").await;
    queue(&mut store, ActionKind::Delete);
    replay(&mut server, &mut store.local(), ACCOUNT, LATER)
        .await
        .unwrap();
    let open = BTreeSet::from([id.clone()]);
    assert!(forget(&mut store, &open).is_empty());
    assert!(store.drafts.load(&id).is_ok());
    assert_eq!(store.journal.to_forget(ACCOUNT).len(), 1, "kept");
    // Closed without an edit: the next forget removes it.
    assert_eq!(forget(&mut store, &BTreeSet::new()), [id.as_str()]);
    assert!(store.drafts.load(&id).is_err());
    assert!(store.journal.to_forget(ACCOUNT).is_empty());
}

#[tokio::test]
async fn a_draft_edited_while_open_is_not_forgotten_when_it_closes() {
    let (mut server, mut store) = setup().await;
    let id = uploaded(&mut server, &mut store, "text").await;
    queue(&mut store, ActionKind::Delete);
    replay(&mut server, &mut store.local(), ACCOUNT, LATER)
        .await
        .unwrap();
    let open = BTreeSet::from([id.clone()]);
    assert!(forget(&mut store, &open).is_empty());
    store.drafts.save(&id, &draft("edited", 2)).unwrap();
    assert!(forget(&mut store, &BTreeSet::new()).is_empty());
    assert_eq!(store.drafts.load(&id).unwrap().body, "edited");
    assert!(store.journal.to_forget(ACCOUNT).is_empty(), "settled");
}

#[tokio::test]
async fn an_intent_the_cache_cannot_judge_waits_until_the_folders_are_back() {
    let (mut server, mut store) = setup().await;
    let id = uploaded(&mut server, &mut store, "text").await;
    queue(&mut store, ActionKind::Delete);
    replay(&mut server, &mut store.local(), ACCOUNT, LATER)
        .await
        .unwrap();
    // The cache was rebuilt: no folders known yet.
    let rebuilt = envio_store::Db::open_in_memory().unwrap();
    let kept = std::mem::replace(&mut store.db, rebuilt);
    assert!(forget(&mut store, &BTreeSet::new()).is_empty());
    assert_eq!(store.journal.to_forget(ACCOUNT).len(), 1);
    store.db = kept;
    assert_eq!(forget(&mut store, &BTreeSet::new()), [id]);
}

#[tokio::test]
async fn each_intent_settles_on_its_own() {
    let (mut server, mut store) = setup().await;
    let id = uploaded(&mut server, &mut store, "text").await;
    queue(&mut store, ActionKind::Delete);
    replay(&mut server, &mut store.local(), ACCOUNT, LATER)
        .await
        .unwrap();
    // A second committed action in a folder the cache does not know.
    let target = envio_store::MessageRef {
        account: ACCOUNT.into(),
        folder: "Elsewhere".into(),
        uidvalidity: 1,
        uid: 1,
        message_id: Some("<z@x>".into()),
    };
    let other = store
        .journal
        .push(NOW_MS, target, ActionKind::Delete)
        .unwrap();
    store.journal.commit(other.seq).unwrap();
    assert_eq!(forget(&mut store, &BTreeSet::new()), [id]);
    let left = store.journal.to_forget(ACCOUNT);
    assert_eq!(left.iter().map(|a| a.seq).collect::<Vec<_>>(), [other.seq]);
}

/// Restores a folder's mode when dropped, so a failed assert cannot leave
/// the temp dir unremovable.
struct Mode(std::path::PathBuf);

impl Drop for Mode {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
    }
}

#[tokio::test]
async fn a_transient_io_error_keeps_the_intent_and_a_later_run_forgets_the_draft() {
    use std::os::unix::fs::PermissionsExt;
    let (mut server, mut store) = setup().await;
    let id = uploaded(&mut server, &mut store, "text").await;
    queue(&mut store, ActionKind::Delete);
    replay(&mut server, &mut store.local(), ACCOUNT, LATER)
        .await
        .unwrap();
    // The draft's file cannot be removed: its folder is read-only.
    // Its "gone" marker folder too, or the draft would be hidden at once.
    let dir = store.dir.path().join("drafts");
    let dirs = [dir.join("gone"), dir.clone()];
    let _restore: Vec<Mode> = dirs.iter().cloned().map(Mode).collect();
    for dir in dirs.iter().filter(|dir| dir.exists()) {
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    }
    assert!(forget(&mut store, &BTreeSet::new()).is_empty());
    assert!(store.drafts.load(&id).is_ok(), "still there");
    assert_eq!(store.journal.to_forget(ACCOUNT).len(), 1, "intent kept");
    for dir in dirs.iter().filter(|dir| dir.exists()) {
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    assert_eq!(forget(&mut store, &BTreeSet::new()), [id.as_str()]);
    assert!(store.drafts.load(&id).is_err());
    assert!(store.journal.to_forget(ACCOUNT).is_empty());
}
