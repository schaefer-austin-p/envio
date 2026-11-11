//! Addresses seen in synced and sent mail, for autocomplete. A cache like
//! the rest of the database: sync refills it.

use rusqlite::params;

use crate::db::Db;
use crate::error::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeenAddress {
    /// Lower-case.
    pub address: String,
    /// The latest non-empty display name seen with it.
    pub name: String,
    /// Unix seconds.
    pub last_seen: i64,
    pub count: u32,
}

impl Db {
    /// Records `found` as (name, address) pairs seen at `when`: each pair
    /// adds one to its address's count. Addresses are lower-cased; an empty
    /// name never replaces a known one.
    pub fn remember_addresses(&self, found: &[(String, String)], when: i64) -> Result<()> {
        self.savepoint(|| {
            let mut stmt = self.conn().prepare(
                "INSERT INTO addresses (address, name, last_seen, count) VALUES (?1, ?2, ?3, 1)
                 ON CONFLICT (address) DO UPDATE SET
                     name = CASE WHEN excluded.name = '' THEN name ELSE excluded.name END,
                     last_seen = max(last_seen, excluded.last_seen),
                     count = count + 1",
            )?;
            for (name, address) in found {
                let address = address.trim().to_lowercase();
                if !address.is_empty() {
                    stmt.execute(params![address, name.trim(), when])?;
                }
            }
            Ok(())
        })
    }

    /// Up to `limit` addresses whose address, or any word of whose name,
    /// starts with `prefix` (case-insensitive; ASCII case only), most used
    /// first, then most recent. An empty prefix suggests nothing.
    pub fn suggest(&self, prefix: &str, limit: u32) -> Result<Vec<SeenAddress>> {
        let prefix = prefix.trim().to_lowercase();
        if prefix.is_empty() {
            return Ok(Vec::new());
        }
        let escaped = prefix
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let start = format!("{escaped}%");
        let word = format!("% {escaped}%");
        let mut stmt = self.conn().prepare(
            "SELECT address, name, last_seen, count FROM addresses
             WHERE address LIKE ?1 ESCAPE '\\'
                OR name LIKE ?1 ESCAPE '\\' OR name LIKE ?2 ESCAPE '\\'
             ORDER BY count DESC, last_seen DESC, address LIMIT ?3",
        )?;
        let found = stmt
            .query_map(params![start, word, limit], |row| {
                Ok(SeenAddress {
                    address: row.get(0)?,
                    name: row.get(1)?,
                    last_seen: row.get(2)?,
                    count: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(found)
    }
}
