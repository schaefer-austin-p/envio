use envio_store::{Draft, FolderId, FolderRole, MessageRowId};
use envio_sync::UNDO_MS;

use super::*;
use crate::api::command::{ComposeId, UndoId, UserAction};
use crate::api::event::Event;
use crate::clock::now_ms;
use crate::core::Config;
use crate::worker::testing::{Seeded, Sink, open, seed};

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

fn draft(account: &str) -> Draft {
    Draft {
        account: account.into(),
        to: vec!["Bob <bob@example.org>".into()],
        subject: "unsent".into(),
        body: "body".into(),
        updated_at_ms: 5_000,
        ..Draft::default()
    }
}

struct Made {
    folder: FolderId,
    id: String,
    row: MessageRowId,
    files: std::path::PathBuf,
}

/// A Drafts folder in the cache, a draft file with an attachment copy, and
/// the draft's pseudo-row id as the list would have given it.
fn with_draft(w: &mut Worker<'_>, r: &Rig) -> Made {
    let folder = w
        .store
        .db
        .upsert_folder(&r.seeded.account, "Drafts", FolderRole::Drafts)
        .unwrap();
    let id = w.store.drafts.create(&draft(&r.seeded.account)).unwrap();
    let kept = w.store.drafts.keep_bytes(&id, "a.txt", b"x").unwrap();
    let shown = w.with_local_drafts(folder, Vec::new(), (None, true));
    assert_eq!(shown.len(), 1);
    Made {
        folder,
        id,
        row: shown[0].id,
        files: kept.parent().unwrap().to_owned(),
    }
}

fn shown(w: &Worker<'_>, folder: FolderId) -> usize {
    w.with_local_drafts(folder, Vec::new(), (None, true)).len()
}

fn offers(sink: &Sink) -> Vec<UndoId> {
    sink.events()
        .iter()
        .filter_map(|e| match e {
            Event::UndoOffer { undo, .. } => Some(*undo),
            _ => None,
        })
        .collect()
}

#[test]
fn deleting_a_pseudo_row_hides_it_and_keeps_its_file() {
    let r = rig();
    let mut w = open(&r.config, &r.sink);
    let m = with_draft(&mut w, &r);
    w.act(&[m.row], UserAction::Delete);
    assert_eq!(shown(&w, m.folder), 0, "hidden at once");
    assert!(w.store.drafts.load(&m.id).is_ok(), "the file stays");
    assert_eq!(offers(&r.sink).len(), 1);
    assert!(
        r.sink
            .events()
            .iter()
            .any(|e| matches!(e, Event::LocalDraftsChanged { .. }))
    );
    // A crash here: a new worker finds the draft and its row again.
    drop(w);
    let again = Sink::default();
    let w = open(&r.config, &again);
    assert_eq!(shown(&w, m.folder), 1);
}

#[test]
fn expiry_deletes_the_file_and_its_folder() {
    let r = rig();
    let mut w = open(&r.config, &r.sink);
    let m = with_draft(&mut w, &r);
    w.act(&[m.row], UserAction::Delete);
    assert!(w.expire_undos(now_ms()).is_empty(), "inside the window");
    assert!(w.store.drafts.load(&m.id).is_ok());
    assert!(m.files.exists());
    let closed = w.expire_undos(now_ms() + UNDO_MS + 1_000);
    assert_eq!(closed.len(), 1);
    assert!(w.store.drafts.load(&m.id).is_err(), "file gone");
    assert!(!m.files.exists(), "attachment copies gone");
    assert_eq!(shown(&w, m.folder), 0);
}

#[test]
fn undo_brings_the_row_back() {
    let r = rig();
    let mut w = open(&r.config, &r.sink);
    let m = with_draft(&mut w, &r);
    w.act(&[m.row], UserAction::Delete);
    w.undo_action(offers(&r.sink)[0], now_ms());
    assert_eq!(shown(&w, m.folder), 1);
    assert!(w.store.drafts.load(&m.id).is_ok());
    assert!(
        r.sink
            .events()
            .iter()
            .any(|e| matches!(e, Event::UndoClosed { undone: true, .. }))
    );
    // Expiry after the Undo deletes nothing.
    let _ = w.expire_undos(now_ms() + UNDO_MS + 1_000);
    assert!(w.store.drafts.load(&m.id).is_ok());
}

#[test]
fn a_mixed_act_gives_one_offer_and_undo_restores_both() {
    let r = rig();
    let mut w = open(&r.config, &r.sink);
    let m = with_draft(&mut w, &r);
    w.act(&[m.row, r.seeded.message], UserAction::Delete);
    assert_eq!(offers(&r.sink).len(), 1);
    let inbox = r.seeded.folder;
    assert!(
        w.store.db.message_at(inbox, 1).unwrap().is_none(),
        "server row gone"
    );
    assert_eq!(shown(&w, m.folder), 0);
    w.undo_action(offers(&r.sink)[0], now_ms());
    assert!(
        w.store.db.message_at(inbox, 1).unwrap().is_some(),
        "server row back"
    );
    assert_eq!(shown(&w, m.folder), 1);
    assert!(w.store.drafts.load(&m.id).is_ok());
}

#[test]
fn a_mixed_act_expires_both_halves() {
    let r = rig();
    let mut w = open(&r.config, &r.sink);
    let m = with_draft(&mut w, &r);
    w.act(&[m.row, r.seeded.message], UserAction::Delete);
    let closed = w.expire_undos(now_ms() + UNDO_MS + 1_000);
    assert_eq!(closed.len(), 1);
    assert!(w.store.drafts.load(&m.id).is_err());
    assert!(
        w.live[&r.seeded.account].wants.replay,
        "the server half replays"
    );
}

#[test]
fn other_actions_on_a_pseudo_row_change_nothing() {
    let r = rig();
    let mut w = open(&r.config, &r.sink);
    let m = with_draft(&mut w, &r);
    for action in [
        UserAction::MarkRead(true),
        UserAction::MarkRead(false),
        UserAction::Flag(true),
        UserAction::Archive,
    ] {
        w.act(&[m.row], action);
    }
    assert_eq!(shown(&w, m.folder), 1);
    assert!(w.store.drafts.load(&m.id).is_ok());
    assert!(offers(&r.sink).is_empty());
    assert_eq!(w.store.journal.pending().count(), 0);
    let noticed = r
        .sink
        .events()
        .iter()
        .any(|e| matches!(e, Event::Notice { .. }));
    assert!(!noticed, "skipped quietly");
}

#[test]
fn a_draft_an_open_compose_holds_is_skipped_at_expiry() {
    let r = rig();
    let mut w = open(&r.config, &r.sink);
    let m = with_draft(&mut w, &r);
    w.act(&[m.row], UserAction::Delete);
    // The compose opens during the window.
    w.book.adopt(ComposeId(1), m.id.clone());
    let _ = w.expire_undos(now_ms() + UNDO_MS + 1_000);
    assert!(
        w.store.drafts.load(&m.id).is_ok(),
        "not deleted under its compose"
    );
    assert_eq!(shown(&w, m.folder), 1, "the row is back");
}

#[test]
fn a_draft_already_open_is_not_held_or_offered() {
    let r = rig();
    let mut w = open(&r.config, &r.sink);
    let m = with_draft(&mut w, &r);
    w.book.adopt(ComposeId(1), m.id.clone());
    w.act(&[m.row], UserAction::Delete);
    assert!(offers(&r.sink).is_empty());
    assert_eq!(shown(&w, m.folder), 1);
}

#[test]
fn an_unknown_negative_id_is_ignored() {
    let r = rig();
    let mut w = open(&r.config, &r.sink);
    w.act(&[MessageRowId(-77)], UserAction::Delete);
    assert!(offers(&r.sink).is_empty());
}
