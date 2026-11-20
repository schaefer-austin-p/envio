//! The local-only Drafts entry (#303) as the worker lists it.

use std::sync::Mutex;

use envio_store::{Draft, FolderRole, ListRef};

use crate::api::command::SearchScope;

use crate::api::event::Event;
use crate::api::view::{FolderRow, MessageRow};
use crate::core::Config;
use crate::worker::testing::{Seeded, Sink, add_account, open, seed};

struct Rig {
    _dir: tempfile::TempDir,
    config: Config,
    seeded: Seeded,
    sink: Sink,
}

fn rig() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    Rig {
        _dir: dir,
        config,
        seeded,
        sink: Sink::default(),
    }
}

fn draft(account: &str, subject: &str) -> Draft {
    Draft {
        account: account.into(),
        to: vec!["Bob <bob@example.org>".into()],
        subject: subject.into(),
        body: "body".into(),
        updated_at_ms: 5_000,
        ..Draft::default()
    }
}

fn folders(sink: &Sink) -> Vec<FolderRow> {
    sink.events()
        .into_iter()
        .rev()
        .find_map(|e| match e {
            Event::Folders { folders, .. } => Some(folders),
            _ => None,
        })
        .expect("a Folders event")
}

fn messages(sink: &Sink) -> Vec<MessageRow> {
    sink.events()
        .into_iter()
        .rev()
        .find_map(|e| match e {
            Event::Messages { rows, .. } => Some(rows),
            _ => None,
        })
        .expect("a Messages event")
}

fn local_entries(rows: &[FolderRow]) -> Vec<&FolderRow> {
    rows.iter().filter(|f| f.id.0 < 0).collect()
}

#[test]
fn no_drafts_folder_gives_one_local_entry_even_with_no_drafts() {
    let r = rig();
    let w = open(&r.config, &r.sink);
    w.list_folders(&r.seeded.account);
    let rows = folders(&r.sink);
    let local = local_entries(&rows);
    assert_eq!(local.len(), 1);
    assert_eq!(local[0].name, "Drafts");
    assert_eq!(local[0].role, FolderRole::Drafts);
    assert_eq!(local[0].unread, 0);
    // The entry comes after the Inbox, as a Drafts folder would.
    assert_eq!(rows[1].id, local[0].id);
    // A second listing keeps the same id.
    w.list_folders(&r.seeded.account);
    assert_eq!(local_entries(&folders(&r.sink))[0].id, local[0].id);
}

#[test]
fn local_entry_lists_only_its_accounts_local_drafts() {
    let r = rig();
    let mut w = open(&r.config, &r.sink);
    let other = add_account(&mut w, "other", 0);
    w.store
        .drafts
        .create(&draft(&r.seeded.account, "mine"))
        .unwrap();
    w.store.drafts.create(&draft(&other, "theirs")).unwrap();
    w.list_folders(&r.seeded.account);
    let entry = local_entries(&folders(&r.sink))[0].id;
    w.list_messages(ListRef::Folder(entry), None);
    let rows = messages(&r.sink);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].subject, "mine");
    assert_eq!(rows[0].folder, entry);
    assert!(rows[0].local_draft.is_some());
    assert_eq!(rows[0].not_uploaded_text, "Not uploaded");
}

#[test]
fn account_with_a_drafts_folder_has_no_local_entry() {
    let r = rig();
    let w = open(&r.config, &r.sink);
    let real = w
        .store
        .db
        .upsert_folder(&r.seeded.account, "Drafts", FolderRole::Drafts)
        .unwrap();
    w.list_folders(&r.seeded.account);
    let rows = folders(&r.sink);
    assert!(local_entries(&rows).is_empty());
    assert!(rows.iter().any(|f| f.id == real));
}

#[test]
fn entry_goes_away_when_the_server_gets_a_drafts_folder() {
    let r = rig();
    let w = open(&r.config, &r.sink);
    w.store
        .drafts
        .create(&draft(&r.seeded.account, "mine"))
        .unwrap();
    w.list_folders(&r.seeded.account);
    let entry = local_entries(&folders(&r.sink))[0].id;
    let real = w
        .store
        .db
        .upsert_folder(&r.seeded.account, "Drafts", FolderRole::Drafts)
        .unwrap();
    w.list_folders(&r.seeded.account);
    assert!(local_entries(&folders(&r.sink)).is_empty());
    // The old entry lists nothing; the real folder shows the draft.
    w.list_messages(ListRef::Folder(entry), None);
    assert!(messages(&r.sink).is_empty());
    w.list_messages(ListRef::Folder(real), None);
    let rows = messages(&r.sink);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].folder, real);
}

#[test]
fn unified_inbox_does_not_list_local_drafts() {
    let r = rig();
    let w = open(&r.config, &r.sink);
    w.store
        .drafts
        .create(&draft(&r.seeded.account, "mine"))
        .unwrap();
    w.list_messages(ListRef::Inboxes, None);
    assert!(messages(&r.sink).iter().all(|m| m.local_draft.is_none()));
}

#[test]
fn load_older_on_the_local_entry_is_done_at_once_with_no_failure() {
    let r = rig();
    let mut w = open(&r.config, &r.sink);
    w.list_folders(&r.seeded.account);
    let entry = local_entries(&folders(&r.sink))[0].id;
    w.load_older(ListRef::Folder(entry), None);
    let events = r.sink.events();
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::OlderLoaded { list, fetched: 0, exhausted: true }
                if *list == ListRef::Folder(entry)
        )),
        "{events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::OlderFailed { .. }))
    );
    assert!(
        w.live.values().all(|l| l.wants.older.is_empty()),
        "no server fetch is wanted"
    );
}

/// Warnings the test logger saw.
static WARNINGS: Mutex<Vec<String>> = Mutex::new(Vec::new());

struct Capture;

impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        if record.level() <= log::Level::Warn {
            WARNINGS.lock().unwrap().push(record.args().to_string());
        }
    }
    fn flush(&self) {}
}

#[test]
fn a_server_search_of_the_local_entry_asks_no_server_and_finds_no_missing_folder() {
    // Only this test installs a logger in the unit-test binary.
    log::set_logger(&Capture).expect("no other test sets a logger");
    log::set_max_level(log::LevelFilter::Warn);
    let r = rig();
    let mut w = open(&r.config, &r.sink);
    w.list_folders(&r.seeded.account);
    let entry = local_entries(&folders(&r.sink))[0].id;
    w.search_server(SearchScope::Folder(entry), "q".into());
    assert!(
        r.sink.events().iter().any(|e| matches!(
            e,
            Event::SearchResults { server: true, rows, .. } if rows.is_empty()
        )),
        "answered at once with nothing"
    );
    assert!(w.live.values().all(|l| l.wants.searches.is_empty()));
    let seen = WARNINGS.lock().unwrap().clone();
    assert!(
        !seen.iter().any(|m| m.contains("finding the folder")),
        "the entry is known, not a missing folder: {seen:?}"
    );
}
