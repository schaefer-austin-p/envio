//! envio-core against Stalwart from crates/envio-net/tests/stalwart/up.sh,
//! through `Core::send` and events only. Skips without it, unless
//! ENVIO_REQUIRE_STALWART=1. Tests share users, so each finds its own
//! messages by a unique subject.

mod support;

use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use envio_core::hooks::{self, Point};
use envio_core::secrets::Secrets;
use envio_core::{
    AccountStatus, Command, ComposeId, DraftInput, Event, FolderRole, Service, StartFrom,
    UserAction,
};
use envio_net::Fingerprint;
use log::LevelFilter;
use secrecy::SecretString;
use support::core_run::{CoreRun, NET, connect, password, setup, setup_at};
use support::proxy::Proxy;
use support::stalwart::{Stalwart, message, message_id};

const UNDO_WAIT: Duration = Duration::from_secs(8);

static TURN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The server, once it is this test's turn. The tests share users, and
/// Stalwart closes SMTP TLS connections without a close_notify when several
/// `Connect`s sign in as one user at once (the core rightly calls that
/// Temporary, "Could not reach the server"), so the tests run one at a time.
async fn find() -> Option<(Stalwart, tokio::sync::MutexGuard<'static, ()>)> {
    let turn = TURN.lock().await;
    Stalwart::find().await.map(|server| (server, turn))
}

/// Tells when the account's server accepts IDLE next: mail that arrives
/// after that is reported, so tests deliver after it, never on a timer.
fn idle_entered(account: &str) -> std::sync::mpsc::Receiver<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    hooks::before_next(Point::IdleEntered, account, move || {
        if let Ok(tx) = tx.lock() {
            let _ = tx.send(());
        }
    });
    rx
}

fn unique(tag: &str) -> String {
    let id = message_id();
    format!("{tag} {}", id.trim_matches(['<', '>']))
}

/// Puts a message with `subject` in the user's INBOX; returns its Message-ID.
async fn deliver(server: &Stalwart, user: &str, subject: &str) -> String {
    let mut session = server.imap(&creds(user)).await;
    let id = message_id();
    session
        .append("INBOX", &[], &message(&id, subject, 500))
        .await
        .unwrap();
    session.logout().await.unwrap();
    id
}

fn creds(user: &str) -> envio_net::Credentials {
    envio_net::Credentials::new(user, &password(user))
}

/// The UIDs in `folder` of the message with `id`, on a fresh session.
async fn on_server(server: &Stalwart, user: &str, folder: &str, id: &str) -> Vec<u32> {
    let mut session = server.imap(&creds(user)).await;
    session.select(folder).await.unwrap();
    let found = session.search_message_id(id).await.unwrap();
    session.logout().await.unwrap();
    found
}

/// Polls the server until `ready` accepts the number of copies in grace's
/// `folder`.
async fn server_settles(
    server: &Stalwart,
    folder: &str,
    id: &str,
    ready: impl Fn(usize) -> bool,
) -> usize {
    let end = Instant::now() + NET;
    loop {
        let count = on_server(server, "grace", folder, id).await.len();
        if ready(count) {
            return count;
        }
        assert!(Instant::now() < end, "{folder} has {count} of {id}");
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

async fn flagged_on_server(server: &Stalwart, user: &str, id: &str) -> bool {
    let mut session = server.imap(&creds(user)).await;
    session.select("INBOX").await.unwrap();
    let uids = session.search_message_id(id).await.unwrap();
    let flags = session.fetch_flags(&uids, None).await.unwrap();
    session.logout().await.unwrap();
    flags.first().is_some_and(|f| f.flags.flagged)
}

fn closed(run: &CoreRun, undo: envio_core::UndoId) -> bool {
    run.wait(UNDO_WAIT, |event| match event {
        Event::UndoClosed { undo: u, undone } if *u == undo => Some(*undone),
        _ => None,
    })
}

fn offered(
    run: &CoreRun,
    ids: Vec<envio_core::MessageRowId>,
    action: UserAction,
) -> envio_core::UndoId {
    run.core.send(Command::Act { ids, action });
    run.wait(NET, |event| match event {
        Event::UndoOffer { undo, .. } => Some(*undo),
        _ => None,
    })
}

#[tokio::test]
async fn add_account_sync_act_undo_send_and_find_in_sent() {
    let Some((server, _turn)) = find().await else {
        return;
    };
    let pin = server.pin.to_hex();
    let flag = unique("Flag me");
    let keep = unique("Keep me");
    let bin = unique("Bin me");
    let flag_id = deliver(&server, "grace", &flag).await;
    let keep_id = deliver(&server, "grace", &keep).await;
    let bin_id = deliver(&server, "grace", &bin).await;

    // Setup with trust on first use: IMAP, then SMTP.
    let run = CoreRun::start(LevelFilter::Info);
    let setup = setup(&server, "grace");
    run.core.send(connect(setup.clone(), "grace", vec![]));
    let (service, hex) = run.wait(NET, |event| match event {
        Event::TrustPrompt { service, hex, .. } => Some((*service, hex.clone())),
        _ => None,
    });
    assert_eq!((service, hex.as_str()), (Service::Imap, pin.as_str()));
    let imap = vec![(Service::Imap, pin.clone())];
    run.core.send(connect(setup.clone(), "grace", imap.clone()));
    let (service, hex) = run.wait(NET, |event| match event {
        Event::TrustPrompt { service, hex, .. } => Some((*service, hex.clone())),
        _ => None,
    });
    assert_eq!((service, hex.as_str()), (Service::Smtp, pin.as_str()));
    let both = vec![(Service::Imap, pin.clone()), (Service::Smtp, pin.clone())];
    run.core.send(connect(setup, "grace", both));
    let account = run.wait(NET, |event| match event {
        Event::AccountAdded { account } => Some(account.key.clone()),
        _ => None,
    });
    let passwords = run.dir.path().join("data/passwords.json");
    let mode = std::fs::metadata(&passwords).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
    let accounts = std::fs::read_to_string(run.paths().accounts()).unwrap();
    assert!(!accounts.contains("grace-secret-1"), "{accounts}");

    // Sync: the delivered messages show in the Inbox.
    let (inbox, _) = run.folder(&account, FolderRole::Inbox);
    let flag_row = run.row_with(inbox, &flag);
    let keep_row = run.row_with(inbox, &keep);
    let bin_row = run.row_with(inbox, &bin);

    // Flag: offered, and after the window the server has \Flagged.
    let undo = offered(&run, vec![flag_row.id], UserAction::Flag(true));
    assert!(!closed(&run, undo));
    let end = Instant::now() + NET;
    while !flagged_on_server(&server, "grace", &flag_id).await {
        assert!(Instant::now() < end, "the server never got \\Flagged");
        tokio::time::sleep(Duration::from_millis(300)).await;
    }

    // Delete, then Undo inside the window: the row is back, the server
    // never moved it.
    let undo = offered(&run, vec![keep_row.id], UserAction::Delete);
    run.core.send(Command::Undo { undo });
    assert!(closed(&run, undo));
    run.row_with(inbox, &keep);
    server_settles(&server, "INBOX", &keep_id, |n| n == 1).await;

    // Delete and let it expire: the message ends in Trash on the server.
    let undo = offered(&run, vec![bin_row.id], UserAction::Delete);
    assert!(!closed(&run, undo));
    let (_, trash) = run.folder(&account, FolderRole::Trash);
    server_settles(&server, &trash, &bin_id, |n| n == 1).await;
    assert!(
        on_server(&server, "grace", "INBOX", &bin_id)
            .await
            .is_empty()
    );
    // The undone message stayed: still in INBOX, never in Trash, though its
    // delete was replayed together with the later one.
    assert_eq!(
        on_server(&server, "grace", "INBOX", &keep_id).await.len(),
        1
    );
    assert!(
        on_server(&server, "grace", &trash, &keep_id)
            .await
            .is_empty()
    );

    // Send to herself: queued, the Outbox empties, Sent has it once and
    // it arrives in the Inbox.
    let subject = unique("To myself");
    let compose = ComposeId(1);
    run.core.send(Command::StartCompose {
        compose,
        from: StartFrom::New {
            account: account.clone(),
        },
        draft_id: None,
        mailto: None,
    });
    run.wait(NET, |event| {
        matches!(event, Event::ComposeReady { .. }).then_some(())
    });
    let draft = DraftInput {
        account: account.clone(),
        to: vec!["grace@envio.test".to_owned()],
        subject: subject.clone(),
        body: "Hello, me.".to_owned(),
        ..DraftInput::default()
    };
    run.core.send(Command::Send { compose, draft });
    run.wait(NET, |event| {
        matches!(event, Event::Queued { .. }).then_some(())
    });
    // Filing in Sent removes the item; the core says so unasked.
    run.wait(NET, |event| match event {
        Event::Outbox { items } if items.is_empty() => Some(()),
        _ => None,
    });
    run.core.send(Command::ListOutbox);
    run.wait(NET, |event| match event {
        Event::Outbox { items } => {
            assert!(items.is_empty(), "{items:?}");
            Some(())
        }
        _ => None,
    });
    let (_, sent) = run.folder(&account, FolderRole::Sent);
    let mut session = server.imap(&creds("grace")).await;
    let state = session.select(&sent).await.unwrap();
    let all: Vec<u32> = (1..state.uid_next).collect();
    let copies = session
        .fetch_summaries(&all)
        .await
        .unwrap()
        .iter()
        .filter(|m| String::from_utf8_lossy(&m.header).contains(&subject))
        .count();
    assert_eq!(copies, 1, "Sent must hold the message exactly once");
    session.logout().await.unwrap();
    run.row_with(inbox, &subject);
}

#[tokio::test]
async fn a_changed_pin_stops_and_shows_both_fingerprints() {
    let Some((server, _turn)) = find().await else {
        return;
    };
    let proxy = Proxy::start(&server.host, server.imaps);
    let run = CoreRun::start(LevelFilter::Info);
    let account = run.add_account_with("grace", &setup_at(&server, "grace", proxy.port));
    let fake = "ab".repeat(32);
    let key = account.clone();
    let run = run.restart_with(|paths| {
        let mut accounts = envio_store::Accounts::open(&paths.accounts()).unwrap();
        let mut settings = accounts.get(&key).unwrap().settings.clone();
        settings.imap.pin = Some(fake.clone());
        accounts.update(&key, settings).unwrap();
    });
    let details = run.wait(NET, |event| match event {
        Event::AccountStatus {
            status: AccountStatus::Certificate(Some(details)),
            ..
        } => Some(details.clone()),
        _ => None,
    });
    assert_eq!(details.service, Service::Imap);
    assert_eq!(details.pinned, Fingerprint::from_hex(&"ab".repeat(32)));
    assert_eq!(details.presented, server.pin);
    // Stopped: nothing is synced, nothing connects. A message is waiting, so
    // a sync that wrongly ran would show it.
    let waiting = unique("Waiting behind a changed pin");
    deliver(&server, "grace", &waiting).await;
    let attempts = proxy.connections();
    let synced = run.happens(Duration::from_secs(3), |event| {
        matches!(
            event,
            Event::FoldersChanged { .. } | Event::MessagesChanged { .. }
        )
    });
    assert!(!synced, "the account synced against a changed certificate");
    assert_eq!(
        proxy.connections(),
        attempts,
        "the account connected again against a changed certificate"
    );
}

#[tokio::test]
async fn a_wrong_password_pauses_for_sign_in() {
    let Some((server, _turn)) = find().await else {
        return;
    };
    let proxy = Proxy::start(&server.host, server.imaps);
    let run = CoreRun::start(LevelFilter::Info);
    let account = run.add_account_with("grace", &setup_at(&server, "grace", proxy.port));
    run.folder(&account, FolderRole::Inbox);
    // The password changed at the provider: the stored one no longer works
    // (a live connection would not notice), so the next start is refused.
    let data = run.dir.path().join("data");
    let key = account.clone();
    let run = run.restart_with(|_| {
        let mut secrets = Secrets::open(&data).unwrap();
        secrets
            .set(&key, SecretString::from("stale-password".to_owned()))
            .unwrap();
    });
    let signed_out = |event: &Event| match event {
        Event::AccountStatus {
            status: AccountStatus::SignInFailed,
            ..
        } => Some(()),
        _ => None,
    };
    run.wait(NET, signed_out);
    // Another wrong one is tried once and refused.
    run.core.send(Command::UpdatePassword {
        account: account.clone(),
        password: SecretString::from("not-the-password".to_owned()),
    });
    run.wait(NET, signed_out);
    // Paused: no second attempt. A status event would not show one (the
    // status stays SignInFailed), a connection through the proxy does.
    let tried = proxy.connections();
    run.happens(Duration::from_secs(5), |_| false);
    assert_eq!(
        proxy.connections(),
        tried,
        "the core tried to sign in again by itself"
    );
    run.core.send(Command::UpdatePassword {
        account,
        password: SecretString::from(password("grace")),
    });
    run.wait(NET, |event| match event {
        Event::AccountStatus {
            status: AccountStatus::Ready,
            ..
        } => Some(()),
        _ => None,
    });
}

#[tokio::test]
async fn a_non_ascii_password_signs_in() {
    let Some((server, _turn)) = find().await else {
        return;
    };
    let run = CoreRun::start(LevelFilter::Info);
    // `add_account` fails on a setup problem: IMAP (AUTHENTICATE PLAIN)
    // and SMTP both accepted the password.
    let account = run.add_account(&server, "ivan");
    run.folder(&account, FolderRole::Inbox);
}

#[tokio::test]
async fn new_mail_wakes_idle_after_a_full_sync() {
    let Some((server, _turn)) = find().await else {
        return;
    };
    let run = CoreRun::start(LevelFilter::Info);
    let account = run.add_account(&server, "grace");
    let (inbox, _) = run.folder(&account, FolderRole::Inbox);
    let idle = idle_entered(&account);
    idle.recv_timeout(NET).expect("IDLE was never entered");
    let subject = unique("Wake up");
    deliver(&server, "grace", &subject).await;
    run.row_with(inbox, &subject);
}

#[tokio::test]
async fn idle_comes_back_after_the_connection_drops() {
    let Some((server, _turn)) = find().await else {
        return;
    };
    let proxy = Proxy::start(&server.host, server.imaps);
    let run = CoreRun::start(LevelFilter::Info);
    let account = run.add_account_with("grace", &setup_at(&server, "grace", proxy.port));
    let (inbox, _) = run.folder(&account, FolderRole::Inbox);
    let idle = idle_entered(&account);
    idle.recv_timeout(NET).expect("IDLE was never entered");
    let first = unique("Before the drop");
    deliver(&server, "grace", &first).await;
    run.row_with(inbox, &first);

    proxy.cut();
    run.wait(NET, |event| match event {
        Event::AccountStatus {
            status: AccountStatus::Offline { .. },
            ..
        } => Some(()),
        _ => None,
    });
    // Mail arrives while the connection is down; the retry finds it.
    let during = unique("During the drop");
    deliver(&server, "grace", &during).await;
    let idle = idle_entered(&account);
    proxy.up();
    run.wait(Duration::from_secs(90), |event| match event {
        Event::AccountStatus {
            status: AccountStatus::Ready,
            ..
        } => Some(()),
        _ => None,
    });
    run.row_with(inbox, &during);
    // And IDLE is back: new mail arrives without a Refresh.
    idle.recv_timeout(NET).expect("IDLE did not come back");
    let after = unique("After the drop");
    deliver(&server, "grace", &after).await;
    run.row_with(inbox, &after);
}

#[tokio::test]
async fn mail_that_arrives_before_the_idle_select_is_not_missed() {
    let Some((server, _turn)) = find().await else {
        return;
    };
    let server = Arc::new(server);
    let run = CoreRun::start(LevelFilter::Info);
    let account = run.add_account(&server, "grace");
    let (inbox, _) = run.folder(&account, FolderRole::Inbox);
    let late = unique("Between sync and IDLE");
    let hook_server = Arc::clone(&server);
    let hook_subject = late.clone();
    hooks::before_next(Point::IdleSelect, &account, move || {
        // On the worker thread, after the Inbox was synced and before its
        // SELECT for IDLE: the mail lands in exactly that gap.
        let server = Arc::clone(&hook_server);
        let subject = hook_subject.clone();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(deliver(&server, "grace", &subject));
        })
        .join()
        .unwrap();
    });
    run.core.send(Command::Refresh {
        account: Some(account),
    });
    run.row_with(inbox, &late);
}

#[tokio::test]
async fn an_open_whose_body_cannot_be_fetched_answers_with_a_failure() {
    let Some((server, _turn)) = find().await else {
        return;
    };
    let proxy = Arc::new(Proxy::start(&server.host, server.imaps));
    let subject = unique("Never fetched");
    deliver(&server, "grace", &subject).await;
    let run = CoreRun::start(LevelFilter::Info);
    let account = run.add_account_with("grace", &setup_at(&server, "grace", proxy.port));
    let (inbox, _) = run.folder(&account, FolderRole::Inbox);
    let row = run.row_with(inbox, &subject);
    // The connection dies as the body is about to be fetched.
    let cut = Arc::clone(&proxy);
    hooks::before_next(Point::BodyFetch, &account, move || cut.cut());
    run.core.send(Command::OpenMessage {
        id: row.id,
        images: false,
        width_px: 360,
    });
    run.wait(Duration::from_secs(15), |event| match event {
        Event::MessageUnavailable { id, .. } if *id == row.id => Some(()),
        _ => None,
    });
}
