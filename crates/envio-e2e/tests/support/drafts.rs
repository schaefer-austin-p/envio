//! Helpers for the drafts tests: Drafts as the server holds it, a draft put
//! there by another client, and Edit-and-save through `Core::send`.

use std::time::{Duration, Instant};

use envio_core::{Command, ComposeId, Event, StartFrom};
use envio_net::imap::Flag;

use super::core_run::{CoreRun, NET, password};
use super::stalwart::{Stalwart, message_id};

pub const USER: &str = "grace";

/// Live messages in the user's Drafts folder whose subject has `needle`.
pub async fn on_server(server: &Stalwart, drafts: &str, needle: &str) -> usize {
    let creds = envio_net::Credentials::new(USER, &password(USER));
    let mut session = server.imap(&creds).await;
    let state = session.select(drafts).await.unwrap();
    let all: Vec<u32> = (1..state.uid_next).collect();
    let found = session
        .fetch_summaries(&all)
        .await
        .unwrap()
        .iter()
        .filter(|m| !m.flags.deleted && String::from_utf8_lossy(&m.header).contains(needle))
        .count();
    session.logout().await.unwrap();
    found
}

/// Waits until the server holds exactly `want` drafts with `needle`.
pub async fn settles_at(server: &Stalwart, drafts: &str, needle: &str, want: usize) {
    let end = Instant::now() + NET;
    loop {
        let found = on_server(server, drafts, needle).await;
        if found == want {
            return;
        }
        assert!(
            Instant::now() < end,
            "{needle}: {found} copies in Drafts, wanted {want}"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// A draft as another client leaves it in Drafts: `extra` headers, From as
/// Envio would write it (the account's name and address).
pub async fn put_draft(
    server: &Stalwart,
    drafts: &str,
    (subject, extra): (&str, &str),
    body: &str,
) {
    let creds = envio_net::Credentials::new(USER, &password(USER));
    let mut session = server.imap(&creds).await;
    let eml = format!(
        "From: {USER} <{USER}@envio.test>\r\nTo: grace@envio.test\r\n{extra}\
Subject: {subject}\r\nMessage-ID: {}\r\nDate: Thu, 19 Nov 2026 12:00:00 +0000\r\n\
MIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{body}\r\n",
        message_id()
    );
    let flags = [Flag::Draft, Flag::Seen];
    session
        .append(drafts, &flags, eml.as_bytes())
        .await
        .unwrap();
    session.logout().await.unwrap();
}

/// What the live (not \Deleted) messages of Drafts hold now: the Subject
/// line of each header block and the body text.
pub async fn drafts_now(server: &Stalwart, drafts: &str, tag: &str) -> Vec<(String, String)> {
    let creds = envio_net::Credentials::new(USER, &password(USER));
    let mut session = server.imap(&creds).await;
    let state = session.select(drafts).await.unwrap();
    let all: Vec<u32> = (1..state.uid_next).collect();
    let mut found = Vec::new();
    for m in session.fetch_summaries(&all).await.unwrap() {
        let header = String::from_utf8_lossy(&m.header).into_owned();
        if m.flags.deleted || !header.contains(tag) {
            continue;
        }
        let subject = header
            .lines()
            .find_map(|l| l.strip_prefix("Subject: "))
            .unwrap_or_default()
            .trim()
            .to_owned();
        let body = session.fetch_body(m.uid, m.size as usize).await.unwrap();
        let body = String::from_utf8_lossy(&body.unwrap_or_default()).into_owned();
        found.push((subject, body));
    }
    session.logout().await.unwrap();
    found
}

/// Edits the server draft with `subject`, changes it to `edited` and
/// leaves compose, which saves (and uploads) it.
pub fn edit_and_save(
    run: &CoreRun,
    (account, folder): (&str, envio_core::FolderId),
    subject: &str,
    edited: &str,
) {
    run.core().send(Command::Refresh {
        account: Some(account.to_owned()),
    });
    edit_shown_and_save(run, (account, folder), subject, edited);
}

/// `edit_and_save` without the refresh first: the cache is as the last
/// sync left it.
pub fn edit_shown_and_save(
    run: &CoreRun,
    (account, folder): (&str, envio_core::FolderId),
    subject: &str,
    edited: &str,
) {
    let row = run.row_with(folder, subject);
    let compose = ComposeId(1);
    run.core().send(Command::StartCompose {
        compose,
        from: StartFrom::Edit { id: row.id },
        draft_id: None,
        mailto: None,
    });
    let mut draft = run.wait(NET, |event| match event {
        Event::ComposeReady { draft, .. } => Some(draft.clone()),
        _ => None,
    });
    assert_eq!(draft.account, account);
    // A person edits well after the draft opened. A change stamped in the
    // very millisecond of the import would look like the import's own
    // (linked drafts count as uploaded at that stamp).
    std::thread::sleep(Duration::from_millis(50));
    draft.subject = edited.to_owned();
    draft.body = "Changed text.".to_owned();
    run.core().send(Command::DraftChanged { compose, draft });
    run.core().send(Command::CloseCompose {
        compose,
        discard: false,
    });
}
