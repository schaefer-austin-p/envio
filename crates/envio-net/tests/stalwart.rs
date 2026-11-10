//! envio-net against a real server: Stalwart from tests/stalwart/up.sh.
//! Skips without it, unless ENVIO_REQUIRE_STALWART=1.

mod support;

use std::time::Duration;

use envio_net::imap::{Flag, ImapSession, Role};
use envio_net::smtp::SmtpSession;
use envio_net::{Credentials, NetError, Roots, Security};
use support::stalwart::{ALICE, BOB, Stalwart, alice, bob, message, message_id};

/// Polls bob's INBOX until `message_id` is there; returns how many copies.
async fn wait_for(server: &Stalwart, credentials: &Credentials, message_id: &str) -> usize {
    for _ in 0..50 {
        let mut session = server.imap(credentials).await;
        session.select("INBOX").await.unwrap();
        let found = session.search_message_id(message_id).await.unwrap();
        session.logout().await.unwrap();
        if !found.is_empty() {
            return found.len();
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    0
}

#[tokio::test]
async fn both_imap_ports_sign_in_with_the_pinned_certificate() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    for (port, security) in [
        (server.imaps, Security::Tls),
        (server.imap, Security::StartTls),
    ] {
        let endpoint = server.endpoint(port, security);
        let session = ImapSession::open(&endpoint, &Roots::empty(), &alice())
            .await
            .unwrap();
        assert!(session.caps().uidplus, "{security:?}");
        session.logout().await.unwrap();
    }
}

#[tokio::test]
async fn a_wrong_password_is_a_sign_in_error() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let endpoint = server.endpoint(server.imaps, Security::Tls);
    let wrong = Credentials::new("alice", "not-her-password");
    let err = ImapSession::open(&endpoint, &Roots::empty(), &wrong)
        .await
        .err()
        .unwrap();
    assert!(matches!(err, NetError::SignIn(_)), "{err:?}");
}

#[tokio::test]
async fn folders_come_with_their_roles() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let mut session = server.imap(&alice()).await;
    let folders = session.list_folders().await.unwrap();
    for role in [
        Role::Inbox,
        Role::Sent,
        Role::Drafts,
        Role::Trash,
        Role::Junk,
    ] {
        assert!(
            folders.iter().any(|f| f.role == Some(role) && f.selectable),
            "{role:?}: {folders:?}"
        );
    }
    session.logout().await.unwrap();
}

#[tokio::test]
async fn an_appended_message_reads_back_with_flags_and_changes() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let mut session = server.imap(&alice()).await;
    let id = message_id();
    let eml = message(&id, "Round trip", 2_000);
    session.append("INBOX", &[Flag::Seen], &eml).await.unwrap();
    let state = session.select("INBOX").await.unwrap();
    let uids = session.search_message_id(&id).await.unwrap();
    assert_eq!(uids.len(), 1);
    let uid = uids[0];
    assert!(uid < state.uid_next);

    let summaries = session.fetch_summaries(&[uid]).await.unwrap();
    assert_eq!(summaries.len(), 1);
    assert!(summaries[0].flags.seen && !summaries[0].flags.flagged);
    assert!(String::from_utf8_lossy(&summaries[0].header).contains(&id));
    assert!(summaries[0].internal_date.is_some());
    assert_eq!(
        session.fetch_body(uid, eml.len()).await.unwrap().unwrap(),
        eml
    );

    // CONDSTORE: only the message whose flags changed comes back. Other
    // tests append to the same INBOX in parallel, so keep our UID only.
    let before = session.select("INBOX").await.unwrap().highest_modseq;
    let before = before.expect("Stalwart has CONDSTORE");
    session
        .store_flags(
            &[uid],
            true,
            &[Flag::Flagged, Flag::Keyword("$Forwarded".into())],
        )
        .await
        .unwrap();
    let changed: Vec<_> = session
        .fetch_flags_from(1, Some(before))
        .await
        .unwrap()
        .into_iter()
        .filter(|update| update.uid == uid)
        .collect();
    assert_eq!(changed.len(), 1, "{changed:?}");
    assert!(
        changed[0].flags.flagged && changed[0].flags.keywords.contains(&"$Forwarded".to_owned())
    );
    session
        .store_flags(&[uid], false, &[Flag::Flagged])
        .await
        .unwrap();
    assert!(
        !session.fetch_flags(&[uid], None).await.unwrap()[0]
            .flags
            .flagged
    );
    session.logout().await.unwrap();
}

#[tokio::test]
async fn a_moved_message_leaves_the_folder() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let mut session = server.imap(&alice()).await;
    let folders = session.list_folders().await.unwrap();
    let trash = folders
        .iter()
        .find(|f| f.role == Some(Role::Trash))
        .unwrap()
        .path
        .clone();
    let id = message_id();
    session
        .append("INBOX", &[], &message(&id, "Move me", 500))
        .await
        .unwrap();
    session.select("INBOX").await.unwrap();
    let uids = session.search_message_id(&id).await.unwrap();
    session.move_to(&uids, &trash).await.unwrap();
    assert!(session.search_message_id(&id).await.unwrap().is_empty());
    session.select(&trash).await.unwrap();
    assert_eq!(session.search_message_id(&id).await.unwrap().len(), 1);
    session.logout().await.unwrap();
}

#[tokio::test]
async fn non_ascii_text_search_finds_the_message() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let mut session = server.imap(&alice()).await;
    assert!(session.caps().literal_plus);
    let id = message_id();
    let word = format!("Grüße{}", &id[1..12]);
    session
        .append("INBOX", &[], &message(&id, &word, 200))
        .await
        .unwrap();
    session.select("INBOX").await.unwrap();
    let mut found = Vec::new();
    // Stalwart indexes in the background.
    for _ in 0..50 {
        found = session.search_text(&word).await.unwrap();
        if !found.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert!(!found.is_empty());
    session.logout().await.unwrap();
}

#[tokio::test]
async fn mail_sent_on_either_smtp_port_reaches_the_inbox() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    for (port, security) in [
        (server.submissions, Security::Tls),
        (server.submission, Security::StartTls),
    ] {
        let mut smtp = SmtpSession::connect(&server.endpoint(port, security), &Roots::empty())
            .await
            .unwrap();
        smtp.login(&alice()).await.unwrap();
        let id = message_id();
        smtp.send(ALICE, &[BOB.to_owned()], &message(&id, "Hello Bob", 1_000))
            .await
            .unwrap();
        smtp.quit().await.unwrap();
        assert_eq!(wait_for(&server, &bob(), &id).await, 1, "{security:?}");
    }
}

#[tokio::test]
async fn smtp_with_a_wrong_password_is_a_sign_in_error() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let endpoint = server.endpoint(server.submissions, Security::Tls);
    let mut smtp = SmtpSession::connect(&endpoint, &Roots::empty())
        .await
        .unwrap();
    let err = smtp
        .login(&Credentials::new("alice", "nope"))
        .await
        .unwrap_err();
    assert!(matches!(err, NetError::SignIn(_)), "{err:?}");
}
