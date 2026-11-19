//! The Drafts list's rows for local drafts that have no server copy.
//! Pure: the worker reads the files, the cache and the journal, and these
//! functions decide what shows.

use std::collections::{HashMap, HashSet};

use envio_mime::Address;
use envio_store::{Action, Cursor, Draft, Flags, Folder, FolderId, MessageRowId};

use super::rows::{sender_label, subject};
use crate::api::view::MessageRow;
use crate::text;

/// The negative row id of each local draft, for this session. Cached rows
/// have positive SQLite rowids, so the two never collide.
pub(super) struct LocalIds {
    by_draft: HashMap<String, i64>,
    by_id: HashMap<i64, String>,
    next: i64,
}

impl Default for LocalIds {
    fn default() -> Self {
        Self {
            by_draft: HashMap::new(),
            by_id: HashMap::new(),
            next: -1,
        }
    }
}

impl LocalIds {
    /// The draft's row id: the same one every time, never one used before.
    pub fn id_of(&mut self, draft_id: &str) -> MessageRowId {
        if let Some(id) = self.by_draft.get(draft_id) {
            return MessageRowId(*id);
        }
        let id = self.next;
        self.next -= 1;
        self.by_draft.insert(draft_id.to_owned(), id);
        self.by_id.insert(id, draft_id.to_owned());
        MessageRowId(id)
    }

    /// The draft a pseudo-row id stands for.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "T4 and T5 turn a tapped or deleted row back into its draft"
        )
    )]
    pub fn draft_of(&self, id: MessageRowId) -> Option<&str> {
        self.by_id.get(&id.0).map(String::as_str)
    }
}

/// Local drafts of `account` with no current server copy (Decision 2), as
/// rows. `on_server` holds the Message-IDs in the account's Drafts folder
/// and `leaving` those of copies a pending Delete or Move to Trash is
/// removing; both are trimmed. A draft is hidden as leaving only when it has
/// no save the copy lacks: the others are kept and uploaded again, so
/// hiding them would hide unsent work. A draft whose compose is open (`open`)
/// stays too, as #299 keeps its server copy.
pub(super) fn local_rows(
    drafts: &[(String, Draft)],
    (account, folder): (&str, FolderId),
    (on_server, leaving, open): (&HashSet<String>, &HashSet<String>, &HashSet<String>),
    ids: &mut LocalIds,
) -> Vec<MessageRow> {
    let mut rows = Vec::new();
    for (draft_id, draft) in drafts {
        if draft.account != account {
            continue;
        }
        let server = &draft.server;
        if server.incomplete {
            // A half-done import: its server row is the real one.
            continue;
        }
        let message_id = server.message_id.as_deref().map(str::trim);
        let pending_id = server.pending_id.as_deref().map(str::trim);
        // Every id the draft has or had on the server, not the import
        // original (`imported_from`): a detached copy leaves it in place.
        let had = || {
            message_id
                .into_iter()
                .chain(server.earlier.iter().map(|e| e.trim()))
        };
        let copied = had().chain(pending_id).any(|id| on_server.contains(id));
        let going = had().any(|id| leaving.contains(id))
            && server.uploaded_ms == draft.updated_at_ms
            && !open.contains(draft_id);
        if copied || going {
            continue;
        }
        let id = ids.id_of(draft_id);
        let date = draft.updated_at_ms / 1000;
        rows.push(MessageRow {
            id,
            folder,
            account: account.to_owned(),
            cursor: Cursor { date, id },
            has_attachments: !draft.attachments.is_empty(),
            from: to_line(&draft.to),
            from_address: first_address(&draft.to),
            subject: subject(draft.subject.clone()),
            preview: preview(&draft.body),
            date: Some(date),
            flags: Flags::empty(),
            local_draft: Some(draft_id.clone()),
        });
    }
    rows
}

/// The first non-empty line of the body, trimmed, about 100 characters.
fn preview(body: &str) -> String {
    let line = body.lines().map(str::trim).find(|l| !l.is_empty());
    line.unwrap_or_default().chars().take(100).collect()
}

/// The cached page with the local rows in its range merged in, newest first
/// (Decision 7). The range is `(after, last cached key]`, or everything
/// older than `after` when `end`, so pages never overlap and the last key of
/// a page that is not the end is always a cached row.
pub(super) fn merge_page(
    page: Vec<MessageRow>,
    local: Vec<MessageRow>,
    after: Option<Cursor>,
    end: bool,
) -> Vec<MessageRow> {
    let key = |c: Cursor| (c.date, c.id);
    let floor = if end {
        None
    } else {
        page.last().map(|row| key(row.cursor))
    };
    let mut rows = page;
    rows.extend(local.into_iter().filter(|row| {
        let k = key(row.cursor);
        after.is_none_or(|a| k < key(a)) && floor.is_none_or(|f| k > f)
    }));
    rows.sort_by_key(|row| std::cmp::Reverse(key(row.cursor)));
    rows
}

/// "To: Bob, cy@example.org": the recipients' names or addresses.
pub(super) fn to_line(to: &[String]) -> String {
    let names: Vec<String> = to
        .iter()
        .map(|t| match Address::parse(t) {
            Some(a) => sender_label(a.name.as_deref(), &a.email),
            None => t.trim().to_owned(),
        })
        .collect();
    if names.is_empty() {
        text::read::to_recipients(&text::read::no_recipients_listed())
    } else {
        text::read::to_recipients(&names.join(", "))
    }
}

/// The addresses of a cached recipient list ("Name <a@b>, c@d"), split at
/// the commas that end an address.
pub(super) fn split_list(list: &str) -> Vec<String> {
    let (mut out, mut at, mut angle) = (Vec::new(), 0, false);
    for (i, c) in list.char_indices() {
        match c {
            '<' => angle = true,
            '>' => angle = false,
            ',' if !angle && ends_address(list[at..i].trim()) => {
                out.push(&list[at..i]);
                at = i + 1;
            }
            _ => {}
        }
    }
    out.push(&list[at..]);
    out.into_iter()
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Whether the text so far is a whole address. The cache holds names
/// unquoted (`Address::display()`), so a comma inside a name must not split:
/// only a closing `>` or a bare address (an `@`, no space) ends one.
fn ends_address(text: &str) -> bool {
    text.ends_with('>') || (text.contains('@') && !text.contains(char::is_whitespace))
}

/// What a Drafts row shows for a cached message's recipients: the To line
/// and the key its stamp is tinted by (the first recipient's address).
pub(super) fn recipients_of(to: &str) -> (String, String) {
    let to = split_list(to);
    (to_line(&to), first_address(&to))
}

fn first_address(to: &[String]) -> String {
    let first = to.first().map(String::as_str).unwrap_or_default();
    Address::parse(first).map_or_else(|| first.to_owned(), |a| a.email)
}

/// The trimmed Message-IDs of the Drafts copies that a pending action of
/// `account` is removing for good. Goes through the same `leaves_drafts` as
/// #299's `drafts_to_forget`, so the two cannot drift.
pub(super) fn leaving<'a>(
    pending: impl Iterator<Item = &'a Action>,
    account: &str,
    folders: &[Folder],
) -> HashSet<String> {
    pending
        .filter(|action| action.target.account == account)
        .filter_map(|action| envio_sync::plan::drafts::leaves_drafts(action, folders))
        .map(|id| id.trim().to_owned())
        .collect()
}

#[cfg(test)]
mod tests;
