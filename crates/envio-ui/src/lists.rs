//! The rows a message model shows, and the operations that change them:
//! reset, append a page, merge a refreshed first page.

use envio_core::Cursor;

use crate::models::Op;
use crate::rows::{MessageRowUi, Row};

/// A shown row and where it sits in the newest-first order.
#[derive(Debug, Clone)]
pub struct Shown {
    pub row: MessageRowUi,
    pub cursor: Cursor,
}

impl Shown {
    fn key(&self) -> (i64, i64) {
        (self.cursor.date, self.cursor.id.0)
    }
}

#[derive(Debug, Default)]
pub struct ShownList {
    rows: Vec<Shown>,
}

fn rows_of(shown: &[Shown]) -> Vec<Row> {
    shown.iter().map(|s| Row::Message(s.row.clone())).collect()
}

impl ShownList {
    pub fn contains(&self, id: i64) -> bool {
        self.rows.iter().any(|s| s.row.id == id)
    }

    /// A shown row with a local draft behind it is among these ids.
    pub fn has_local(&self, ids: &std::collections::BTreeSet<i64>) -> bool {
        self.rows
            .iter()
            .any(|s| !s.row.draft_id.is_empty() && ids.contains(&s.row.id))
    }

    /// Where the next page starts: after the last row shown.
    pub fn last_cursor(&self) -> Option<Cursor> {
        self.rows.last().map(|s| s.cursor)
    }

    pub fn reset(&mut self, rows: Vec<Shown>) -> Op {
        let op = Op::Reset(rows_of(&rows));
        self.rows = rows;
        op
    }

    /// Add an older page at the end; rows already shown are skipped.
    pub fn append(&mut self, page: Vec<Shown>) -> Option<Op> {
        let fresh: Vec<Shown> = page
            .into_iter()
            .filter(|s| !self.contains(s.row.id))
            .collect();
        if fresh.is_empty() {
            return None;
        }
        let at = self.rows.len();
        let op = Op::Insert {
            at,
            rows: rows_of(&fresh),
        };
        self.rows.extend(fresh);
        Some(op)
    }

    /// Merge a fresh first page; see `merge_page`.
    pub fn merge_first(&mut self, first: Vec<Shown>, end: bool) -> Vec<Op> {
        self.merge_page(None, first, end)
    }

    /// Whether a shown row is older than `cursor`, so a refresh must read
    /// further down.
    pub fn has_older_than(&self, cursor: Cursor) -> bool {
        let key = (cursor.date, cursor.id.0);
        self.rows.iter().any(|s| s.key() < key)
    }

    /// Merge a fresh page that starts after `upper` (`None`: the first
    /// page): new rows go in at their sorted place, changed rows are
    /// updated, rows gone from the range the page covers are removed; rows
    /// beyond that range stay. `end`: the page was the last, so it covers
    /// everything older than `upper`.
    pub fn merge_page(&mut self, upper: Option<Cursor>, page: Vec<Shown>, end: bool) -> Vec<Op> {
        let upper = upper.map(|c| (c.date, c.id.0));
        let bound = if end {
            None
        } else {
            page.last().map(Shown::key)
        };
        let covers = |s: &Shown| {
            upper.is_none_or(|u| s.key() < u) && (end || bound.is_some_and(|b| s.key() >= b))
        };
        let mut ops = self.remove_gone(&page, covers);
        let start = upper.map_or(0, |u| {
            self.rows
                .iter()
                .position(|s| s.key() < u)
                .unwrap_or(self.rows.len())
        });
        let (mut i, mut p, mut insert_at) = (start, start, None::<usize>);
        let mut pending: Vec<Shown> = Vec::new();
        for new in page {
            if self.rows.get(i).is_some_and(|s| s.key() == new.key()) {
                flush(&mut ops, &mut insert_at, &mut pending);
                if self.rows[i].row != new.row {
                    ops.push(Op::Update {
                        at: p,
                        row: Row::Message(new.row.clone()),
                    });
                }
                self.rows[i] = new;
                i += 1;
            } else {
                insert_at.get_or_insert(p);
                self.rows.insert(i, new.clone());
                i += 1;
                pending.push(new);
            }
            p += 1;
        }
        flush(&mut ops, &mut insert_at, &mut pending);
        ops
    }

    /// Remove the shown rows in the page's range that the page lacks, as
    /// runs from the back so earlier positions stay valid.
    fn remove_gone(&mut self, page: &[Shown], covers: impl Fn(&Shown) -> bool) -> Vec<Op> {
        let gone = |s: &Shown| covers(s) && !page.iter().any(|f| f.key() == s.key());
        let mut ops = Vec::new();
        let mut i = self.rows.len();
        while i > 0 {
            i -= 1;
            if !gone(&self.rows[i]) {
                continue;
            }
            let end_at = i + 1;
            while i > 0 && gone(&self.rows[i - 1]) {
                i -= 1;
            }
            ops.push(Op::Remove {
                at: i,
                count: end_at - i,
            });
            self.rows.drain(i..end_at);
        }
        ops
    }

    /// Set the selected mark of a shown row; the update to apply, if the
    /// row is shown and changes.
    pub fn set_selected(&mut self, id: i64, on: bool) -> Option<Op> {
        let at = self.rows.iter().position(|s| s.row.id == id)?;
        let shown = &mut self.rows[at];
        if shown.row.selected == on {
            return None;
        }
        shown.row.selected = on;
        Some(Op::Update {
            at,
            row: Row::Message(shown.row.clone()),
        })
    }

    /// Set the open mark of a shown row; the update to apply, if the row
    /// is shown and changes.
    pub fn set_open(&mut self, id: i64, on: bool) -> Option<Op> {
        let at = self.rows.iter().position(|s| s.row.id == id)?;
        let shown = &mut self.rows[at];
        if shown.row.open == on {
            return None;
        }
        shown.row.open = on;
        Some(Op::Update {
            at,
            row: Row::Message(shown.row.clone()),
        })
    }

    /// The shown ids, in order.
    pub fn ids(&self) -> Vec<i64> {
        self.rows.iter().map(|s| s.row.id).collect()
    }

    /// Set the keyboard-focus mark of a shown row; the update to apply, if
    /// the row is shown and changes.
    pub fn set_focus(&mut self, id: i64, on: bool) -> Option<Op> {
        let at = self.rows.iter().position(|s| s.row.id == id)?;
        let shown = &mut self.rows[at];
        if shown.row.kbd_focus == on {
            return None;
        }
        shown.row.kbd_focus = on;
        Some(Op::Update {
            at,
            row: Row::Message(shown.row.clone()),
        })
    }

    /// Show a new name on the rows of an account: one update per row.
    pub fn rename_account(&mut self, account: &str, name: &str) -> Vec<Op> {
        let mut ops = Vec::new();
        for (at, shown) in self.rows.iter_mut().enumerate() {
            if shown.row.account == account && shown.row.account_name != name {
                name.clone_into(&mut shown.row.account_name);
                ops.push(Op::Update {
                    at,
                    row: Row::Message(shown.row.clone()),
                });
            }
        }
        ops
    }

    /// Drop the rows of an account, as runs from the back.
    pub fn remove_account(&mut self, account: &str) -> Vec<Op> {
        let mut ops = Vec::new();
        let mut i = self.rows.len();
        while i > 0 {
            i -= 1;
            if self.rows[i].row.account != account {
                continue;
            }
            let end_at = i + 1;
            while i > 0 && self.rows[i - 1].row.account == account {
                i -= 1;
            }
            ops.push(Op::Remove {
                at: i,
                count: end_at - i,
            });
            self.rows.drain(i..end_at);
        }
        ops
    }
}

fn flush(ops: &mut Vec<Op>, at: &mut Option<usize>, pending: &mut Vec<Shown>) {
    if let Some(at) = at.take() {
        ops.push(Op::Insert {
            at,
            rows: rows_of(pending),
        });
        pending.clear();
    }
}
