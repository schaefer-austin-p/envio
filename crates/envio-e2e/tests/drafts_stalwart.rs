//! Drafts and the server's Drafts folder (#52), through `Core::send` and
//! events only, against Stalwart from crates/envio-net/tests/stalwart/up.sh.
//! Skips without it, unless ENVIO_REQUIRE_STALWART=1.

mod support;

use std::time::{Duration, Instant};

use envio_core::{Command, ComposeId, DraftInput, Event, FolderRole, StartFrom};
use envio_net::imap::Flag;
use log::LevelFilter;
use support::core_run::{CoreRun, NET, password};
use support::stalwart::{Stalwart, message_id};

const USER: &str = "grace";

/// Live messages in the user's Drafts folder whose subject has `needle`.
async fn on_server(server: &Stalwart, drafts: &str, needle: &str) -> usize {
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
async fn settles_at(server: &Stalwart, drafts: &str, needle: &str, want: usize) {
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

fn open_new(run: &CoreRun, compose: ComposeId, account: &str) {
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

#[tokio::test]
async fn a_left_draft_is_uploaded_once_replaced_by_a_later_save_and_gone_when_sent() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let run = CoreRun::start(LevelFilter::Info);
    let account = run.add_account(&server, USER);
    let (_, drafts) = run.folder(&account, FolderRole::Drafts);
    let tag = message_id();
    let tag = tag.trim_matches(['<', '>']).to_owned();
    let first = format!("Draft one {tag}");
    let second = format!("Draft two {tag}");
    let input = |subject: &str, id: Option<String>| DraftInput {
        account: account.clone(),
        draft_id: id,
        to: vec!["grace@envio.test".to_owned()],
        subject: subject.to_owned(),
        body: "Not finished.".to_owned(),
        ..DraftInput::default()
    };

    // Leaving compose uploads the saved draft.
    let compose = ComposeId(1);
    open_new(&run, compose, &account);
    run.core().send(Command::DraftChanged {
        compose,
        draft: input(&first, None),
    });
    let id = run.wait(NET, |event| match event {
        Event::DraftSaved { draft_id, .. } => Some(draft_id.clone()),
        _ => None,
    });
    run.core().send(Command::CloseCompose {
        compose,
        discard: false,
    });
    settles_at(&server, &drafts, &tag, 1).await;
    settles_at(&server, &drafts, &first, 1).await;

    // Another save replaces the copy: still one, with the new subject.
    let compose = ComposeId(2);
    run.core().send(Command::StartCompose {
        compose,
        from: StartFrom::Draft,
        draft_id: Some(id.clone()),
        mailto: None,
    });
    run.wait(NET, |event| {
        matches!(event, Event::ComposeReady { .. }).then_some(())
    });
    run.core().send(Command::DraftChanged {
        compose,
        draft: input(&second, Some(id.clone())),
    });
    run.core().send(Command::CloseCompose {
        compose,
        discard: false,
    });
    settles_at(&server, &drafts, &second, 1).await;
    settles_at(&server, &drafts, &first, 0).await;
    settles_at(&server, &drafts, &tag, 1).await;

    // Sending it takes the Drafts copy away.
    let compose = ComposeId(3);
    run.core().send(Command::StartCompose {
        compose,
        from: StartFrom::Draft,
        draft_id: Some(id.clone()),
        mailto: None,
    });
    run.wait(NET, |event| {
        matches!(event, Event::ComposeReady { .. }).then_some(())
    });
    run.core().send(Command::Send {
        compose,
        draft: input(&second, Some(id)),
    });
    run.wait(NET, |event| {
        matches!(event, Event::Queued { .. }).then_some(())
    });
    settles_at(&server, &drafts, &tag, 0).await;
}

/// A draft as another client leaves it in Drafts: `extra` headers, From as
/// Envio would write it (the account's name and address).
async fn put_draft(server: &Stalwart, drafts: &str, (subject, extra): (&str, &str), body: &str) {
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
async fn drafts_now(server: &Stalwart, drafts: &str, tag: &str) -> Vec<(String, String)> {
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
fn edit_and_save(
    run: &CoreRun,
    (account, folder): (&str, envio_core::FolderId),
    subject: &str,
    edited: &str,
) {
    run.core().send(Command::Refresh {
        account: Some(account.to_owned()),
    });
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

#[tokio::test]
async fn editing_a_plain_server_draft_replaces_it_and_a_lossy_one_is_copied() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let run = CoreRun::start(LevelFilter::Info);
    let account = run.add_account(&server, USER);
    let (folder, drafts) = run.folder(&account, FolderRole::Drafts);
    let tag = message_id();
    let tag = tag.trim_matches(['<', '>']).to_owned();

    // Plain: Envio rebuilds all of it, so its upload replaces the original.
    let plain = format!("Plain original {tag}");
    let plain_edited = format!("Plain edited {tag}");
    put_draft(&server, &drafts, (&plain, ""), "Original text.").await;
    edit_and_save(&run, (&account, folder), &plain, &plain_edited);
    settles_at(&server, &drafts, &plain_edited, 1).await;
    settles_at(&server, &drafts, &plain, 0).await;
    let now = drafts_now(&server, &drafts, &plain_edited).await;
    assert_eq!(now.len(), 1, "exactly one copy: {now:?}");
    assert_eq!(now[0].0, plain_edited);
    assert!(now[0].1.contains("Changed text."), "{now:?}");
    println!("read back (plain): {now:?}");

    // Lossy: a header Envio does not write would be lost, so the edit is a
    // copy and the original stays.
    let lossy = format!("Lossy original {tag}");
    let lossy_edited = format!("Lossy edited {tag}");
    put_draft(
        &server,
        &drafts,
        (&lossy, "Reply-To: other@envio.test\r\nX-Priority: 1\r\n"),
        "Original text.",
    )
    .await;
    edit_and_save(&run, (&account, folder), &lossy, &lossy_edited);
    settles_at(&server, &drafts, &lossy_edited, 1).await;
    // Give a wrongly queued replacement time to show.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let originals = drafts_now(&server, &drafts, &lossy).await;
    assert_eq!(originals.len(), 1, "the original stays: {originals:?}");
    assert!(originals[0].1.contains("Original text."), "{originals:?}");
    let copies = drafts_now(&server, &drafts, &lossy_edited).await;
    assert_eq!(copies.len(), 1, "{copies:?}");
    assert!(copies[0].1.contains("Changed text."), "{copies:?}");
    println!("read back (lossy): original {originals:?}, copy {copies:?}");
}
