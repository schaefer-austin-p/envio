//! The `Message` fake answers a summary FETCH the way a real server does.

mod support;

use envio_core::{Command, Event, FolderRole, ListRef};
use support::Harness;
use support::fake::{Kind, serve};

#[test]
fn a_summary_fetch_gets_a_date_and_a_header() {
    let server = serve(Kind::Message);
    let mut h = Harness::unstarted();
    h.foreground = true;
    let key = h.seed_account_at("me@example.org", Some("pw"), server.port);
    h.pin_imap(&key, &server.hex);
    // An empty cache: the startup sync fetches the summary of UID 1.
    let inbox = h.seed_folder_with(&key, "INBOX", FolderRole::Inbox, &[]);
    h.start_core();
    h.wait(|e| match e {
        Event::MessagesChanged { folders } if folders.contains(&inbox) => Some(()),
        _ => None,
    });
    let list = ListRef::Folder(inbox);
    h.send(Command::ListMessages { list, after: None });
    let rows = h.wait(|e| match e {
        Event::Messages { list: l, rows, .. } if *l == list => Some(rows.clone()),
        _ => None,
    });
    assert_eq!(rows.len(), 1);
    // The header carries the subject; the date can only come from INTERNALDATE.
    assert_eq!(rows[0].subject, "file");
    assert_eq!(rows[0].date, Some(1_577_836_800));
}
