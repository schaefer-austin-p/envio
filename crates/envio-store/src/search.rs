//! Full-text search over cached mail (FTS5).

use rusqlite::params;

use crate::db::Db;
use crate::error::Result;
use crate::messages::{COLUMNS, Message, from_row};

impl Db {
    /// Messages in `account` matching every word of `query` (each word also
    /// matches as a prefix), newest first.
    pub fn search(&self, account: &str, query: &str, limit: u32) -> Result<Vec<Message>> {
        let Some(fts) = fts_query(query) else {
            return Ok(Vec::new());
        };
        let mut stmt = self.conn().prepare(&format!(
            "SELECT {COLUMNS} FROM messages_fts
             JOIN messages m ON m.id = messages_fts.rowid
             JOIN folders f ON f.id = m.folder_id
             WHERE messages_fts MATCH ?1 AND f.account = ?2
             ORDER BY m.date DESC, m.id DESC LIMIT ?3"
        ))?;
        let messages = stmt
            .query_map(params![fts, account, limit], from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(messages)
    }
}

/// Turns what the user typed into an FTS5 query: each word in double quotes
/// (so FTS5 operators are plain text) with a prefix `*`, all words required.
fn fts_query(input: &str) -> Option<String> {
    let terms: Vec<String> = input
        .split_whitespace()
        .map(|word| word.replace('"', ""))
        .filter(|word| !word.is_empty())
        .map(|word| format!("\"{word}\"*"))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_quoted_and_prefixed() {
        assert_eq!(
            fts_query("ada  report").as_deref(),
            Some("\"ada\"* \"report\"*")
        );
        assert_eq!(fts_query("say\"hi\"").as_deref(), Some("\"sayhi\"*"));
        assert_eq!(fts_query("OR").as_deref(), Some("\"OR\"*"));
    }

    #[test]
    fn nothing_to_search_is_none() {
        assert_eq!(fts_query(""), None);
        assert_eq!(fts_query("  \t "), None);
        assert_eq!(fts_query("\"\" \""), None);
    }
}
