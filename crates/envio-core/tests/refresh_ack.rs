//! The ack of a refresh: `Refreshed` carries the ticket and comes after the
//! full pass ran, with nothing in the test setting what the worker wants.

mod support;

use envio_core::{Command, Event, FolderRole};
use support::Harness;
use support::fake::{Kind, serve};

const EML: &[u8] = b"From: Ada <ada@example.org>\r\nSubject: s\r\n\r\nbody\r\n";

#[test]
fn a_refresh_is_acked_with_its_ticket_after_a_full_pass() {
    let mut h = Harness::unstarted();
    h.foreground = true;
    let server = serve(Kind::Older);
    let key = h.seed_account_at("me@example.org", Some("pw"), server.port);
    h.pin_imap(&key, &server.hex);
    h.seed_folder_with(&key, "INBOX", FolderRole::Inbox, &[(10, EML)]);
    h.start_core();
    h.send(Command::Refresh {
        account: Some(key),
        ticket: 41,
    });
    let ticket = h.wait(|e| match e {
        Event::Refreshed { ticket } => Some(*ticket),
        _ => None,
    });
    assert_eq!(ticket, 41);
    let searched = server
        .log
        .lock()
        .unwrap()
        .iter()
        .filter(|l| l.contains("UID SEARCH"))
        .count();
    assert!(searched >= 1, "the pass listed the Inbox before the ack");
}
