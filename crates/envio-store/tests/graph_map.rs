use std::path::{Path, PathBuf};

use envio_store::{
    Allocation, Db, FolderId, FolderRole, GraphMap, MetaChange, StoreError, Upserted,
};
use rusqlite::Connection;

const HIGH: u32 = 1 << 31;

struct Setup {
    _dir: tempfile::TempDir,
    path: PathBuf,
    db: Db,
    map: GraphMap,
    folder: FolderId,
}

fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("envio.db");
    let db = Db::open(&path).unwrap();
    let map = GraphMap::open(&path).unwrap();
    let folder = db
        .upsert_folder("1700", "INBOX", FolderRole::Inbox)
        .unwrap();
    db.step(|db| db.graph_upsert_folder(folder, "AAA="))
        .unwrap();
    Setup {
        _dir: dir,
        path,
        db,
        map,
        folder,
    }
}

fn ids(list: &[(&str, i64)]) -> Vec<(String, i64)> {
    list.iter().map(|(id, ms)| (id.to_string(), *ms)).collect()
}

fn done(allocation: Allocation) -> Vec<(String, u32)> {
    match allocation {
        Allocation::Done(uids) => uids,
        Allocation::Exhausted => panic!("exhausted"),
    }
}

fn raw(path: &Path, sql: &str) {
    Connection::open(path).unwrap().execute_batch(sql).unwrap();
}

fn change(id: &str, flags: u8, categories: &[&str]) -> MetaChange {
    MetaChange {
        graph_id: id.into(),
        flags,
        categories: categories.iter().map(|c| c.to_string()).collect(),
    }
}

#[test]
fn a_new_folder_starts_with_both_ranges_open() {
    let s = setup();
    let folder = s.map.folder(s.folder).unwrap().unwrap();
    assert_eq!(folder.graph_id, "AAA=");
    assert_ne!(folder.generation, 0);
    assert_eq!(folder.uid_next, HIGH);
    assert_eq!(folder.low_next, HIGH - 1);
    assert_eq!((folder.modseq, folder.delta_link.as_str()), (0, ""));
}

#[test]
fn reset_twice_gives_two_different_non_zero_generations_and_an_empty_map() {
    let s = setup();
    done(s.map.allocate(s.folder, &ids(&[("m1", 1)])).unwrap());
    let first = s.map.reset(s.folder).unwrap();
    assert_eq!(s.map.rows_since(s.folder, 0).unwrap().len(), 0);
    let second = s.map.reset(s.folder).unwrap();
    assert!(first != 0 && second != 0 && first != second);
    let folder = s.map.folder(s.folder).unwrap().unwrap();
    assert_eq!(folder.generation, second);
    assert_eq!((folder.uid_next, folder.low_next), (HIGH, HIGH - 1));
}

#[test]
fn reset_clears_the_counters_and_the_delta_link() {
    let s = setup();
    s.db.graph_commit(s.folder, "https://example.org/delta?x=1", 9)
        .unwrap();
    done(s.map.allocate(s.folder, &ids(&[("m1", 1)])).unwrap());
    s.map.reset(s.folder).unwrap();
    let folder = s.map.folder(s.folder).unwrap().unwrap();
    assert_eq!((folder.modseq, folder.delta_link.as_str()), (0, ""));
    assert_eq!(folder.uid_next, HIGH);
}

#[test]
fn allocate_is_ascending_by_received_time() {
    let s = setup();
    let uids = done(
        s.map
            .allocate(s.folder, &ids(&[("b", 20), ("a", 10), ("c", 30)]))
            .unwrap(),
    );
    let by_id = |id: &str| uids.iter().find(|(i, _)| i == id).unwrap().1;
    assert_eq!(by_id("a"), HIGH);
    assert_eq!(by_id("b"), HIGH + 1);
    assert_eq!(by_id("c"), HIGH + 2);
    assert_eq!(s.map.folder(s.folder).unwrap().unwrap().uid_next, HIGH + 3);
}

#[test]
fn allocate_is_idempotent_per_graph_id_and_survives_a_reopen() {
    let s = setup();
    let first = done(
        s.map
            .allocate(s.folder, &ids(&[("a", 10), ("b", 20)]))
            .unwrap(),
    );
    drop(s.map);
    let map = GraphMap::open(&s.path).unwrap();
    let again = done(
        map.allocate(s.folder, &ids(&[("b", 20), ("c", 30), ("a", 10)]))
            .unwrap(),
    );
    for (id, uid) in &first {
        assert_eq!(again.iter().find(|(i, _)| i == id).unwrap().1, *uid);
    }
    let c = again.iter().find(|(i, _)| i == "c").unwrap().1;
    assert_eq!(c, HIGH + 2);
    assert_eq!(map.uid_of(s.folder, "a").unwrap(), Some(HIGH));
    assert_eq!(map.uid_of(s.folder, "zz").unwrap(), None);
}

#[test]
fn uids_are_never_reused_after_a_removal() {
    let s = setup();
    done(
        s.map
            .allocate(s.folder, &ids(&[("a", 1), ("b", 2)]))
            .unwrap(),
    );
    s.map.apply(s.folder, &[], &["b".to_string()]).unwrap();
    let uids = done(s.map.allocate(s.folder, &ids(&[("c", 3)])).unwrap());
    assert_eq!(uids, vec![("c".to_string(), HIGH + 2)]);
}

#[test]
fn allocate_near_the_top_is_exhausted_and_writes_nothing() {
    let s = setup();
    raw(
        &s.path,
        &format!("UPDATE graph_folders SET uid_next = {}", u32::MAX - 3),
    );
    // Two ids still fit: uid_next ends at 2^32 - 2.
    done(
        s.map
            .allocate(s.folder, &ids(&[("a", 1), ("b", 2)]))
            .unwrap(),
    );
    // Reaching 2^32 - 1 is not allowed.
    let result = s.map.allocate(s.folder, &ids(&[("c", 3)])).unwrap();
    assert!(matches!(result, Allocation::Exhausted));
    assert_eq!(s.map.uid_of(s.folder, "c").unwrap(), None);
    // An id that is already mapped needs no room.
    done(s.map.allocate(s.folder, &ids(&[("a", 1)])).unwrap());
}

#[test]
fn allocate_low_counts_down_and_returns_the_existing_uid_for_a_known_id() {
    let s = setup();
    let a = s.map.allocate_low(s.folder, "a", 5).unwrap().unwrap();
    let b = s.map.allocate_low(s.folder, "b", 6).unwrap().unwrap();
    assert_eq!((a, b), (HIGH - 1, HIGH - 2));
    assert_eq!(s.map.allocate_low(s.folder, "a", 5).unwrap(), Some(a));
    let row = s.map.row(s.folder, a).unwrap().unwrap();
    assert_eq!((row.graph_id.as_str(), row.received_ms), ("a", 5));
    // An id the high range mapped keeps that uid.
    let high = done(s.map.allocate(s.folder, &ids(&[("h", 7)])).unwrap());
    assert_eq!(
        s.map.allocate_low(s.folder, "h", 7).unwrap(),
        Some(high[0].1)
    );
}

#[test]
fn allocate_low_near_zero_is_exhausted() {
    let s = setup();
    raw(&s.path, "UPDATE graph_folders SET low_next = 2");
    assert_eq!(s.map.allocate_low(s.folder, "a", 1).unwrap(), Some(2));
    assert_eq!(s.map.allocate_low(s.folder, "b", 1).unwrap(), None);
    // The known id still answers.
    assert_eq!(s.map.allocate_low(s.folder, "a", 1).unwrap(), Some(2));
    assert_eq!(s.map.uid_of(s.folder, "b").unwrap(), None);
}

#[test]
fn upsert_folder_reports_new_same_and_changed() {
    let db = Db::open_in_memory().unwrap();
    let folder = db
        .upsert_folder("1700", "INBOX", FolderRole::Inbox)
        .unwrap();
    let new = db.graph_upsert_folder(folder, "AAA=").unwrap();
    let Upserted::New(generation) = new else {
        panic!("{new:?}")
    };
    assert_ne!(generation, 0);
    assert_eq!(
        db.graph_upsert_folder(folder, "AAA=").unwrap(),
        Upserted::Same
    );
    let changed = db.graph_upsert_folder(folder, "BBB=").unwrap();
    let Upserted::Changed(next) = changed else {
        panic!("{changed:?}")
    };
    assert!(next != 0 && next != generation);
    assert_eq!(
        db.graph_upsert_folder(folder, "BBB=").unwrap(),
        Upserted::Same
    );
}

#[test]
fn a_changed_graph_id_wipes_the_rows() {
    let s = setup();
    done(s.map.allocate(s.folder, &ids(&[("a", 1)])).unwrap());
    s.db.step(|db| db.graph_upsert_folder(s.folder, "BBB="))
        .unwrap();
    assert!(s.map.rows_since(s.folder, 0).unwrap().is_empty());
    assert_eq!(s.map.folder(s.folder).unwrap().unwrap().uid_next, HIGH);
}

#[test]
fn a_graph_id_that_moved_to_another_folder_row_takes_the_map_with_it() {
    let s = setup();
    let moved =
        s.db.upsert_folder("1700", "Moved", FolderRole::Other)
            .unwrap();
    s.db.graph_upsert_folder(moved, "AAA=").unwrap();
    assert!(s.map.folder(s.folder).unwrap().is_none());
    assert_eq!(s.map.folder_row_of("AAA=").unwrap(), Some(moved));
}

#[test]
fn a_reset_through_db_in_a_step_is_seen_by_the_next_map_call() {
    let s = setup();
    let before = s.map.folder(s.folder).unwrap().unwrap().generation;
    done(s.map.allocate(s.folder, &ids(&[("a", 1)])).unwrap());
    let fresh = s.db.step(|db| db.graph_reset(s.folder)).unwrap();
    assert_ne!(fresh, before);
    let after = s.map.folder(s.folder).unwrap().unwrap();
    assert_eq!(after.generation, fresh);
    assert_eq!(after.uid_next, HIGH);
    assert!(s.map.rows_since(s.folder, 0).unwrap().is_empty());
}

#[test]
fn commit_saves_the_delta_link_and_modseq() {
    let s = setup();
    s.db.step(|db| db.graph_commit(s.folder, "https://example.org/d?t=1", 4))
        .unwrap();
    let folder = s.map.folder(s.folder).unwrap().unwrap();
    assert_eq!(folder.delta_link, "https://example.org/d?t=1");
    assert_eq!(folder.modseq, 4);
}

#[test]
fn deleting_the_folder_cascades_both_tables() {
    let s = setup();
    done(s.map.allocate(s.folder, &ids(&[("a", 1)])).unwrap());
    raw(&s.path, "PRAGMA foreign_keys = ON; DELETE FROM folders");
    assert!(s.map.folder(s.folder).unwrap().is_none());
    assert!(s.map.rows_since(s.folder, 0).unwrap().is_empty());
    assert_eq!(s.map.folder_row_of("AAA=").unwrap(), None);
}

#[test]
fn apply_moves_the_modseq_of_changed_rows_only() {
    let s = setup();
    done(
        s.map
            .allocate(s.folder, &ids(&[("a", 1), ("b", 2), ("c", 3)]))
            .unwrap(),
    );
    // A first delta lists every message: nothing differs yet.
    s.map
        .apply(
            s.folder,
            &[
                change("a", 0, &[]),
                change("b", 0, &[]),
                change("c", 0, &[]),
            ],
            &[],
        )
        .unwrap();
    assert!(s.map.rows_changed(s.folder, 0).unwrap().is_empty());
    let old = s.map.folder(s.folder).unwrap().unwrap().modseq;
    s.map
        .apply(
            s.folder,
            &[
                change("a", 1, &[]),
                change("b", 0, &[]),
                change("c", 0, &["Red"]),
            ],
            &[],
        )
        .unwrap();
    let changed = s.map.rows_changed(s.folder, old).unwrap();
    let mut got: Vec<_> = changed.iter().map(|r| r.graph_id.as_str()).collect();
    got.sort();
    assert_eq!(got, ["a", "c"]);
    assert!(changed.iter().all(|r| r.modseq > old));
    let b = s.map.row(s.folder, HIGH + 1).unwrap().unwrap();
    assert_eq!(b.modseq, 0);
    let c = s.map.row(s.folder, HIGH + 2).unwrap().unwrap();
    assert_eq!(c.categories, ["Red"]);
    // The folder's counter moved too, so the next call gets a newer one.
    let now = s.map.folder(s.folder).unwrap().unwrap().modseq;
    assert!(now > old);
    // Repeating the same page changes nothing.
    s.map.apply(s.folder, &[change("a", 1, &[])], &[]).unwrap();
    assert!(s.map.rows_changed(s.folder, now).unwrap().is_empty());
    assert_eq!(s.map.folder(s.folder).unwrap().unwrap().modseq, now);
}

#[test]
fn apply_removes_rows_and_ignores_unknown_removals() {
    let s = setup();
    done(
        s.map
            .allocate(s.folder, &ids(&[("a", 1), ("b", 2)]))
            .unwrap(),
    );
    s.map
        .apply(s.folder, &[], &["a".to_string(), "gone".to_string()])
        .unwrap();
    assert_eq!(s.map.uid_of(s.folder, "a").unwrap(), None);
    assert_eq!(s.map.uid_of(s.folder, "b").unwrap(), Some(HIGH + 1));
}

#[test]
fn lookups_by_time_and_by_uid() {
    let s = setup();
    done(
        s.map
            .allocate(s.folder, &ids(&[("a", 10), ("b", 20), ("c", 30)]))
            .unwrap(),
    );
    let since: Vec<_> = s
        .map
        .rows_since(s.folder, 15)
        .unwrap()
        .into_iter()
        .map(|r| r.graph_id)
        .collect();
    assert_eq!(since, ["b", "c"]);
    let mut pairs = s.map.ids_of(s.folder, &[HIGH, HIGH + 2, 5]).unwrap();
    pairs.sort();
    assert_eq!(
        pairs,
        [(HIGH, "a".to_string()), (HIGH + 2, "c".to_string())]
    );
    assert!(s.map.row(s.folder, 5).unwrap().is_none());
}

#[test]
fn folder_row_of_finds_a_folder_after_a_reopen() {
    let s = setup();
    drop(s.map);
    let map = GraphMap::open(&s.path).unwrap();
    assert_eq!(map.folder_row_of("AAA=").unwrap(), Some(s.folder));
    assert_eq!(map.folder_row_of("nope").unwrap(), None);
}

#[test]
fn opening_a_database_that_was_never_migrated_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.db");
    raw(&path, "PRAGMA user_version = 5");
    let error = GraphMap::open(&path).err().unwrap();
    assert!(
        matches!(error, StoreError::SchemaTooOld { found: 5, .. }),
        "{error:?}"
    );
}

#[test]
fn two_folders_swapping_graph_ids_leave_no_stale_rows() {
    let s = setup();
    let other =
        s.db.upsert_folder("1700", "Other", FolderRole::Other)
            .unwrap();
    s.db.graph_upsert_folder(other, "BBB=").unwrap();
    done(s.map.allocate(s.folder, &ids(&[("a", 1)])).unwrap());
    done(s.map.allocate(other, &ids(&[("b", 1)])).unwrap());
    s.db.step(|db| {
        db.graph_upsert_folder(s.folder, "BBB=")?;
        db.graph_upsert_folder(other, "AAA=")
    })
    .unwrap();
    assert_eq!(s.map.ids_of(other, &[HIGH]).unwrap(), []);
    assert_eq!(s.map.ids_of(s.folder, &[HIGH]).unwrap(), []);
    let uids = done(s.map.allocate(other, &ids(&[("x", 2)])).unwrap());
    assert_eq!(uids, vec![("x".to_string(), HIGH)]);
    let uids = done(s.map.allocate(s.folder, &ids(&[("y", 2)])).unwrap());
    assert_eq!(uids, vec![("y".to_string(), HIGH)]);
    assert_eq!(
        s.map.ids_of(other, &[HIGH]).unwrap(),
        [(HIGH, "x".to_string())]
    );
}

#[test]
fn an_id_repeated_in_one_allocate_call_is_mapped_once() {
    let s = setup();
    let uids = done(
        s.map
            .allocate(s.folder, &ids(&[("a", 1), ("b", 2), ("a", 1)]))
            .unwrap(),
    );
    let a: Vec<_> = uids.iter().filter(|(id, _)| id == "a").collect();
    assert_eq!(a.len(), 1);
    assert_eq!(s.map.folder(s.folder).unwrap().unwrap().uid_next, HIGH + 2);
}

#[test]
fn count_is_the_rows_of_both_ranges() {
    let s = setup();
    assert_eq!(s.map.count(s.folder).unwrap(), 0);
    s.map
        .allocate(s.folder, &ids(&[("a", 1), ("b", 2)]))
        .unwrap();
    s.map.allocate_low(s.folder, "c", 3).unwrap();
    assert_eq!(s.map.count(s.folder).unwrap(), 3);
}
