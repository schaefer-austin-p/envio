//! The "Load older messages" row: shown at the end of a folder or the
//! unified inbox, pressed to fetch one batch, and `Done` once the server
//! has nothing older. Search results are their own page without the row,
//! so a search leaves it alone: the list behind it keeps its row.

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

    /// The list was scrolled near its end: ask for a batch, like a tap, but
    /// only once until the batch is answered (a fling fires this many times),
    /// and not for a list whose last batch failed or found nothing (also
    /// after leaving and reopening it), so offline there is no retry loop and
    /// the row stays for a tap.
    pub fn load_older_near_end(&mut self) -> Option<(Command, Vec<Output>)> {
        let list = self.current.as_ref()?.list;
        if self.older != OlderRow::Ready || self.older_auto_blocked.contains(&list) {
            return None;
        }
        self.load_older()
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
        self.older_paging = false;
        self.set_older(OlderRow::Hidden)
    }

    /// What Load older reached is dropped: "no older messages" no longer
    /// holds, so the row can be pressed again.
    pub(super) fn older_dropped(&mut self) -> Vec<Output> {
        self.older_done = false;
        self.older_auto_blocked.clear();
        if self.older == OlderRow::Done {
            return self.set_older(OlderRow::Ready);
        }
        Vec::new()
    }

    /// Asks for the page below the loaded rows once the list is idle (the
    /// view is not relied on: a short list never scrolls).
    fn older_page_in(&mut self) -> Vec<Output> {
        if !self.older_paging {
            return Vec::new();
        }
        let at_end = self.current.as_ref().is_none_or(|c| c.end);
        if at_end {
            // Nothing to page in (the list ended anyway): a later refresh
            // must not send a page nobody asked for.
            self.older_paging = false;
            return Vec::new();
        }
        let Some(command) = self.more() else {
            return Vec::new();
        };
        self.older_paging = false;
        vec![Output::Send(command)]
    }

    /// A page was shown: the rows Load older cached are paged in, and at
    /// the end of the list a hidden row appears.
    pub(super) fn older_shown(&mut self) -> Vec<Output> {
        let mut outs = self.older_page_in();
        outs.extend(self.older_ready());
        outs
    }

    fn older_ready(&mut self) -> Vec<Output> {
        let at_end = self.current.as_ref().is_some_and(|c| c.end);
        if self.older != OlderRow::Hidden || !at_end {
            return Vec::new();
        }
        self.older_paging = false;
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
        // Nothing more to find (or nothing found): scrolling stops asking.
        if exhausted || fetched == 0 {
            self.older_auto_blocked.insert(list);
        } else {
            self.older_auto_blocked.remove(&list);
        }
        if fetched > 0 {
            cur.end = false;
            self.older_paging = true;
            let mut outs = self.set_older(OlderRow::Hidden);
            outs.extend(self.older_page_in());
            return outs;
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
        self.older_auto_blocked.insert(list);
        let mut outs = self.set_older(OlderRow::Ready);
        outs.push(Output::Signal(Signal::Notice(text)));
        outs
    }
}
