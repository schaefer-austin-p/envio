//! Drafts and the server's Drafts folder (#52), through `Core::send` and
//! events only, against Stalwart from crates/envio-net/tests/stalwart/up.sh.
//! Skips without it, unless ENVIO_REQUIRE_STALWART=1.

mod support;

use std::time::{Duration, Instant};

use envio_core::{Command, ComposeId, DraftInput, Event, FolderRole, StartFrom};
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
