//! The UI's core features against Stalwart from
//! crates/envio-net/tests/stalwart/up.sh. Skips without it, unless
//! ENVIO_REQUIRE_STALWART=1.

mod support;

use envio_core::{Command, Event, FolderRole, SearchScope};
use log::LevelFilter;
use support::core_run::{CoreRun, NET, password};
use support::stalwart::{Stalwart, message, message_id};

const DAY: i64 = 86_400;

fn now_secs() -> i64 {
    support::now_ms() / 1_000
}

#[tokio::test]
async fn search_on_server_finds_mail_older_than_the_window() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    // A word no other run has used, so the search finds only this message.
    let word = format!(
        "zq{}",
        message_id().replace(|c: char| !c.is_ascii_alphanumeric(), "")
    );
    let subject = format!("Old report {word}");
    let creds = envio_net::Credentials::new("grace", &password("grace"));
    let mut session = server.imap(&creds).await;
    session
        .append_at(
            "INBOX",
            now_secs() - 60 * DAY,
            &message(&message_id(), &subject, 300),
        )
        .await
        .unwrap();
    session.logout().await.unwrap();

    let run = CoreRun::start(LevelFilter::Off);
    let account = run.add_account(&server, "grace");
    // The first sync has run once the Inbox is listed and shows its rows.
    let (inbox, _) = run.folder(&account, FolderRole::Inbox);
    run.rows(inbox);

    // The window is thirty days: sync did not fetch it, so a local search
    // misses it.
    let scope = SearchScope::Account(account.clone());
    run.core.send(Command::Search {
        scope: scope.clone(),
        query: word.clone(),
    });
    let local = run.wait(NET, |event| match event {
        Event::SearchResults {
            server: false,
            rows,
            ..
        } => Some(rows.clone()),
        _ => None,
    });
    assert!(local.is_empty(), "{local:?}");

    // Search on server finds it, and caches it.
    run.core.send(Command::SearchServer {
        scope: scope.clone(),
        query: word.clone(),
    });
    let found = run.wait(NET, |event| match event {
        Event::SearchResults {
            server: true, rows, ..
        } => Some(rows.clone()),
        Event::SearchFailed { text, .. } => panic!("search failed: {text}"),
        _ => None,
    });
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].subject, subject);
}
