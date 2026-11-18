//! The sync window and "Load older" over envio-net into envio-store,
//! against Stalwart from crates/envio-net/tests/stalwart/up.sh. Skips
//! without it, unless ENVIO_REQUIRE_STALWART=1.

mod support;

use envio_net::imap::ImapSession;
use envio_store::FolderRole;
use envio_sync::driver::{load_older, sync_folder, sync_folders};
use envio_sync::plan::sync::Kept;
use support::stalwart::{Stalwart, message, message_id};
use support::{Store, User, now_ms};

const DAY: i64 = 86_400;

/// `message` without its Date header, so the message's date is the day it
/// arrived (what `append_at` sets), not the header's fixed one.
fn undated(message_id: &str, subject: &str, body_len: usize) -> Vec<u8> {
    let eml = message(message_id, subject, body_len);
    let text = String::from_utf8(eml).unwrap();
    let start = text.find("Date: ").unwrap();
    let end = start + text[start..].find("\r\n").unwrap() + 2;
    format!("{}{}", &text[..start], &text[end..]).into_bytes()
}
/// Messages dated over 30 days ago, and within them.
const OUTSIDE: usize = 80;
const INSIDE: usize = 40;

/// Days before now that message `i` (0 = oldest, UID `i + 1`) arrived: 80
/// from 89 to 31 days ago, then 40 from 28 days ago to today. Nothing sits
/// on the 30-day edge, where the server's day rounding would decide.
fn days_ago(i: usize) -> i64 {
    let i = i64::try_from(i).unwrap();
    if i < i64::try_from(OUTSIDE).unwrap() {
        89 - i * 58 / 79
    } else {
        28 - (i - 80) * 28 / 39
    }
}

/// Seeds the user's INBOX with 120 messages spread over 89 days, and syncs
/// the folder list. Returns the session, the store and the Inbox's id.
async fn seeded(server: &Stalwart, user: &User) -> (ImapSession, Store, envio_store::FolderId) {
    let mut session = server.imap(&user.credentials()).await;
    let now = now_ms() / 1_000;
    for i in 0..OUTSIDE + INSIDE {
        session
            .append_at(
                "INBOX",
                now - days_ago(i) * DAY,
                &undated(&message_id(), &format!("Mail {i}"), 200),
            )
            .await
            .unwrap();
    }
    let mut store = Store::new(user.name);
    sync_folders(&mut session, &mut store.local(), user.name)
        .await
        .unwrap();
    let inbox = store.folder(FolderRole::Inbox).id;
    (session, store, inbox)
}

/// Syncs the folder to the end (a big folder takes several calls).
async fn sync(
    session: &mut ImapSession,
    store: &mut Store,
    inbox: envio_store::FolderId,
    (window, floor): (Option<u32>, Option<Kept>),
) {
    while sync_folder(
        session,
        &mut store.local(),
        inbox,
        now_ms(),
        window,
        floor.clone(),
    )
    .await
    .unwrap()
    .more
    {}
}

fn cached(store: &Store, inbox: envio_store::FolderId) -> usize {
    store.db.uids(inbox).unwrap().len()
}

#[tokio::test]
async fn first_sync_caches_every_message_in_the_window() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let (mut session, mut store, inbox) = seeded(&server, &User::new("judy")).await;
    sync(&mut session, &mut store, inbox, (Some(30), None)).await;
    // All of those dated within 30 days, and well over the 10 a
    // single batch step once left (#89).
    assert_eq!(cached(&store, inbox), INSIDE);
    assert!(cached(&store, inbox) > 10);
    session.logout().await.unwrap();
}

#[tokio::test]
async fn load_older_fetches_50_then_the_rest_then_is_exhausted() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let (mut session, mut store, inbox) = seeded(&server, &User::new("karl")).await;
    sync(&mut session, &mut store, inbox, (Some(30), None)).await;
    let first = load_older(
        &mut session,
        &mut store.local(),
        inbox,
        (now_ms(), Some(30)),
        None,
    )
    .await
    .unwrap();
    assert_eq!(first.fetched.len(), 50);
    assert!(!first.exhausted);
    assert_eq!(cached(&store, inbox), INSIDE + 50);
    let second = load_older(
        &mut session,
        &mut store.local(),
        inbox,
        (now_ms(), Some(30)),
        Some(first.kept()),
    )
    .await
    .unwrap();
    assert_eq!(second.fetched.len(), OUTSIDE - 50);
    assert!(second.exhausted);
    assert_eq!(cached(&store, inbox), INSIDE + OUTSIDE);
    session.logout().await.unwrap();
}

#[tokio::test]
async fn a_sync_with_the_floor_keeps_everything() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let (mut session, mut store, inbox) = seeded(&server, &User::new("liam")).await;
    sync(&mut session, &mut store, inbox, (Some(30), None)).await;
    let first = load_older(
        &mut session,
        &mut store.local(),
        inbox,
        (now_ms(), Some(30)),
        None,
    )
    .await
    .unwrap();
    sync(
        &mut session,
        &mut store,
        inbox,
        (Some(30), Some(first.kept())),
    )
    .await;
    assert_eq!(cached(&store, inbox), INSIDE + 50);
    session.logout().await.unwrap();
}

#[tokio::test]
async fn a_sync_without_the_floor_goes_back_to_the_window() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let (mut session, mut store, inbox) = seeded(&server, &User::new("mona")).await;
    sync(&mut session, &mut store, inbox, (Some(30), None)).await;
    load_older(
        &mut session,
        &mut store.local(),
        inbox,
        (now_ms(), Some(30)),
        None,
    )
    .await
    .unwrap();
    assert_eq!(cached(&store, inbox), INSIDE + 50);
    sync(&mut session, &mut store, inbox, (Some(30), None)).await;
    assert_eq!(cached(&store, inbox), INSIDE);
    session.logout().await.unwrap();
}

#[tokio::test]
async fn a_90_day_window_caches_everything() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let (mut session, mut store, inbox) = seeded(&server, &User::new("nina")).await;
    sync(&mut session, &mut store, inbox, (Some(90), None)).await;
    assert_eq!(cached(&store, inbox), INSIDE + OUTSIDE);
    // Window All reaches the same mail by UID SEARCH ALL.
    sync(&mut session, &mut store, inbox, (None, None)).await;
    assert_eq!(cached(&store, inbox), INSIDE + OUTSIDE);
    session.logout().await.unwrap();
}

#[tokio::test]
async fn load_older_reaches_old_mail_appended_after_recent_mail() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let user = User::new("olga");
    let mut session = server.imap(&user.credentials()).await;
    let now = now_ms() / 1_000;
    // Recent mail first, old mail appended after it: the old mail has the
    // higher UIDs (#94).
    for (days, count) in [(1, 10), (60, 60)] {
        for i in 0..count {
            session
                .append_at(
                    "INBOX",
                    now - days * DAY,
                    &undated(&message_id(), &format!("Mail {days} {i}"), 200),
                )
                .await
                .unwrap();
        }
    }
    let mut store = Store::new(user.name);
    sync_folders(&mut session, &mut store.local(), user.name)
        .await
        .unwrap();
    let inbox = store.folder(FolderRole::Inbox).id;
    sync(&mut session, &mut store, inbox, (Some(30), None)).await;
    assert_eq!(cached(&store, inbox), 10);
    let first = load_older(
        &mut session,
        &mut store.local(),
        inbox,
        (now_ms(), Some(30)),
        None,
    )
    .await
    .unwrap();
    assert_eq!(first.fetched.len(), 50);
    assert!(!first.exhausted);
    let second = load_older(
        &mut session,
        &mut store.local(),
        inbox,
        (now_ms(), Some(30)),
        Some(first.kept()),
    )
    .await
    .unwrap();
    assert_eq!(second.fetched.len(), 10);
    assert!(second.exhausted);
    assert_eq!(cached(&store, inbox), 70);
    session.logout().await.unwrap();
}
