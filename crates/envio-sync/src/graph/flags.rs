//! Writing flags: the IMAP flags as a Graph PATCH. `\Seen` is `isRead`,
//! `\Flagged` the flag status, keywords are categories (read, merged and
//! written back, so another client's categories survive). `\Answered`,
//! `\Draft` and `\Deleted` have no Graph write and are dropped.

use envio_net::Result;
use envio_net::graph::Patch;
use envio_net::imap::Flag;

use super::{GraphServer, storage};

/// What a flag change asks of Graph.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Wanted {
    pub read: Option<bool>,
    pub flagged: Option<bool>,
    pub keywords: Vec<String>,
}

pub(super) fn wanted(add: bool, flags: &[Flag]) -> Wanted {
    let mut out = Wanted::default();
    for flag in flags {
        match flag {
            Flag::Seen => out.read = Some(add),
            Flag::Flagged => out.flagged = Some(add),
            Flag::Keyword(k) => out.keywords.push(k.clone()),
            Flag::Answered | Flag::Draft | Flag::Deleted => {}
        }
    }
    out
}

/// `current` with the keywords added or removed (Graph compares category
/// names without regard to case).
pub(super) fn merge(current: &[String], add: bool, keywords: &[String]) -> Vec<String> {
    let mut out = current.to_vec();
    for k in keywords {
        let same = |c: &String| c.eq_ignore_ascii_case(k);
        if add {
            if !out.iter().any(same) {
                out.push(k.clone());
            }
        } else {
            out.retain(|c| !same(c));
        }
    }
    out
}

impl GraphServer {
    pub(super) async fn set_flags(
        &mut self,
        uids: &[u32],
        add: bool,
        flags: &[Flag],
    ) -> Result<()> {
        let want = wanted(add, flags);
        if want == Wanted::default() {
            return Ok(());
        }
        let folder = self.selected()?;
        for (_, graph_id) in self.map.ids_of(folder.id, uids).map_err(storage)? {
            let mut patch = Patch {
                read: want.read,
                flagged: want.flagged,
                categories: None,
            };
            if !want.keywords.is_empty() {
                // A message that is gone has nothing to change.
                let Some(current) = self.client.get_categories(&graph_id).await? else {
                    continue;
                };
                let merged = merge(&current, add, &want.keywords);
                if merged != current {
                    patch.categories = Some(merged);
                }
            }
            if patch != Patch::default() {
                self.client.patch(&graph_id, &patch).await?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_seen_flagged_and_keywords_are_written() {
        let all = [
            Flag::Seen,
            Flag::Answered,
            Flag::Draft,
            Flag::Deleted,
            Flag::Flagged,
            Flag::Keyword("K".into()),
        ];
        let w = wanted(false, &all);
        assert_eq!(
            (w.read, w.flagged, w.keywords),
            (Some(false), Some(false), vec!["K".to_string()])
        );
        assert_eq!(
            wanted(true, &[Flag::Answered, Flag::Draft]),
            Wanted::default()
        );
    }

    #[test]
    fn merge_is_case_blind_and_never_duplicates() {
        let cur = vec!["Red".to_string(), "B".to_string()];
        assert_eq!(
            merge(&cur, true, &["red".into(), "C".into()]),
            ["Red", "B", "C"]
        );
        assert_eq!(merge(&cur, false, &["b".into()]), ["Red"]);
    }
}
