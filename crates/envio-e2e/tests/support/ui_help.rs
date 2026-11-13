//! Small helpers of the UI tests against Stalwart.

use envio_core::{Command, ComposeId, Event, ListRef, MessageRow, StartFrom};

use super::core_run::{CoreRun, NET, password};
use super::stalwart::{Stalwart, message, message_id};

/// A word no other run has used.
pub fn unique_word() -> String {
    format!(
        "zq{}",
        message_id().replace(|c: char| !c.is_ascii_alphanumeric(), "")
    )
}

/// Puts a plain message with `subject` in the user's INBOX.
pub async fn deliver(server: &Stalwart, user: &str, subject: &str) {
    let creds = envio_net::Credentials::new(user, &password(user));
    let mut session = server.imap(&creds).await;
    session
        .append("INBOX", &[], &message(&message_id(), subject, 300))
        .await
        .unwrap();
    session.logout().await.unwrap();
}

/// The draft files in the data folder, as text.
pub fn draft_texts(run: &CoreRun) -> Vec<String> {
    let Ok(dir) = std::fs::read_dir(run.paths().drafts()) else {
        return Vec::new();
    };
    dir.flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .collect()
}

/// Every row of the unified Inbox, page after page.
pub fn unified(run: &CoreRun) -> Vec<MessageRow> {
    let list = ListRef::Inboxes;
    let mut all: Vec<MessageRow> = Vec::new();
    let mut after = None;
    loop {
        run.core().send(Command::ListMessages { list, after });
        let (rows, end) = run.wait(NET, |event| match event {
            Event::Messages {
                list: l,
                after: a,
                rows,
                end,
            } if *l == list && *a == after => Some((rows.clone(), *end)),
            _ => None,
        });
        after = rows.last().map(|row| row.cursor);
        all.extend(rows);
        if end || after.is_none() {
            return all;
        }
    }
}

/// Opens a new compose window on the account and waits until it is ready.
pub fn compose_new(run: &CoreRun, compose: ComposeId, account: &str) {
    run.core().send(Command::StartCompose {
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
