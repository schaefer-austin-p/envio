//! The "Load older messages" row: shown at the end of a folder or the
//! unified inbox (never a search), pressed to fetch one batch, and `Done`
//! once the server has nothing older.

use envio_core::{Command, ListRef};

use super::{Output, Ui};
use crate::signals::{OlderRow, Signal};

impl Ui {
    /// The row was pressed: ask for a batch. `None` unless it is `Ready`.
    pub fn load_older(&mut self) -> Option<(Command, Vec<Output>)> {
        if self.older != OlderRow::Ready {
            return None;
        }
        let list = self.current.as_ref()?.list;
        let outs = self.set_older(OlderRow::Loading);
        Some((Command::LoadOlder { list }, outs))
    }

    /// Moves the row to `to`; the output is empty if it is there already.
    pub(super) fn set_older(&mut self, to: OlderRow) -> Vec<Output> {
        if self.older == to {
            return Vec::new();
        }
        self.older = to;
        vec![Output::Signal(Signal::OlderRow(to))]
    }

    /// A list is opened: the row starts over.
    pub(super) fn older_reset(&mut self) -> Vec<Output> {
        self.older_done = false;
        self.searching = false;
        self.set_older(OlderRow::Hidden)
    }

    /// A search is asked: no row for it.
    pub(super) fn older_search(&mut self) -> Vec<Output> {
        self.searching = true;
        self.set_older(OlderRow::Hidden)
    }

    /// A page was shown: at the end of the list, a hidden row appears.
    pub(super) fn older_shown(&mut self) -> Vec<Output> {
        let at_end = self.current.as_ref().is_some_and(|c| c.end);
        if self.older != OlderRow::Hidden || self.searching || !at_end {
            return Vec::new();
        }
        let row = if self.older_done {
            OlderRow::Done
        } else {
            OlderRow::Ready
        };
        self.set_older(row)
    }

    /// A batch was cached. Rows that arrived are paged in first (the list
    /// is no longer at its end), then the row returns.
    pub(super) fn older_loaded(
        &mut self,
        list: ListRef,
        fetched: usize,
        exhausted: bool,
    ) -> Vec<Output> {
        let Some(cur) = self.current.as_mut().filter(|c| c.list == list) else {
            return Vec::new();
        };
        if self.older != OlderRow::Loading {
            return Vec::new();
        }
        self.older_done = exhausted;
        if fetched > 0 {
            cur.end = false;
            return self.set_older(OlderRow::Hidden);
        }
        self.set_older(if exhausted {
            OlderRow::Done
        } else {
            OlderRow::Ready
        })
    }

    /// A batch could not be loaded: the row can be pressed again.
    pub(super) fn older_failed(&mut self, list: ListRef, text: String) -> Vec<Output> {
        let current = self.current.as_ref().is_some_and(|c| c.list == list);
        if !current || self.older != OlderRow::Loading {
            return Vec::new();
        }
        let mut outs = self.set_older(OlderRow::Ready);
        outs.push(Output::Signal(Signal::Notice(text)));
        outs
    }
}
