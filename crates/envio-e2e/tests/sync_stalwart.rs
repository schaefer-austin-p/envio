//! envio-sync over envio-net into envio-store, against Stalwart from
//! crates/envio-net/tests/stalwart/up.sh. Skips without it, unless
//! ENVIO_REQUIRE_STALWART=1.

mod support;

use envio_net::imap::ImapSession;
use envio_net::smtp::SmtpSession;
use envio_net::{Roots, Security};
use envio_store::{ActionKind, Flags, FolderRole, NewOutboxItem};
use envio_sync::UNDO_MS;
use envio_sync::driver::{AccountSynced, file_sent, record, send_outbox, sync_account, undo};
use support::stalwart::{Stalwart, message, message_id};
use support::{Store, User, now_ms};

/// A run that ended early or skipped a folder must fail the test, not pass it.
fn clean(run: AccountSynced) -> AccountSynced {
    assert!(run.stopped.is_none(), "run stopped: {:?}", run.stopped);
    assert!(run.refused.is_empty(), "refused: {:?}", run.refused);
    run
}

/// Puts `subjects` in the user's INBOX; returns their Message-IDs.
async fn deliver(session: &mut ImapSession, subjects: &[&str]) -> Vec<String> {
    let mut ids = Vec::new();
    for subject in subjects {
        let id = message_id();
        session
            .append("INBOX", &[], &message(&id, subject, 500))
            .await
            .unwrap();
        ids.push(id);
    }
    ids
}

async fn synced(
    server: &Stalwart,
    user: &User,
    subjects: &[&str],
) -> (ImapSession, Store, Vec<String>) {
    let mut session = server.imap(&user.credentials()).await;
    let ids = deliver(&mut session, subjects).await;
    let mut store = Store::new(user.name);
    clean(
        sync_account(
            &mut session,
            &mut store.local(),
            user.name,
            now_ms(),
            Some(30),
        )
        .await
        .unwrap(),
    );
    (session, store, ids)
}

#[tokio::test]
async fn a_first_sync_caches_the_inbox_and_a_second_fetches_nothing() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let carol = User::new("carol");
    let (mut session, mut store, _) = synced(&server, &carol, &["One", "Two"]).await;
    let mut subjects = store.subjects(FolderRole::Inbox);
    subjects.sort();
    assert_eq!(subjects, ["One", "Two"]);
    for role in [FolderRole::Sent, FolderRole::Drafts, FolderRole::Trash] {
        store.folder(role);
    }
    let again = clean(
        sync_account(
            &mut session,
            &mut store.local(),
            carol.name,
            now_ms(),
            Some(30),
        )
        .await
        .unwrap(),
    );
    assert!(!again.folders.is_empty());
    assert!(again.folders.iter().all(|(_, synced)| synced.fetched == 0));
    session.logout().await.unwrap();
}

#[tokio::test]
async fn a_flag_change_reaches_the_server_after_the_undo_window() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let dave = User::new("dave");
    let (mut session, mut store, _) = synced(&server, &dave, &["Read me"]).await;
    let inbox = store.folder(FolderRole::Inbox);
    let message = store
        .db
        .list(envio_store::ListRef::Folder(inbox.id), None, 1)
        .unwrap()[0]
        .clone();
    let kind = ActionKind::SetFlags {
        add: Flags::SEEN.with(Flags::FLAGGED),
        remove: Flags::empty(),
    };
    let now = now_ms();
    record(&mut store.local(), message.id, kind, now).unwrap();
    let run = clean(
        sync_account(
            &mut session,
            &mut store.local(),
            dave.name,
            now + UNDO_MS,
            Some(30),
        )
        .await
        .unwrap(),
    );
    assert_eq!(run.replayed.done, 1);
    session.logout().await.unwrap();
    let mut fresh = server.imap(&dave.credentials()).await;
    fresh.select("INBOX").await.unwrap();
    let flags = fresh.fetch_flags(&[message.uid], None).await.unwrap();
    assert!(flags[0].flags.seen && flags[0].flags.flagged);
    fresh.logout().await.unwrap();
}

#[tokio::test]
async fn delete_with_undo_then_delete_for_real_moves_to_trash() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let erin = User::new("erin");
    let (mut session, mut store, ids) = synced(&server, &erin, &["Keep", "Bin"]).await;
    let inbox = store.folder(FolderRole::Inbox);
    let messages = store
        .db
        .list(envio_store::ListRef::Folder(inbox.id), None, 10)
        .unwrap();
    let pick = |subject: &str| {
        messages
            .iter()
            .find(|message| message.envelope.subject == subject)
            .unwrap()
            .id
    };
    let now = now_ms();
    let taken = record(&mut store.local(), pick("Keep"), ActionKind::Delete, now).unwrap();
    assert!(undo(&mut store.local(), &taken).unwrap());
    record(&mut store.local(), pick("Bin"), ActionKind::Delete, now).unwrap();
    let run = clean(
        sync_account(
            &mut session,
            &mut store.local(),
            erin.name,
            now + UNDO_MS,
            Some(30),
        )
        .await
        .unwrap(),
    );
    assert_eq!(run.replayed.done, 1);
    assert!(run.replayed.reverted.is_empty());
    assert_eq!(store.subjects(FolderRole::Inbox), ["Keep"]);
    assert_eq!(store.subjects(FolderRole::Trash), ["Bin"]);
    session.logout().await.unwrap();
    // The server, on a fresh session: Keep (ids[0]) in INBOX, Bin (ids[1])
    // moved to Trash.
    let trash_path = store.folder(FolderRole::Trash).path;
    let mut fresh = server.imap(&erin.credentials()).await;
    fresh.select("INBOX").await.unwrap();
    assert_eq!(fresh.search_message_id(&ids[0]).await.unwrap().len(), 1);
    assert!(fresh.search_message_id(&ids[1]).await.unwrap().is_empty());
    fresh.select(&trash_path).await.unwrap();
    assert_eq!(fresh.search_message_id(&ids[1]).await.unwrap().len(), 1);
    assert!(fresh.search_message_id(&ids[0]).await.unwrap().is_empty());
    fresh.logout().await.unwrap();
}

#[tokio::test]
async fn sent_mail_is_filed_in_sent_once_and_leaves_the_outbox() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let frank = User::new("frank");
    let (mut session, mut store, _) = synced(&server, &frank, &[]).await;
    let id = message_id();
    let item = NewOutboxItem {
        send_after_ms: 0,
        account: frank.name.into(),
        message_id: id.clone(),
        queued_at_ms: now_ms(),
        from: frank.address.clone(),
        recipients: vec![frank.address.clone()],
    };
    store
        .outbox
        .enqueue(item, &message(&id, "To myself", 2_000))
        .unwrap();
    let endpoint = server.endpoint(server.submissions, Security::Tls);
    let mut smtp = SmtpSession::connect(&endpoint, &Roots::empty())
        .await
        .unwrap();
    smtp.login(&frank.credentials()).await.unwrap();
    let sent = send_outbox(&mut smtp, &mut store.local(), frank.name, now_ms())
        .await
        .unwrap();
    assert_eq!(sent.accepted.len(), 1);
    assert!(sent.failed.is_empty() && sent.unconfirmed.is_empty() && sent.stopped.is_none());
    smtp.quit().await.unwrap();
    for round in 0..2 {
        let filed = file_sent(&mut session, &mut store.local(), frank.name, false)
            .await
            .unwrap();
        assert!(filed.ask.is_empty() && filed.unfiled.is_empty());
        // The first run files it; the second finds nothing left to do.
        assert_eq!(filed.removed.len(), usize::from(round == 0));
    }
    assert!(store.outbox.items().unwrap().is_empty());
    let sent_path = store.folder(FolderRole::Sent).path;
    session.select(&sent_path).await.unwrap();
    assert_eq!(session.search_message_id(&id).await.unwrap().len(), 1);
    let found = session.search_message_id(&id).await.unwrap();
    let flags = session.fetch_flags(&found, None).await.unwrap();
    assert!(flags[0].flags.seen);
    session.logout().await.unwrap();
}
