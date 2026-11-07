use envio_store::{Db, FolderRole, StoreError, SyncState};

#[test]
fn upsert_returns_the_same_id_and_updates_the_role() {
    let db = Db::open_in_memory().unwrap();
    let first = db.upsert_folder("a1", "INBOX", FolderRole::Other).unwrap();
    let again = db.upsert_folder("a1", "INBOX", FolderRole::Inbox).unwrap();
    assert_eq!(first, again);
    let folder = db.folder(first).unwrap();
    assert_eq!(folder.role, FolderRole::Inbox);
    assert_eq!(folder.account, "a1");
    assert_eq!(folder.path, "INBOX");
    assert_eq!(folder.sync, None);
}

#[test]
fn folders_lists_one_account_by_path() {
    let db = Db::open_in_memory().unwrap();
    db.upsert_folder("a1", "Sent", FolderRole::Sent).unwrap();
    db.upsert_folder("a1", "INBOX", FolderRole::Inbox).unwrap();
    db.upsert_folder("a2", "INBOX", FolderRole::Inbox).unwrap();
    let paths: Vec<String> = db
        .folders("a1")
        .unwrap()
        .into_iter()
        .map(|f| f.path)
        .collect();
    assert_eq!(paths, ["INBOX", "Sent"]);
}

#[test]
fn every_role_round_trips() {
    let db = Db::open_in_memory().unwrap();
    let roles = [
        FolderRole::Inbox,
        FolderRole::Sent,
        FolderRole::Drafts,
        FolderRole::Trash,
        FolderRole::Archive,
        FolderRole::Junk,
        FolderRole::Other,
    ];
    for (index, role) in roles.into_iter().enumerate() {
        let id = db.upsert_folder("a1", &format!("f{index}"), role).unwrap();
        assert_eq!(db.folder(id).unwrap().role, role);
    }
}

#[test]
fn sync_state_round_trips() {
    let db = Db::open_in_memory().unwrap();
    let id = db.upsert_folder("a1", "INBOX", FolderRole::Inbox).unwrap();
    let state = SyncState {
        uidvalidity: u32::MAX,
        uidnext: 42,
        highestmodseq: Some(i64::MAX as u64),
    };
    db.set_sync_state(id, &state).unwrap();
    assert_eq!(db.folder(id).unwrap().sync, Some(state));
    let plain = SyncState {
        uidvalidity: 7,
        uidnext: 1,
        highestmodseq: None,
    };
    db.set_sync_state(id, &plain).unwrap();
    assert_eq!(db.folder(id).unwrap().sync, Some(plain));
}

#[test]
fn a_modseq_over_63_bits_is_refused() {
    let db = Db::open_in_memory().unwrap();
    let id = db.upsert_folder("a1", "INBOX", FolderRole::Inbox).unwrap();
    let state = SyncState {
        uidvalidity: 1,
        uidnext: 1,
        highestmodseq: Some(u64::MAX),
    };
    let error = db.set_sync_state(id, &state).unwrap_err();
    assert!(matches!(error, StoreError::BadRecord { .. }), "{error:?}");
    assert_eq!(db.folder(id).unwrap().sync, None);
}

#[test]
fn unknown_folders_are_not_found() {
    let db = Db::open_in_memory().unwrap();
    let id = db.upsert_folder("a1", "INBOX", FolderRole::Inbox).unwrap();
    db.remove_folder(id).unwrap();
    assert!(matches!(db.folder(id), Err(StoreError::NotFound)));
    let state = SyncState {
        uidvalidity: 1,
        uidnext: 1,
        highestmodseq: None,
    };
    assert!(matches!(
        db.set_sync_state(id, &state),
        Err(StoreError::NotFound)
    ));
    assert!(matches!(db.reset_folder(id), Err(StoreError::NotFound)));
    assert!(matches!(db.remove_folder(id), Err(StoreError::NotFound)));
}

#[test]
fn reset_clears_the_sync_state() {
    let db = Db::open_in_memory().unwrap();
    let id = db.upsert_folder("a1", "INBOX", FolderRole::Inbox).unwrap();
    let state = SyncState {
        uidvalidity: 1,
        uidnext: 5,
        highestmodseq: Some(9),
    };
    db.set_sync_state(id, &state).unwrap();
    db.reset_folder(id).unwrap();
    assert_eq!(db.folder(id).unwrap().sync, None);
}

#[test]
fn removing_an_account_cache_drops_only_its_folders() {
    let db = Db::open_in_memory().unwrap();
    db.upsert_folder("a1", "INBOX", FolderRole::Inbox).unwrap();
    db.upsert_folder("a2", "INBOX", FolderRole::Inbox).unwrap();
    db.remove_account_cache("a1").unwrap();
    assert!(db.folders("a1").unwrap().is_empty());
    assert_eq!(db.folders("a2").unwrap().len(), 1);
}

#[test]
fn removed_account_folders_are_returned_and_ids_are_never_reused() {
    let db = Db::open_in_memory().unwrap();
    let inbox = db.upsert_folder("a1", "INBOX", FolderRole::Inbox).unwrap();
    let sent = db.upsert_folder("a1", "Sent", FolderRole::Sent).unwrap();
    let mut removed = db.remove_account_cache("a1").unwrap();
    removed.sort_by_key(|id| id.0);
    assert_eq!(removed, [inbox, sent]);
    let fresh = db.upsert_folder("a2", "INBOX", FolderRole::Inbox).unwrap();
    assert!(fresh.0 > sent.0);
    db.remove_folder(fresh).unwrap();
    let again = db.upsert_folder("a2", "INBOX", FolderRole::Inbox).unwrap();
    assert_ne!(again, fresh);
}

#[test]
fn a_failed_step_rolls_back_folder_changes() {
    let db = Db::open_in_memory().unwrap();
    let error = db
        .step(|db| {
            db.upsert_folder("a1", "INBOX", FolderRole::Inbox)?;
            Err::<(), _>(StoreError::NotFound)
        })
        .unwrap_err();
    assert!(matches!(error, StoreError::NotFound));
    assert!(db.folders("a1").unwrap().is_empty());
}

#[test]
fn reset_inside_a_step_rolls_back_with_it() {
    let db = Db::open_in_memory().unwrap();
    let id = db.upsert_folder("a1", "INBOX", FolderRole::Inbox).unwrap();
    let state = SyncState {
        uidvalidity: 1,
        uidnext: 5,
        highestmodseq: None,
    };
    db.set_sync_state(id, &state).unwrap();
    let _ = db.step(|db| {
        db.reset_folder(id)?;
        Err::<(), _>(StoreError::NotFound)
    });
    assert_eq!(db.folder(id).unwrap().sync, Some(state));
}
