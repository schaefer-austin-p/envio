//! Bodies downloaded ahead of the open (#102), through `Core::send` and
//! events only, against Stalwart from crates/envio-net/tests/stalwart/up.sh.
//! Skips without it, unless ENVIO_REQUIRE_STALWART=1.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use envio_core::hooks::{self, Point};
use envio_core::{Command, Event, FolderRole, ImagesChoice};
use envio_store::{BodyState, Db, MessageRowId};
use log::LevelFilter;
use support::core_run::{CoreRun, NET, password, setup_at};
use support::proxy::Proxy;
use support::stalwart::{Stalwart, message, message_id};

const USER: &str = "grace";
/// Past the 256 KB a body may have to be fetched ahead of the open.
const BIG: usize = 300 * 1024;

/// Puts a message with `subject` and about `len` bytes in the INBOX. It has
/// no Date header, so its date is the arrival: now.
async fn deliver(server: &Stalwart, subject: &str, len: usize) {
    let creds = envio_net::Credentials::new(USER, &password(USER));
    let mut session = server.imap(&creds).await;
    let eml = String::from_utf8(message(&message_id(), subject, len)).unwrap();
    let eml = eml.replace("Date: Sun, 08 Nov 2026 10:00:00 +0000\r\n", "");
    session.append("INBOX", &[], eml.as_bytes()).await.unwrap();
    session.logout().await.unwrap();
}

fn body_state(run: &CoreRun, id: MessageRowId) -> BodyState {
    Db::open(&run.paths().db())
        .unwrap()
        .message(id)
        .unwrap()
        .body
}

#[tokio::test]
async fn recent_small_bodies_are_downloaded_and_open_without_the_server() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let proxy = Proxy::start(&server.host, server.imaps);
    let tag = message_id();
    let small: Vec<String> = (1..=3).map(|n| format!("Ahead {n} {tag}")).collect();
    let big = format!("Ahead big {tag}");
    deliver(&server, &big, BIG).await;
    for subject in &small {
        deliver(&server, subject, 400).await;
    }
    let run = CoreRun::start(LevelFilter::Info);
    let account = run.add_account_with(USER, &setup_at(&server, USER, proxy.port));
    let (inbox, _) = run.folder(&account, FolderRole::Inbox);
    let ids: Vec<MessageRowId> = small.iter().map(|s| run.row_with(inbox, s).id).collect();
    let big_id = run.row_with(inbox, &big).id;

    // Nothing was opened, yet all three small bodies arrive.
    let end = Instant::now() + NET;
    while !ids
        .iter()
        .all(|id| body_state(&run, *id) == BodyState::Downloaded)
    {
        assert!(Instant::now() < end, "bodies never downloaded");
        std::thread::sleep(Duration::from_millis(200));
    }
    assert_eq!(body_state(&run, big_id), BodyState::Missing);

    // With the server gone, opening a prefetched message still works and
    // asks for no body.
    let fetched = Arc::new(AtomicBool::new(false));
    let seen = Arc::clone(&fetched);
    hooks::before_next(Point::BodyFetch, &account, move || {
        seen.store(true, Ordering::SeqCst);
    });
    proxy.cut();
    run.core().send(Command::OpenMessage {
        id: ids[0],
        images: ImagesChoice::Default,
        width_px: 360,
    });
    run.wait(Duration::from_secs(10), |event| match event {
        Event::MessageOpened { id, .. } if *id == ids[0] => Some(()),
        _ => None,
    });
    assert!(!fetched.load(Ordering::SeqCst), "the open fetched a body");

    // The big one was left for the open, which needs the server.
    run.core().send(Command::OpenMessage {
        id: big_id,
        images: ImagesChoice::Default,
        width_px: 360,
    });
    run.wait(Duration::from_secs(15), |event| match event {
        Event::MessageUnavailable { id, .. } if *id == big_id => Some(()),
        _ => None,
    });
}
