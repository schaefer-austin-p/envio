//! The IMAP keywords of a cached message (issue #324). The column is
//! nullable: `NULL` is unknown, `''` is "none", otherwise the keywords sorted
//! and joined by one space (a keyword is an atom, so it holds no space).
//! Callers treat unknown like "has keywords".

use rusqlite::types::Value;
use rusqlite::{OptionalExtension, params, params_from_iter};

use crate::db::{Db, found};
use crate::error::Result;
use crate::folders::FolderId;

/// One chunk of UIDs stays under SQLite's bound-variable limit.
const UID_CHUNK: usize = 500;

fn encode(keywords: &[String]) -> String {
    let mut sorted: Vec<&str> = keywords.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    sorted.dedup();
    sorted.join(" ")
}

fn decode(stored: Option<&str>) -> Option<Vec<String>> {
    stored.map(|text| text.split_whitespace().map(str::to_owned).collect())
}

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

    /// True if any of these cached UIDs has unknown (`NULL`) keywords. UIDs
    /// that are not cached are ignored.
    pub fn has_unknown_keywords(&self, folder: FolderId, uids: &[u32]) -> Result<bool> {
        for chunk in uids.chunks(UID_CHUNK) {
            let marks = vec!["?"; chunk.len()].join(",");
            let args = std::iter::once(Value::from(folder.0))
                .chain(chunk.iter().map(|&uid| Value::from(i64::from(uid))));
            let hit: bool = self.conn().query_row(
                &format!(
                    "SELECT EXISTS(SELECT 1 FROM messages
                     WHERE folder_id = ? AND keywords IS NULL AND uid IN ({marks}))"
                ),
                params_from_iter(args),
                |row| row.get(0),
            )?;
            if hit {
                return Ok(true);
            }
        }
        Ok(false)
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
}
