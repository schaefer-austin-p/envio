//! The UI's core features against Stalwart from
//! crates/envio-net/tests/stalwart/up.sh. Skips without it, unless
//! ENVIO_REQUIRE_STALWART=1.

mod support;

use std::time::{Duration, Instant};

use envio_core::{
    Command, ComposeId, DraftInput, Event, FolderRole, ImagesChoice, SearchScope, StartFrom,
};
use log::LevelFilter;
use support::core_run::{CoreRun, NET, password, setup_at};
use support::proxy::Proxy;
use support::stalwart::{Stalwart, message, message_id};
use support::ui_help::{deliver, draft_texts, unified, unique_word};

const DAY: i64 = 86_400;

fn now_secs() -> i64 {
    support::now_ms() / 1_000
}

#[tokio::test]
async fn search_on_server_finds_mail_older_than_the_window() {
    let _turn = TURN.lock().await;
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

/// Serial: tests sign in as the same users, and Stalwart drops SMTP TLS
/// connections when several of those sign-ins overlap.
static TURN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn compose_new(run: &CoreRun, compose: ComposeId, account: &str) {
    run.core.send(Command::StartCompose {
        compose,
        from: StartFrom::New {
            account: account.to_owned(),
        },
        draft_id: None,
        mailto: None,
    });
    run.wait(NET, |event| {
        matches!(event, Event::ComposeReady { .. }).then_some(())
    });
}

#[tokio::test]
async fn a_draft_survives_a_slow_first_sync() {
    let _turn = TURN.lock().await;
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let subject = format!("Waiting {}", unique_word());
    deliver(&server, "ivan", &subject).await;

    // IMAPS goes through a proxy. The account is added and synced at full
    // speed. Then the app stops, the cache is deleted (it is rebuilt from
    // the server) and the proxy holds each server batch for 3 s, so the
    // next start's first sync is stalled while the draft is written.
    let proxy = Proxy::start(&server.host, server.imaps);
    let run = CoreRun::start(LevelFilter::Off);
    let account = run.add_account_with("ivan", &setup_at(&server, "ivan", proxy.port));
    let (inbox, _) = run.folder(&account, FolderRole::Inbox);
    run.row_with(inbox, &subject);
    let cache = run.dir.path().join("cache");
    let run = run.restart_with(|_| {
        std::fs::remove_dir_all(&cache).unwrap();
        proxy.slow(Duration::from_secs(3));
    });

    let compose = ComposeId(1);
    compose_new(&run, compose, &account);
    let body = format!("Typed during the sync {}", unique_word());
    let draft = DraftInput {
        account: account.clone(),
        to: vec!["grace@envio.test".to_owned()],
        subject: "A draft".to_owned(),
        body: body.clone(),
        ..DraftInput::default()
    };
    let start = Instant::now();
    run.core.send(Command::DraftChanged { compose, draft });
    while !draft_texts(&run).iter().any(|t| t.contains(&body)) {
        assert!(
            start.elapsed() < Duration::from_millis(1_200),
            "the draft was not on disk within 1.2 s"
        );
        std::thread::sleep(Duration::from_millis(25));
    }

    // The sync really was stalled: the Inbox does not show the mail yet.
    // When the proxy lets go it does.
    run.core.send(Command::ListFolders {
        account: account.clone(),
    });
    let folders = run.wait(NET, |event| match event {
        Event::Folders { folders, .. } => Some(folders.clone()),
        _ => None,
    });
    if let Some(inbox) = folders.iter().find(|f| f.role == FolderRole::Inbox) {
        assert!(
            run.rows(inbox.id).iter().all(|r| r.subject != subject),
            "the first sync had finished; the proxy did not slow it"
        );
    }
    proxy.slow(Duration::ZERO);
    let (inbox, _) = run.folder(&account, FolderRole::Inbox);
    run.row_with(inbox, &subject);
    assert!(draft_texts(&run).iter().any(|t| t.contains(&body)));
}

#[tokio::test]
async fn the_unified_inbox_shows_two_accounts() {
    let _turn = TURN.lock().await;
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let word = unique_word();
    let (for_grace, for_heidi) = (format!("Grace {word}"), format!("Heidi {word}"));
    deliver(&server, "grace", &for_grace).await;
    deliver(&server, "heidi", &for_heidi).await;

    let run = CoreRun::start(LevelFilter::Off);
    let grace = run.add_account(&server, "grace");
    let heidi = run.add_account(&server, "heidi");
    // Wait until both accounts have synced their Inbox.
    for (account, subject) in [(&grace, &for_grace), (&heidi, &for_heidi)] {
        let (inbox, _) = run.folder(account, FolderRole::Inbox);
        run.row_with(inbox, subject);
    }

    let rows = unified(&run);
    let ours: Vec<_> = rows.iter().filter(|r| r.subject.ends_with(&word)).collect();
    assert_eq!(ours.len(), 2, "{ours:?}");
    assert!(
        ours.iter()
            .any(|r| r.account == grace && r.subject == for_grace)
    );
    assert!(
        ours.iter()
            .any(|r| r.account == heidi && r.subject == for_heidi)
    );
}

#[tokio::test]
async fn autocomplete_suggests_a_synced_sender() {
    let _turn = TURN.lock().await;
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let word = unique_word();
    let address = format!("{word}@sender.test");
    let subject = format!("From a stranger {word}");
    let eml = format!(
        "From: Quentin {word} <{address}>\r\nTo: grace@envio.test\r\nSubject: {subject}\r\n\
         Message-ID: {}\r\nDate: Sun, 08 Nov 2026 10:00:00 +0000\r\n\
         MIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nHi.\r\n",
        message_id()
    );
    let creds = envio_net::Credentials::new("grace", &password("grace"));
    let mut session = server.imap(&creds).await;
    session.append("INBOX", &[], eml.as_bytes()).await.unwrap();
    session.logout().await.unwrap();

    let run = CoreRun::start(LevelFilter::Off);
    let account = run.add_account(&server, "grace");
    let (inbox, _) = run.folder(&account, FolderRole::Inbox);
    run.row_with(inbox, &subject);

    // The first letters of the address, then a word of the name.
    for typed in [word[..word.len() - 3].to_owned(), "quentin".to_owned()] {
        run.core.send(Command::Autocomplete {
            query: typed.clone(),
        });
        let items = run.wait(NET, |event| match event {
            Event::Suggestions { query, items } if *query == typed => Some(items.clone()),
            _ => None,
        });
        assert!(
            items
                .iter()
                .any(|s| s.address == address && !s.from_contacts),
            "{typed}: {items:?}"
        );
    }
}

#[tokio::test]
async fn an_attachment_round_trips_through_send() {
    let _turn = TURN.lock().await;
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let run = CoreRun::start(LevelFilter::Off);
    let account = run.add_account(&server, "heidi");

    // Bytes that are not text, so a lossy path would show.
    let bytes: Vec<u8> = (0..=255u8).cycle().take(5_000).collect();
    let source_dir = tempfile::tempdir().unwrap();
    let source = source_dir.path().join("blob.bin");
    std::fs::write(&source, &bytes).unwrap();

    let compose = ComposeId(1);
    compose_new(&run, compose, &account);
    run.core.send(Command::Attach {
        compose,
        path: source,
    });
    let copy = run.wait(NET, |event| match event {
        Event::Attached { path, .. } => Some(path.clone()),
        Event::AttachFailed { text, .. } => panic!("attach failed: {text}"),
        _ => None,
    });
    // The picked file may vanish (Content Hub's copy is temporary).
    drop(source_dir);

    let subject = format!("With a file {}", unique_word());
    let draft = DraftInput {
        account: account.clone(),
        to: vec!["heidi@envio.test".to_owned()],
        subject: subject.clone(),
        body: "See the attachment.".to_owned(),
        attachments: vec![copy],
        ..DraftInput::default()
    };
    run.core.send(Command::Send { compose, draft });
    run.wait(NET, |event| match event {
        Event::Queued { .. } => Some(()),
        Event::ComposeRefused { text, .. } => panic!("refused: {text}"),
        _ => None,
    });

    let (sent, _) = run.folder(&account, FolderRole::Sent);
    let row = run.row_with(sent, &subject);
    run.core.send(Command::OpenMessage {
        id: row.id,
        images: ImagesChoice::Default,
        width_px: 360,
    });
    let view = run.wait(NET, |event| match event {
        Event::MessageOpened { id, view } if *id == row.id => Some(view.clone()),
        _ => None,
    });
    assert_eq!(view.attachments.len(), 1, "{:?}", view.attachments);
    assert_eq!(view.attachments[0].name, "blob.bin");
    let out = tempfile::tempdir().unwrap();
    run.core.send(Command::SaveAttachment {
        id: row.id,
        index: 0,
        dir: out.path().to_owned(),
    });
    let saved = run.wait(NET, |event| match event {
        Event::AttachmentSaved { path, .. } => Some(path.clone()),
        Event::AttachmentFailed { .. } => panic!("saving failed"),
        _ => None,
    });
    assert_eq!(std::fs::read(saved).unwrap(), bytes);
}
