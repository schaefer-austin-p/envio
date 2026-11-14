//! A message above the partial threshold, through envio-core against Stalwart
//! (see core_stalwart.rs): opening it fetches only the text, the attachment
//! is fetched when saved. Skips without Stalwart, unless
//! ENVIO_REQUIRE_STALWART=1.

mod support;

use std::time::Duration;

use envio_core::{Command, Event, FolderRole, ImagesChoice};
use envio_net::imap::Flag;
use envio_store::{BodyState, Db};
use log::LevelFilter;
use support::core_run::{CoreRun, NET, password};
use support::stalwart::{Stalwart, message_id};

/// 150 000 lines of 76 base64 characters: 11.4 MB on the wire, above the
/// threshold, and 8.5 MB decoded.
const LINES: usize = 150_000;
const LINE: &str =
    "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVphYmNkZWZnaGlqa2xtbm9wcXJzdHV2d3h5ejAxMjM0\r\n";

#[tokio::test]
async fn a_big_message_opens_from_its_text_and_saves_its_attachment_on_demand() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let subject = format!("Big {}", message_id().trim_matches(['<', '>']));
    let id = message_id();
    let attachment = LINE.repeat(LINES);
    let eml = format!(
        "From: Alice <alice@envio.test>\r\nTo: Heidi <heidi@envio.test>\r\n\
         Subject: {subject}\r\nMessage-ID: {id}\r\nDate: Sun, 08 Nov 2026 10:00:00 +0000\r\n\
         MIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=\"outer\"\r\n\r\n\
         --outer\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nHello big world\r\n\
         --outer\r\nContent-Type: application/pdf; name=\"big.pdf\"\r\n\
         Content-Disposition: attachment; filename=\"big.pdf\"\r\n\
         Content-Transfer-Encoding: base64\r\n\r\n{attachment}\r\n--outer--\r\n",
    );
    {
        let creds = envio_net::Credentials::new("heidi", &password("heidi"));
        let mut session = server.imap(&creds).await;
        session
            .append("INBOX", &[Flag::Seen], eml.as_bytes())
            .await
            .unwrap();
        session.logout().await.unwrap();
    }

    let run = CoreRun::start(LevelFilter::Info);
    let account = run.add_account(&server, "heidi");
    let (inbox, _) = run.folder(&account, FolderRole::Inbox);
    let row = run.row_with(inbox, &subject);
    run.core().send(Command::OpenMessage {
        id: row.id,
        images: ImagesChoice::Default,
        width_px: 360,
    });
    let view = run.wait(NET, |event| match event {
        Event::MessageOpened { id, view } if *id == row.id => Some(view.clone()),
        _ => None,
    });
    assert!(view.html.contains("Hello big world"));

    let db = Db::open(&run.paths().db()).unwrap();
    let message = db.message(row.id).unwrap();
    assert_eq!(message.body, BodyState::Partial);
    let on_disk = envio_store::Bodies::new(run.paths().bodies())
        .read(message.folder, message.uid)
        .unwrap()
        .len();
    assert!(on_disk < 10_000, "{on_disk} bytes cached");
    drop(db);

    let saved = run.dir.path().join("saved");
    run.core().send(Command::SaveAttachment {
        id: row.id,
        index: 0,
        dir: saved.clone(),
    });
    let path = run.wait(Duration::from_secs(60), |event| match event {
        Event::AttachmentSaved { id, path, .. } if *id == row.id => Some(path.clone()),
        Event::AttachmentFailed { id, .. } if *id == row.id => panic!("save failed"),
        _ => None,
    });
    // The user's mailbox is shared with other tests: leave it as it was.
    let creds = envio_net::Credentials::new("heidi", &password("heidi"));
    let mut session = server.imap(&creds).await;
    session.select("INBOX").await.unwrap();
    let uids = session.search_message_id(&id).await.unwrap();
    session.delete(&uids).await.unwrap();
    session.logout().await.unwrap();

    assert_eq!(path.file_name().unwrap(), "big.pdf");
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(bytes.len(), LINES * 57);
    assert!(bytes.starts_with(b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz01234"));
}
