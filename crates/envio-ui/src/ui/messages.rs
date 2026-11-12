//! The shown message list: pages, refreshes after new mail.

use envio_core::{Command, Cursor, Flags, FolderId, FolderRole, ListRef, MessageRow};

use super::{Output, Ui, change};
use crate::dates::bucket;
use crate::lists::{Shown, ShownList};
use crate::models::Model;
use crate::rows::MessageRowUi;

/// The first page of the shown list: not asked, asked for the first
/// time, or asked again to merge. A refresh walks down the loaded pages:
/// `Walk(after)` waits for the page after that cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum First {
    Idle,
    Initial,
    Refresh,
    Walk(Cursor),
}

#[derive(Debug)]
pub(super) struct Current {
    pub list: ListRef,
    pub shown: ShownList,
    pub first: First,
    /// Mail changed while the first page was outstanding: ask again.
    pub refresh_again: bool,
    /// The cursor of the outstanding next page.
    pub more: Option<Cursor>,
    pub end: bool,
}

impl Current {
    pub fn new(list: ListRef) -> Self {
        Self {
            list,
            shown: ShownList::default(),
            first: First::Initial,
            refresh_again: false,
            more: None,
            end: false,
        }
    }
}

impl Ui {
    /// The next page, unless one is on its way or the list is at its end.
    pub fn more(&mut self) -> Option<Command> {
        let cur = self.current.as_mut()?;
        if cur.end || cur.more.is_some() || matches!(cur.first, First::Initial | First::Walk(_)) {
            return None;
        }
        let after = cur.shown.last_cursor()?;
        cur.more = Some(after);
        Some(Command::ListMessages {
            list: cur.list,
            after: Some(after),
        })
    }

    /// Mail changed in these folders: reload the first page and then each
    /// further loaded page (so a row changed or gone past page one follows),
    /// unless the shown list does not hold them. The unified inbox reloads for a
    /// known Inbox and for a folder never seen (it may be a new Inbox).
    pub(super) fn messages_changed(&mut self, folders: &[FolderId]) -> Vec<Output> {
        let Some(cur) = self.current.as_mut() else {
            return Vec::new();
        };
        let affected = match cur.list {
            ListRef::Folder(id) => folders.contains(&id),
            ListRef::Inboxes => folders.iter().any(|f| {
                self.folder_roles
                    .get(&f.0)
                    .is_none_or(|(_, r)| *r == FolderRole::Inbox)
            }),
        };
        if !affected {
            return Vec::new();
        }
        if cur.first == First::Idle {
            cur.first = First::Refresh;
            vec![Output::Send(Command::ListMessages {
                list: cur.list,
                after: None,
            })]
        } else {
            cur.refresh_again = true;
            Vec::new()
        }
    }

    /// A page is not coming: forget that it is outstanding, so refreshes
    /// and paging work again. A failed refresh page ends the walk.
    pub(super) fn messages_failed(
        &mut self,
        (list, after): (ListRef, Option<Cursor>),
    ) -> Vec<Output> {
        let Some(cur) = self.current.as_mut().filter(|c| c.list == list) else {
            return Vec::new();
        };
        match after {
            None if cur.first != First::Idle => {
                cur.first = First::Idle;
            }
            Some(c) if cur.first == First::Walk(c) => cur.first = First::Idle,
            Some(c) if cur.more == Some(c) => cur.more = None,
            _ => {}
        }
        cur.refresh_again = false;
        Vec::new()
    }

    pub(super) fn shown(&self, r: &MessageRow) -> Shown {
        let name = self.names.get(&r.account).cloned().unwrap_or_default();
        Shown {
            cursor: r.cursor,
            row: MessageRowUi {
                id: r.id.0,
                account: r.account.clone(),
                account_name: name,
                from: r.from.clone(),
                subject: r.subject.clone(),
                preview: r.preview.clone(),
                date: r.date.unwrap_or(0),
                date_bucket: bucket(self.now, r.date, self.utc_offset_s),
                unread: !r.flags.contains(Flags::SEEN),
                flagged: r.flags.contains(Flags::FLAGGED),
                has_attachments: r.has_attachments,
                selected: self.selection.contains(&r.id.0),
            },
        }
    }

    /// A page of `(list, after)`.
    pub(super) fn messages(
        &mut self,
        (list, after): (ListRef, Option<Cursor>),
        rows: &[MessageRow],
        end: bool,
    ) -> Vec<Output> {
        let page: Vec<Shown> = rows.iter().map(|r| self.shown(r)).collect();
        if list == ListRef::Inboxes {
            for r in rows {
                self.folder_roles
                    .insert(r.folder.0, (r.account.clone(), FolderRole::Inbox));
            }
        }
        let Some(cur) = self.current.as_mut().filter(|c| c.list == list) else {
            return Vec::new();
        };
        let mut outs = Vec::new();
        match after {
            None => {
                let ops = match cur.first {
                    First::Idle | First::Walk(_) => return outs,
                    First::Initial => {
                        cur.end = end;
                        vec![cur.shown.reset(page)]
                    }
                    First::Refresh => {
                        cur.end |= end;
                        let last = page.last().map(|s| s.cursor);
                        let ops = cur.shown.merge_first(page, end);
                        outs.extend(ops.into_iter().map(|op| change(Model::Messages, op)));
                        outs.extend(walk_on(cur, list, last, end));
                        return outs;
                    }
                };
                outs.extend(ops.into_iter().map(|op| change(Model::Messages, op)));
                outs.extend(finish(cur, list));
            }
            Some(c) if cur.first == First::Walk(c) => {
                cur.end |= end;
                let last = page.last().map(|s| s.cursor);
                let ops = cur.shown.merge_page(Some(c), page, end);
                outs.extend(ops.into_iter().map(|op| change(Model::Messages, op)));
                outs.extend(walk_on(cur, list, last, end));
            }
            Some(c) if cur.more == Some(c) => {
                cur.more = None;
                cur.end = end;
                outs.extend(cur.shown.append(page).map(|op| change(Model::Messages, op)));
            }
            Some(_) => {}
        }
        outs
    }
}

/// A refresh page is merged: read the next one down while loaded rows lie
/// below it, else finish.
fn walk_on(cur: &mut Current, list: ListRef, last: Option<Cursor>, end: bool) -> Option<Output> {
    match last {
        Some(c) if !end && cur.shown.has_older_than(c) => {
            cur.first = First::Walk(c);
            Some(Output::Send(Command::ListMessages {
                list,
                after: Some(c),
            }))
        }
        _ => finish(cur, list),
    }
}

/// The refresh is done: go again if mail changed meanwhile.
fn finish(cur: &mut Current, list: ListRef) -> Option<Output> {
    if std::mem::take(&mut cur.refresh_again) {
        cur.first = First::Refresh;
        Some(Output::Send(Command::ListMessages { list, after: None }))
    } else {
        cur.first = First::Idle;
        None
    }
}
