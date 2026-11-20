//! Drafts and the server's Drafts folder (#52), through `Core::send` and
//! events only, against Stalwart from crates/envio-net/tests/stalwart/up.sh.
//! Skips without it, unless ENVIO_REQUIRE_STALWART=1.

mod support;

use std::time::Duration;

use envio_core::{Command, ComposeId, DraftInput, Event, FolderRole, StartFrom};
use log::LevelFilter;
use support::core_run::{CoreRun, NET};
use support::drafts::{USER, drafts_now, edit_and_save, put_draft, settles_at};
use support::stalwart::{Stalwart, message_id};

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
