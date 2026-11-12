//! Full-text search over cached mail (FTS5).

use rusqlite::params;

use crate::db::Db;
use crate::error::Result;
use crate::folders::FolderId;
use crate::messages::{COLUMNS, Message, MessageRowId, from_row};

/// How much of a body's text is indexed.
pub const BODY_INDEX_BYTES: usize = 65_536;

/// Where a search looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Within<'a> {
    Everything,
    Account(&'a str),
    Folder(FolderId),
}

impl Db {
    /// Messages in `account` matching every word of `query` (each word also
    /// matches as a prefix) in the envelope fields or in the indexed body,
    /// newest first, each message once.
    pub fn search(&self, account: &str, query: &str, limit: u32) -> Result<Vec<Message>> {
        self.search_in(Within::Account(account), query, limit)
    }

    /// Like [`Db::search`], over every account, one account or one folder.
    pub fn search_in(&self, within: Within<'_>, query: &str, limit: u32) -> Result<Vec<Message>> {
        let Some(fts) = fts_query(query) else {
            return Ok(Vec::new());
        };
        let (account, folder) = match within {
            Within::Everything => (None, None),
            Within::Account(account) => (Some(account), None),
            Within::Folder(folder) => (None, Some(folder.0)),
        };
        let mut stmt = self.conn().prepare(&format!(
            "SELECT {COLUMNS} FROM messages m
             JOIN folders f ON f.id = m.folder_id
             WHERE (?2 IS NULL OR f.account = ?2) AND (?4 IS NULL OR f.id = ?4)
             AND m.id IN (
                 SELECT rowid FROM messages_fts WHERE messages_fts MATCH ?1
                 UNION
                 SELECT rowid FROM bodies_fts WHERE bodies_fts MATCH ?1)
             ORDER BY m.date DESC, m.id DESC LIMIT ?3"
        ))?;
        let messages = stmt
            .query_map(params![fts, account, limit, folder], from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(messages)
    }

    /// Makes a message's body text searchable (the first `BODY_INDEX_BYTES`
    /// bytes, cut at a character boundary); replaces an earlier index entry.
    /// The entry goes with the message.
    pub fn index_body(&self, id: MessageRowId, text: &str) -> Result<()> {
        let text = truncate(text, BODY_INDEX_BYTES);
        self.savepoint(|| {
            self.conn()
                .execute("DELETE FROM bodies_fts WHERE rowid = ?1", [id.0])?;
            self.conn().execute(
                "INSERT INTO bodies_fts (rowid, text)
                 SELECT id, ?2 FROM messages WHERE id = ?1",
                params![id.0, text],
            )?;
            Ok(())
        })
    }
}

fn truncate(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
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
    fn truncation_keeps_whole_characters() {
        assert_eq!(truncate("abc", 5), "abc");
        assert_eq!(truncate("abcdef", 3), "abc");
        assert_eq!(truncate("aéb", 2), "a");
        assert_eq!(truncate("aéb", 3), "aé");
    }

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
