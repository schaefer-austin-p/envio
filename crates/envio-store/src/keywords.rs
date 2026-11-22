//! The IMAP keywords of a cached message (issue #324). The column is
//! nullable: `NULL` is unknown, `''` is "none", otherwise the keywords sorted
//! and joined by one space (a keyword is an atom, so it holds no space).
//! Callers treat unknown like "has keywords".

use rusqlite::{OptionalExtension, params};

use crate::db::{Db, found};
use crate::error::Result;
use crate::folders::FolderId;

fn encode(keywords: &[String]) -> String {
    let mut sorted: Vec<&str> = keywords.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    sorted.dedup();
    sorted.join(" ")
}

fn decode(stored: Option<&str>) -> Option<Vec<String>> {
    stored.map(|text| text.split_whitespace().map(str::to_owned).collect())
}

/// The probe behind [`Db::has_unknown_keywords_between`]; migration 0007's
/// partial index serves it.
const UNKNOWN_SQL: &str = "SELECT EXISTS(SELECT 1 FROM messages
     WHERE folder_id = ?1 AND uid BETWEEN ?2 AND ?3 AND keywords IS NULL)";

impl Db {
    /// Records the keywords the server lists for a cached message (sorted,
    /// deduplicated). A message that is not cached is an error.
    pub fn set_keywords(&self, folder: FolderId, uid: u32, keywords: &[String]) -> Result<()> {
        found(self.conn().execute(
            "UPDATE messages SET keywords = ?3 WHERE folder_id = ?1 AND uid = ?2",
            params![folder.0, uid, encode(keywords)],
        )?)
    }

    /// The cached keywords: `None` when unknown or not cached.
    pub fn keywords_of(&self, folder: FolderId, uid: u32) -> Result<Option<Vec<String>>> {
        let stored: Option<Option<String>> = self
            .conn()
            .query_row(
                "SELECT keywords FROM messages WHERE folder_id = ?1 AND uid = ?2",
                params![folder.0, uid],
                |row| row.get(0),
            )
            .optional()?;
        Ok(decode(stored.flatten().as_deref()))
    }

    /// Every cached UID of a folder with its keywords (`None` unknown),
    /// ascending by UID.
    pub fn keywords_by_uid(&self, folder: FolderId) -> Result<Vec<(u32, Option<Vec<String>>)>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT uid, keywords FROM messages WHERE folder_id = ?1 ORDER BY uid")?;
        let rows = stmt
            .query_map([folder.0], |row| {
                let stored: Option<String> = row.get(1)?;
                Ok((row.get(0)?, decode(stored.as_deref())))
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// True if a cached row of the folder with a UID in `lo..=hi` has
    /// unknown (`NULL`) keywords. One indexed lookup, however long the
    /// folder is; rows outside the range never count.
    pub fn has_unknown_keywords_between(&self, folder: FolderId, lo: u32, hi: u32) -> Result<bool> {
        Ok(self.conn().query_row(
            UNKNOWN_SQL,
            params![folder.0, lo, hi],
            |row| row.get(0),
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_sorts_and_deduplicates_and_decode_inverts() {
        let list = |l: &[&str]| l.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(encode(&[]), "");
        assert_eq!(encode(&list(&["$b", "$a", "$b"])), "$a $b");
        assert_eq!(decode(None), None);
        assert_eq!(decode(Some("")), Some(vec![]));
        assert_eq!(decode(Some("$A $B")), Some(list(&["$A", "$B"])));
    }

    /// The unknown-keywords probe reads the partial index (empty on a folder
    /// whose rows are all known), not the folder's UID range (#399).
    #[test]
    fn unknown_keywords_probe_uses_the_partial_index() {
        let db = Db::open_in_memory().unwrap();
        let plan: Vec<String> = db
            .conn()
            .prepare(&format!("EXPLAIN QUERY PLAN {UNKNOWN_SQL}"))
            .unwrap()
            .query_map(params![1, 1, 9], |row| row.get(3))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        let plan = plan.join(" | ");
        eprintln!("plan: {plan}");
        assert!(plan.contains("messages_unknown_keywords"), "{plan}");
        // Only a partial index stays empty once the keywords are known.
        let sql: String = db
            .conn()
            .query_row(
                "SELECT sql FROM sqlite_master WHERE name = 'messages_unknown_keywords'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(sql.ends_with("WHERE keywords IS NULL"), "{sql}");
    }

    /// One run on a synthetic 30k-row folder, all keywords known: prints the
    /// probe's time (`--nocapture`). The guard is the plan test above.
    #[test]
    fn unknown_keywords_probe_on_a_large_known_folder() {
        let db = Db::open_in_memory().unwrap();
        let f = db
            .upsert_folder("a1", "INBOX", crate::FolderRole::Inbox)
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO messages (folder_id, uid, date, keywords)
                 WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 30000)
                 SELECT ?1, i, i, '' FROM n",
                [f.0],
            )
            .unwrap();
        let start = std::time::Instant::now();
        assert!(!db.has_unknown_keywords_between(f, 1, 30_000).unwrap());
        eprintln!("probe on 30k known rows: {:?}", start.elapsed());
        db.conn()
            .execute("UPDATE messages SET keywords = NULL WHERE uid = 17", [])
            .unwrap();
        assert!(db.has_unknown_keywords_between(f, 1, 30_000).unwrap());
        assert!(!db.has_unknown_keywords_between(f, 18, 30_000).unwrap());
    }
}
