#![allow(dead_code)]
use envio_core::{
    AccountInfo, AccountStatus, Cursor, Event, Flags, FolderId, FolderRole, FolderRow, ListRef,
    MessageRow, MessageRowId,
};
use envio_ui::{Model, ModelChange, Op, Output, Row, Signal, Ui};

pub const NOW: i64 = 1_800_000_000;

pub fn msg(id: i64, date: i64) -> MessageRow {
    msg_in(id, date, 1, "a")
}

pub fn msg_in(id: i64, date: i64, folder: i64, account: &str) -> MessageRow {
    MessageRow {
        id: MessageRowId(id),
        folder: FolderId(folder),
        account: account.into(),
        cursor: Cursor {
            date,
            id: MessageRowId(id),
        },
        has_attachments: false,
        from: "x".into(),
        from_address: "x@example.org".into(),
        subject: format!("s{id}"),
        preview: String::new(),
        date: Some(date),
        flags: Flags::default(),
        local_draft: None,
    }
}

pub fn page(list: ListRef, after: Option<Cursor>, rows: Vec<MessageRow>, end: bool) -> Event {
    Event::Messages {
        list,
        after,
        rows,
        end,
    }
}

pub fn feed(ui: &mut Ui, e: Event) -> Vec<Output> {
    ui.apply(e, NOW, 0)
}

pub fn changes(outs: &[Output], model: Model) -> Vec<&Op> {
    outs.iter()
        .filter_map(|o| match o {
            Output::Change(ModelChange { model: m, op }) if *m == model => Some(op),
            _ => None,
        })
        .collect()
}

pub fn signals(outs: &[Output]) -> Vec<&Signal> {
    outs.iter()
        .filter_map(|o| match o {
            Output::Signal(s) => Some(s),
            _ => None,
        })
        .collect()
}

/// The ids of the message rows an op carries.
pub fn ids(op: &Op) -> Vec<i64> {
    let rows = match op {
        Op::Reset(r) | Op::Insert { rows: r, .. } => r.clone(),
        Op::Update { row, .. } => vec![row.clone()],
        Op::Remove { .. } => vec![],
    };
    rows.iter()
        .map(|r| match r {
            Row::Message(m) => m.id,
            _ => panic!("not a message row"),
        })
        .collect()
}

pub fn sends(outs: &[Output]) -> usize {
    outs.iter().filter(|o| matches!(o, Output::Send(_))).count()
}

pub fn info(key: &str) -> AccountInfo {
    AccountInfo {
        key: key.into(),
        name: key.to_uppercase(),
        label: String::new(),
        email: format!("{key}@example.org"),
        status: AccountStatus::Ready,
        unread: 0,
        broken: false,
        oauth: false,
    }
}

pub fn folder(id: i64, role: FolderRole) -> FolderRow {
    FolderRow {
        id: FolderId(id),
        path: "INBOX".into(),
        name: "Inbox".into(),
        role,
        unread: 0,
    }
}

/// Open `list` and answer its first page.
pub fn opened(ui: &mut Ui, list: ListRef, rows: Vec<MessageRow>, end: bool) -> Vec<Output> {
    let _ = ui.open_list(list);
    feed(ui, page(list, None, rows, end))
}

pub fn cursor(id: i64, date: i64) -> Cursor {
    Cursor {
        date,
        id: MessageRowId(id),
    }
}

pub const F1: ListRef = ListRef::Folder(FolderId(1));
