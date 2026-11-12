//! What sync needs beyond Plan 4's operations, against Stalwart: hearing
//! about changes (IDLE, unsolicited replies), the retention window search
//! and deleting for good. Skips without Stalwart, unless
//! ENVIO_REQUIRE_STALWART=1.

mod support;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use envio_net::imap::{Flag, ImapSession, Role, Woke};
use support::stalwart::{Stalwart, alice, message, message_id};

async fn folder(session: &mut ImapSession, role: Role) -> String {
    let folders = session.list_folders().await.unwrap();
    folders
        .into_iter()
        .find(|f| f.role == Some(role))
        .unwrap()
        .path
}

fn now() -> i64 {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    i64::try_from(secs).unwrap()
}

/// Junk is the one folder no other envio-net test writes to, so IDLE
/// there hears only this test.
#[tokio::test]
async fn idle_wakes_on_stop_on_quiet_and_on_new_mail() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let mut session = server.imap(&alice()).await;
    assert!(session.caps().idle);
    let junk = folder(&mut session, Role::Junk).await;
    session.select(&junk).await.unwrap();

    let (session, woke) = session
        .idle(Duration::from_secs(60), async {})
        .await
        .unwrap();
    assert_eq!(woke, Woke::Stopped);

    let (session, woke) = session
        .idle(Duration::from_secs(1), std::future::pending())
        .await
        .unwrap();
    assert_eq!(woke, Woke::Quiet);

    let mut other = server.imap(&alice()).await;
    let id = message_id();
    let eml = message(&id, "Wake up", 300);
    let append = async {
        tokio::time::sleep(Duration::from_millis(500)).await;
        other.append(&junk, &[], &eml).await.unwrap();
    };
    let (idled, ()) = tokio::join!(
        session.idle(Duration::from_secs(30), std::future::pending()),
        append
    );
    let (mut session, woke) = idled.unwrap();
    assert_eq!(woke, Woke::Changed);

    // The session still works after DONE; clean up.
    let uids = session.search_message_id(&id).await.unwrap();
    session.delete(&uids).await.unwrap();
    session.logout().await.unwrap();
    other.logout().await.unwrap();
}

#[tokio::test]
async fn changes_reported_along_with_a_reply_are_taken_once() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let mut session = server.imap(&alice()).await;
    let drafts = folder(&mut session, Role::Drafts).await;
    session.select(&drafts).await.unwrap();
    assert!(!session.take_changes());

    let mut other = server.imap(&alice()).await;
    let id = message_id();
    other
        .append(&drafts, &[], &message(&id, "Draft", 200))
        .await
        .unwrap();
    session.noop().await.unwrap();
    assert!(session.take_changes());
    assert!(!session.take_changes());

    let uids = session.search_message_id(&id).await.unwrap();
    session.delete(&uids).await.unwrap();
    session.logout().await.unwrap();
    other.logout().await.unwrap();
}

#[tokio::test]
async fn the_window_search_takes_mail_from_its_first_day_on() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let mut session = server.imap(&alice()).await;
    let id = message_id();
    session
        .append("INBOX", &[], &message(&id, "Window", 200))
        .await
        .unwrap();
    session.select("INBOX").await.unwrap();
    let uid = session.search_message_id(&id).await.unwrap()[0];

    let week_ago = now() - 7 * 86_400;
    assert!(session.search_since(week_ago).await.unwrap().contains(&uid));
    let tomorrow = now() + 86_400;
    assert!(!session.search_since(tomorrow).await.unwrap().contains(&uid));
    session.logout().await.unwrap();
}

#[tokio::test]
async fn search_uids_below_returns_lower_uids() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let mut session = server.imap(&alice()).await;
    let first = message_id();
    let second = message_id();
    for id in [&first, &second] {
        session
            .append("INBOX", &[], &message(id, "Below", 200))
            .await
            .unwrap();
    }
    session.select("INBOX").await.unwrap();
    let low = session.search_message_id(&first).await.unwrap()[0];
    let high = session.search_message_id(&second).await.unwrap()[0];
    assert!(low < high);
    let below = session.search_uids_below(high).await.unwrap();
    assert!(below.contains(&low) && !below.contains(&high));
    assert!(below.windows(2).all(|w| w[0] < w[1]));
    assert!(session.search_uids_below(1).await.unwrap().is_empty());
    session.logout().await.unwrap();
}

#[tokio::test]
async fn search_all_returns_every_uid() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let mut session = server.imap(&alice()).await;
    let id = message_id();
    // Seen, so a search for unseen mail would miss it. The other tests
    // append to the same inbox in parallel, so the count only grows.
    session
        .append("INBOX", &[Flag::Seen], &message(&id, "All", 200))
        .await
        .unwrap();
    let state = session.select("INBOX").await.unwrap();
    let uid = session.search_message_id(&id).await.unwrap()[0];
    let all = session.search_all().await.unwrap();
    assert!(all.contains(&uid));
    assert!(all.len() as u32 >= state.exists);
    session.logout().await.unwrap();
}

#[tokio::test]
async fn a_deleted_message_is_gone_for_good() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let mut session = server.imap(&alice()).await;
    assert!(session.caps().uidplus);
    let id = message_id();
    session
        .append("INBOX", &[], &message(&id, "Delete me", 200))
        .await
        .unwrap();
    session.select("INBOX").await.unwrap();
    let uids = session.search_message_id(&id).await.unwrap();
    assert_eq!(uids.len(), 1);
    session.delete(&uids).await.unwrap();
    assert!(session.search_message_id(&id).await.unwrap().is_empty());
    assert!(session.fetch_flags(&uids, None).await.unwrap().is_empty());
    session.logout().await.unwrap();
}
