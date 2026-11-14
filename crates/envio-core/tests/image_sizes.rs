//! The reader reports the natural sizes of the remote images it loaded; the
//! open message is shown again with any too-wide one capped (#170).

mod support;

use std::collections::HashMap;

use envio_core::api::view::MessageView;
use envio_core::{Command, Event, FolderRole, ImagesChoice, ListRef, MessageRowId};
use support::Harness;

const ME: &str = "me@example.org";
const URL: &str = "http://images.example.net/banner.png";

const BANNER: &[u8] = b"From: Grace <grace@envio.test>\r\nSubject: news\r\nMIME-Version: 1.0\r\n\
Content-Type: text/html; charset=utf-8\r\n\r\n\
<p>News</p><img src=\"http://images.example.net/banner.png\" alt=\"banner\">";

struct Open {
    h: Harness,
    id: MessageRowId,
}

/// The banner message, cached, its server size `size` when given.
fn seeded(size: Option<u32>) -> Open {
    let mut h = Harness::unstarted();
    let key = h.seed_account(ME, Some("pw"));
    let folder = h.seed_folder_with(&key, "INBOX", FolderRole::Inbox, &[(1, BANNER)]);
    if let Some(size) = size {
        let conn = rusqlite::Connection::open(h.paths().db()).unwrap();
        conn.execute("UPDATE messages SET size = ?1", [size])
            .unwrap();
    }
    h.start_core();
    h.send(Command::ListMessages {
        list: ListRef::Folder(folder),
        after: None,
    });
    let id = h.wait(|e| match e {
        Event::Messages { rows, .. } => Some(rows[0].id),
        _ => None,
    });
    Open { h, id }
}

impl Open {
    fn opened(&self) -> MessageView {
        let id = self.id;
        self.h.wait(|e| match e {
            Event::MessageOpened { id: i, view } if *i == id => Some(view.clone()),
            _ => None,
        })
    }

    fn open(&self, images: ImagesChoice) -> MessageView {
        self.h.send(Command::OpenMessage {
            id: self.id,
            images,
            width_px: 300,
        });
        self.opened()
    }

    fn report(&self, id: MessageRowId, url: &str, size: (u32, u32)) {
        self.h.send(Command::ImageSizes {
            id,
            sizes: HashMap::from([(url.to_owned(), size)]),
        });
    }

    /// Whether a `MessageOpened` came before the answer to a later command.
    fn shown_again(&self) -> bool {
        self.h.send(Command::ListFolders {
            account: String::new(),
        });
        self.h.wait(|e| match e {
            Event::MessageOpened { .. } => Some(true),
            Event::Folders { .. } | Event::Notice { .. } => Some(false),
            _ => None,
        })
    }
}

#[test]
fn a_too_wide_remote_image_is_capped_once_its_size_is_known() {
    let s = seeded(None);
    let first = s.open(ImagesChoice::Load);
    assert!(
        first
            .html
            .contains(&format!(r#"<img src="{URL}" alt="banner">"#)),
        "{}",
        first.html
    );
    s.report(s.id, URL, (1200, 400));
    let again = s.opened();
    assert!(
        again
            .html
            .contains(&format!(r#"<img src="{URL}" alt="banner" width="300">"#)),
        "{}",
        again.html
    );
    assert_eq!(again.remote_images, [URL]);
    // The same report again changes nothing: no second view.
    s.report(s.id, URL, (1200, 400));
    assert!(!s.shown_again());
}

#[test]
fn a_small_image_is_not_shown_again() {
    let s = seeded(None);
    s.open(ImagesChoice::Load);
    s.report(s.id, URL, (120, 40));
    assert!(!s.shown_again());
}

#[test]
fn a_report_for_another_message_is_ignored() {
    let s = seeded(None);
    s.open(ImagesChoice::Load);
    s.report(MessageRowId(s.id.0 + 1), URL, (1200, 400));
    assert!(!s.shown_again());
}

#[test]
fn a_blocked_image_is_not_shown_again() {
    let s = seeded(None);
    let first = s.open(ImagesChoice::Default);
    assert_eq!(first.blocked_images, 1);
    s.report(s.id, URL, (1200, 400));
    assert!(!s.shown_again());
}

#[test]
fn sizes_carry_over_to_opening_the_same_message_again() {
    let s = seeded(None);
    s.open(ImagesChoice::Load);
    s.report(s.id, URL, (1200, 400));
    s.opened();
    let reopened = s.open(ImagesChoice::Load);
    assert!(
        reopened.html.contains(r#"width="300""#),
        "{}",
        reopened.html
    );
}

#[test]
fn a_big_message_is_capped_too() {
    let s = seeded(Some(30 * 1000 * 1000));
    s.h.send(Command::OpenMessage {
        id: s.id,
        images: ImagesChoice::Load,
        width_px: 300,
    });
    s.opened();
    s.report(s.id, URL, (1200, 400));
    let again = s.opened();
    assert!(again.html.contains(r#"width="300""#), "{}", again.html);
}
