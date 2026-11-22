//! Reading messages through the map: the searches by date, summaries,
//! flag updates and bodies. Flags always come from the map rows, which the
//! delta refresh keeps current.

use std::collections::HashSet;

use envio_net::Result;
use envio_net::imap::{FlagUpdate, MessageFlags, Summary};
use envio_store::{Flags, MapRow};

use super::{GraphServer, storage};

/// The IMAP flags of a map row. `\Answered` is never reported (Graph has no
/// such state) and `\Deleted` does not exist there.
pub(super) fn flags_of(row: &MapRow) -> MessageFlags {
    let has = |flag: Flags| row.flags & u8::from(flag) != 0;
    MessageFlags {
        seen: has(Flags::SEEN),
        answered: false,
        flagged: has(Flags::FLAGGED),
        deleted: false,
        draft: has(Flags::DRAFT),
        keywords: row.categories.clone(),
    }
}

/// The header block of a summary: `Name: value` lines and the blank line.
/// A value is one line, so a CR or LF inside it (a folded header) becomes a
/// space and cannot end the block early.
pub(super) fn header_block(headers: &[(String, String)]) -> Vec<u8> {
    let mut out = String::new();
    for (name, value) in headers {
        let value = value.replace(['\r', '\n'], " ");
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    out.push_str("\r\n");
    out.into_bytes()
}

impl GraphServer {
    /// Uids received after `received_after_ms`, oldest first.
    pub(super) fn uids_since(&mut self, received_after_ms: i64) -> Result<Vec<u32>> {
        let folder = self.selected()?;
        let rows = self
            .map
            .rows_since(folder.id, received_after_ms)
            .map_err(storage)?;
        Ok(rows.into_iter().map(|r| r.uid).collect())
    }

    pub(super) async fn summaries(&mut self, uids: &[u32]) -> Result<Vec<Summary>> {
        let folder = self.selected()?;
        let ids = self.map.ids_of(folder.id, uids).map_err(storage)?;
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let graph_ids: Vec<String> = ids.iter().map(|(_, id)| id.clone()).collect();
        let got = self.client.summaries(&graph_ids).await?;
        let mut out = Vec::new();
        for (uid, graph_id) in ids {
            let Some(s) = got.iter().find(|s| s.id == graph_id) else {
                continue;
            };
            let Some(row) = self.map.row(folder.id, uid).map_err(storage)? else {
                continue;
            };
            out.push(Summary {
                uid,
                flags: flags_of(&row),
                size: s.size,
                internal_date: Some(s.received_ms.div_euclid(1000)),
                modseq: Some(row.modseq),
                header: header_block(&s.headers),
            });
        }
        Ok(out)
    }

    pub(super) fn flag_updates(
        &mut self,
        uids: &[u32],
        changed_since: Option<u64>,
    ) -> Result<Vec<FlagUpdate>> {
        let folder = self.selected()?;
        let wanted: HashSet<u32> = uids.iter().copied().collect();
        let rows = match changed_since {
            Some(modseq) => self.map.rows_changed(folder.id, modseq),
            None => {
                let mut rows = Vec::new();
                for &uid in uids {
                    rows.extend(self.map.row(folder.id, uid).map_err(storage)?);
                }
                Ok(rows)
            }
        }
        .map_err(storage)?;
        Ok(rows
            .into_iter()
            .filter(|r| wanted.contains(&r.uid))
            .map(|r| FlagUpdate {
                uid: r.uid,
                flags: flags_of(&r),
                modseq: Some(r.modseq),
            })
            .collect())
    }

    pub(super) async fn body(&mut self, uid: u32) -> Result<Option<Vec<u8>>> {
        let folder = self.selected()?;
        let ids = self.map.ids_of(folder.id, &[uid]).map_err(storage)?;
        match ids.first() {
            Some((_, graph_id)) => self.client.message_mime(graph_id).await,
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folded_value_stays_one_line() {
        let h = vec![("Subject".to_string(), "a\r\n b".to_string())];
        assert_eq!(header_block(&h), b"Subject: a   b\r\n\r\n");
    }

    #[test]
    fn answered_is_never_reported() {
        let row = MapRow {
            uid: 1,
            graph_id: "x".into(),
            received_ms: 0,
            flags: u8::from(Flags::ANSWERED) | u8::from(Flags::SEEN),
            categories: vec!["K".into()],
            modseq: 1,
        };
        let f = flags_of(&row);
        assert!(f.seen && !f.answered && f.keywords == ["K"]);
    }
}
