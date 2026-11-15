//! Answering a meeting invite against Stalwart (crates/envio-net/tests/
//! stalwart/up.sh): the organizer receives an iMIP reply. Skips without it,
//! unless ENVIO_REQUIRE_STALWART=1, which makes a missing server fail.

mod support;

use std::time::{Duration, Instant};

use envio_core::{Answer, CardState, Command, Event, FolderRole, ImagesChoice, UndoScope};
use log::LevelFilter;
use support::core_run::{CoreRun, NET, password};
use support::stalwart::{Stalwart, message_id};

/// The hold is the Undo window (core `UNDO_MS`), then the send.
const HOLD: Duration = Duration::from_secs(8);

fn creds(user: &str) -> envio_net::Credentials {
    envio_net::Credentials::new(user, &password(user))
}

/// An iMIP request from grace to heidi.
fn request(id: &str, summary: &str) -> Vec<u8> {
    format!(
        "From: Grace <grace@envio.test>\r\nTo: heidi@envio.test\r\n\
Subject: {summary}\r\nMessage-ID: {id}\r\nDate: Sun, 08 Nov 2026 10:00:00 +0000\r\n\
MIME-Version: 1.0\r\nContent-Type: multipart/alternative; boundary=\"b\"\r\n\r\n\
--b\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nYou are invited.\r\n\
--b\r\nContent-Type: text/calendar; method=REQUEST; charset=utf-8\r\n\r\n\
BEGIN:VCALENDAR\r\nVERSION:2.0\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\n\
UID:{id}\r\nSEQUENCE:0\r\nDTSTAMP:20261108T100000Z\r\n\
DTSTART:20270115T080000Z\r\nDTEND:20270115T090000Z\r\nSUMMARY:{summary}\r\n\
ORGANIZER;CN=Grace:mailto:grace@envio.test\r\n\
ATTENDEE;CN=Heidi;PARTSTAT=NEEDS-ACTION:mailto:heidi@envio.test\r\n\
END:VEVENT\r\nEND:VCALENDAR\r\n--b--\r\n"
    )
    .into_bytes()
}

/// The mail that reached grace's INBOX with `subject`, once it has.
async fn reply_to_grace(server: &Stalwart, subject: &str) -> Vec<u8> {
    let end = Instant::now() + HOLD + NET;
    loop {
        let mut session = server.imap(&creds("grace")).await;
        session.select("INBOX").await.unwrap();
        let uids = session.search_text(subject).await.unwrap();
        let mut found = None;
        if let Some(&uid) = uids.first() {
            found = session.fetch_body(uid, 4096).await.unwrap();
        }
        session.logout().await.unwrap();
        if let Some(body) = found {
            return body;
        }
        assert!(
            Instant::now() < end,
            "no reply for {subject:?} reached grace"
        );
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

#[tokio::test]
async fn invite_reply_reaches_the_organizer() {
    // `find` panics, not skips, when ENVIO_REQUIRE_STALWART=1 and no server.
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let summary = format!("Sync {}", message_id().trim_matches(['<', '>']));
    {
        let mut session = server.imap(&creds("heidi")).await;
        session
            .append("INBOX", &[], &request(&message_id(), &summary))
            .await
            .unwrap();
        session.logout().await.unwrap();
    }
    let run = CoreRun::start(LevelFilter::Off);
    let account = run.add_account(&server, "heidi");
    let (inbox, _) = run.folder(&account, FolderRole::Inbox);
    let row = run.row_with(inbox, &summary);

    run.core().send(Command::OpenMessage {
        id: row.id,
        images: ImagesChoice::Default,
        width_px: 360,
    });
    let card = run.wait(NET, |e| match e {
        Event::MessageOpened { id, view } if *id == row.id => Some(view.invite.clone()),
        _ => None,
    });
    assert_eq!(card.expect("a card").state, CardState::Open);

    run.core().send(Command::AnswerInvite {
        id: row.id,
        answer: Answer::Accepted,
        note: String::new(),
    });
    let scope = run.wait(NET, |e| match e {
        Event::UndoOffer { scope, .. } => Some(*scope),
        _ => None,
    });
    assert_eq!(scope, UndoScope::Reader);

    let mail = reply_to_grace(&server, &format!("Accepted: {summary}")).await;
    let (reply, _) = envio_mime::invite_of(&mail).expect("a calendar part");
    assert_eq!(reply.method, envio_mime::InviteMethod::Reply);
    assert!(reply.raw.contains("METHOD:REPLY"), "{}", reply.raw);
    assert!(reply.raw.contains("PARTSTAT=ACCEPTED"), "{}", reply.raw);
}
