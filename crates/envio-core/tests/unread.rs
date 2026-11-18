//! The account's Inbox unread count follows the cache after `Started`.

mod support;

use envio_core::{
    Command, Event, FolderId, FolderRole, ImagesChoice, ListRef, MessageRowId, UserAction,
};
use support::Harness;

const ME: &str = "me@example.org";
const MAIL: &[u8] = b"From: Ada <ada@example.org>\r\nSubject: hi\r\n\r\nHello\r\n";

struct Seeded {
    h: Harness,
    key: String,
    inbox: FolderId,
}

fn seeded() -> Seeded {
    let mut h = Harness::unstarted();
    let key = h.seed_account(ME, Some("pw"));
    let inbox = h.seed_folder_with(
        &key,
        "INBOX",
        FolderRole::Inbox,
        &[(1, MAIL), (2, MAIL), (3, MAIL)],
    );
    h.start_core();
    Seeded { h, key, inbox }
}

impl Seeded {
    fn ids(&self) -> Vec<MessageRowId> {
        self.h.send(Command::ListMessages {
            list: ListRef::Folder(self.inbox),
            after: None,
        });
        self.h.wait(|e| match e {
            Event::Messages { rows, .. } => Some(rows.iter().map(|r| r.id).collect()),
            _ => None,
        })
    }

    /// The next count the app is told for the account.
    fn told(&self) -> u32 {
        self.h.wait(|e| match e {
            Event::AccountUnread { account, unread } if *account == self.key => Some(*unread),
            _ => None,
        })
    }
}

#[test]
fn mark_read_and_unread_tell_the_app_the_new_count() {
    let s = seeded();
    let started = s.h.wait(|e| match e {
        Event::Started { accounts, .. } => Some(accounts[0].unread),
        _ => None,
    });
    assert_eq!(started, 3);
    let ids = s.ids();
    s.h.send(Command::Act {
        ids: vec![ids[0], ids[1]],
        action: UserAction::MarkRead(true),
    });
    assert_eq!(s.told(), 1);
    s.h.send(Command::Act {
        ids: vec![ids[0]],
        action: UserAction::MarkRead(false),
    });
    assert_eq!(s.told(), 2);
}

#[test]
fn opening_a_message_tells_the_app_the_new_count() {
    let s = seeded();
    let ids = s.ids();
    s.h.send(Command::OpenMessage {
        id: ids[0],
        images: ImagesChoice::Default,
        width_px: 300,
    });
    assert_eq!(s.told(), 2);
}

struct Badges(std::sync::mpsc::Sender<u32>);

impl envio_core::Poster for Badges {
    fn post(&self, _: &envio_core::Alert) {}
    fn badge(&self, total_unread: u32) {
        let _ = self.0.send(total_unread);
    }
}

#[test]
fn a_poster_set_after_the_start_is_told_the_total_then_each_change() {
    let s = seeded();
    let (tx, rx) = std::sync::mpsc::channel();
    s.h.core.as_ref().unwrap().set_poster(Box::new(Badges(tx)));
    let wait = std::time::Duration::from_secs(5);
    assert_eq!(rx.recv_timeout(wait).unwrap(), 3);
    let ids = s.ids();
    s.h.send(Command::Act {
        ids: vec![ids[0]],
        action: UserAction::MarkRead(true),
    });
    assert_eq!(rx.recv_timeout(wait).unwrap(), 2);
}
