use std::collections::HashSet;

use envio_store::{
    Action, ActionKind, Cursor, Draft, Flags, Folder, FolderId, FolderRole, MessageRef,
    MessageRowId, ServerCopy,
};

use super::*;
use crate::api::view::MessageRow;

fn set(ids: &[&str]) -> HashSet<String> {
    ids.iter().map(|s| (*s).to_owned()).collect()
}

fn draft(account: &str, server: ServerCopy, updated: i64) -> Draft {
    Draft {
        account: account.into(),
        to: vec!["Bob <bob@example.org>".into()],
        subject: "Hi".into(),
        body: "\n  first line  \nsecond".into(),
        updated_at_ms: updated,
        server,
        ..Draft::default()
    }
}

fn copy(message_id: Option<&str>, pending: Option<&str>, uploaded: i64) -> ServerCopy {
    ServerCopy {
        message_id: message_id.map(str::to_owned),
        pending_id: pending.map(str::to_owned),
        uploaded_ms: uploaded,
        ..ServerCopy::default()
    }
}

fn rows_for(
    drafts: &[(String, Draft)],
    on_server: &[&str],
    leaving: &[&str],
    ids: &mut LocalIds,
) -> Vec<MessageRow> {
    local_rows(
        drafts,
        ("a", FolderId(7)),
        (&set(on_server), &set(leaving), &HashSet::new()),
        ids,
    )
}

fn one(d: Draft) -> Vec<(String, Draft)> {
    vec![("d1".into(), d)]
}

fn rows_of(d: Draft, on_server: &[&str], leaving: &[&str]) -> Vec<MessageRow> {
    rows_for(&one(d), on_server, leaving, &mut LocalIds::default())
}

#[test]
fn a_never_uploaded_draft_gives_a_row() {
    let rows = rows_of(draft("a", ServerCopy::default(), 5_000), &[], &[]);
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.local_draft.as_deref(), Some("d1"));
    assert_eq!(row.folder, FolderId(7));
    assert_eq!(row.account, "a");
    assert_eq!(row.from_address, "bob@example.org");
    assert_eq!(row.from, "To: Bob");
    assert_eq!(row.subject, "Hi");
    assert_eq!(row.preview, "first line");
    assert_eq!(row.date, Some(5));
    assert_eq!(
        row.cursor,
        Cursor {
            date: 5,
            id: row.id
        }
    );
    assert!(row.id.0 < 0);
    assert!(!row.has_attachments);
}

#[test]
fn a_draft_with_an_attachment_shows_the_mark() {
    let mut d = draft("a", ServerCopy::default(), 5_000);
    d.attachments = vec!["/x/a.txt".into()];
    assert!(rows_of(d, &[], &[])[0].has_attachments);
}

#[test]
fn a_draft_whose_message_id_is_on_the_server_gives_no_row() {
    let d = draft("a", copy(Some("<m@x>"), None, 5_000), 5_000);
    assert!(rows_of(d, &["<m@x>"], &[]).is_empty());
}

#[test]
fn a_draft_whose_pending_id_is_on_the_server_gives_no_row() {
    let d = draft("a", copy(None, Some("<p@x>"), 0), 5_000);
    assert!(rows_of(d, &["<p@x>"], &[]).is_empty());
}

#[test]
fn stray_whitespace_in_the_draft_ids_still_matches() {
    let d = draft("a", copy(Some(" <m@x>\n"), None, 5_000), 5_000);
    assert!(rows_of(d, &["<m@x>"], &[]).is_empty());
}

#[test]
fn a_current_draft_that_is_leaving_gives_no_row() {
    let d = draft("a", copy(Some("<m@x>"), None, 5_000), 5_000);
    assert!(rows_of(d, &[], &["<m@x>"]).is_empty());
}

#[test]
fn a_draft_with_newer_edits_that_is_leaving_keeps_its_row() {
    let d = draft("a", copy(Some("<m@x>"), None, 4_000), 5_000);
    assert_eq!(rows_of(d, &[], &["<m@x>"]).len(), 1);
}

#[test]
fn leaving_matches_the_message_id_only_not_the_pending_id() {
    let d = draft("a", copy(None, Some("<p@x>"), 5_000), 5_000);
    assert_eq!(rows_of(d, &[], &["<p@x>"]).len(), 1);
}

#[test]
fn a_refused_draft_gives_a_row() {
    let mut server = copy(None, None, 0);
    server.failures = 5;
    server.failed_ms = 5_000;
    assert_eq!(rows_of(draft("a", server, 5_000), &[], &[]).len(), 1);
}

#[test]
fn another_accounts_draft_gives_no_row() {
    let d = draft("b", ServerCopy::default(), 5_000);
    assert!(rows_of(d, &[], &[]).is_empty());
}

#[test]
fn local_ids_are_stable_and_never_reused() {
    let mut ids = LocalIds::default();
    let a = ids.id_of("d1");
    let b = ids.id_of("d2");
    assert_eq!(ids.id_of("d1"), a);
    assert_ne!(a, b);
    assert!(a.0 < 0 && b.0 < 0);
    assert_eq!(ids.draft_of(a), Some("d1"));
    assert_eq!(ids.draft_of(b), Some("d2"));
    assert_eq!(ids.draft_of(MessageRowId(3)), None);
    assert_eq!(ids.draft_of(MessageRowId(-99)), None);
}

#[test]
fn to_line_names_addresses_and_none() {
    let to = ["Bob <bob@example.org>".into(), "cy@example.org".into()];
    assert_eq!(to_line(&to), "To: Bob, cy@example.org");
    assert_eq!(to_line(&[]), "To: (no recipients)");
}

// ---- merge_page

fn row(date: i64, id: i64) -> MessageRow {
    MessageRow {
        id: MessageRowId(id),
        folder: FolderId(7),
        account: "a".into(),
        cursor: Cursor {
            date,
            id: MessageRowId(id),
        },
        has_attachments: false,
        from: String::new(),
        from_address: String::new(),
        subject: String::new(),
        preview: String::new(),
        date: Some(date),
        flags: Flags::empty(),
        local_draft: (id < 0).then(|| format!("d{id}")),
    }
}

fn keys(rows: &[MessageRow]) -> Vec<(i64, i64)> {
    rows.iter().map(|r| (r.cursor.date, r.id.0)).collect()
}

#[test]
fn a_local_row_inside_the_range_is_inserted_at_its_place() {
    let page = vec![row(30, 3), row(10, 1)];
    let merged = merge_page(page, vec![row(20, -1)], None, false);
    assert_eq!(keys(&merged), [(30, 3), (20, -1), (10, 1)]);
}

#[test]
fn a_local_row_older_than_the_last_cached_row_waits_for_the_next_page() {
    let page = vec![row(30, 3), row(20, 2)];
    let merged = merge_page(page, vec![row(10, -1)], None, false);
    assert_eq!(keys(&merged), [(30, 3), (20, 2)]);
}

#[test]
fn an_end_page_takes_every_remaining_local_row() {
    let page = vec![row(30, 3)];
    let merged = merge_page(page, vec![row(10, -1), row(5, -2)], None, true);
    assert_eq!(keys(&merged), [(30, 3), (10, -1), (5, -2)]);
}

#[test]
fn an_empty_end_page_still_shows_the_local_rows() {
    let merged = merge_page(Vec::new(), vec![row(10, -1)], None, true);
    assert_eq!(keys(&merged), [(10, -1)]);
}

#[test]
fn a_local_row_newer_than_after_is_left_out_of_a_later_page() {
    let after = Cursor {
        date: 20,
        id: MessageRowId(2),
    };
    let page = vec![row(15, 1)];
    let local = vec![row(25, -1), row(20, -2), row(10, -3)];
    let merged = merge_page(page, local, Some(after), true);
    // (20, -2) sorts after (20, 2): it belongs to this page.
    assert_eq!(keys(&merged), [(20, -2), (15, 1), (10, -3)]);
}

#[test]
fn equal_dates_are_ordered_by_id() {
    let page = vec![row(10, 5), row(10, 2)];
    let merged = merge_page(page, vec![row(10, -1), row(10, 3)], None, false);
    assert_eq!(keys(&merged), [(10, 5), (10, 3), (10, 2)]);
}

// ---- leaving

fn folders() -> Vec<Folder> {
    let f = |path: &str, role| Folder {
        id: FolderId(1),
        account: "a".into(),
        path: path.into(),
        role,
        sync: None,
    };
    vec![
        f("Drafts", FolderRole::Drafts),
        f("Trash", FolderRole::Trash),
        f("Archive", FolderRole::Archive),
        f("Other", FolderRole::Other),
        f("INBOX", FolderRole::Inbox),
    ]
}

fn action(account: &str, folder: &str, id: &str, kind: ActionKind) -> Action {
    Action {
        seq: 1,
        at_ms: 0,
        target: MessageRef {
            account: account.into(),
            folder: folder.into(),
            uidvalidity: 1,
            uid: 1,
            message_id: Some(id.into()),
        },
        kind,
    }
}

fn leaving_of(actions: &[Action]) -> HashSet<String> {
    leaving(actions.iter(), "a", &folders())
}

#[test]
fn a_pending_delete_or_move_to_trash_is_leaving_trimmed() {
    let to = |p: &str| ActionKind::Move { to: p.into() };
    let actions = [
        action("a", "Drafts", " <d@x>", ActionKind::Delete),
        action("a", "Drafts", "<t@x>", to("Trash")),
    ];
    assert_eq!(leaving_of(&actions), set(&["<d@x>", "<t@x>"]));
}

#[test]
fn other_actions_are_not_leaving() {
    let to = |p: &str| ActionKind::Move { to: p.into() };
    let flag = ActionKind::SetFlags {
        add: Flags::SEEN,
        remove: Flags::empty(),
    };
    let actions = [
        action("a", "Drafts", "<1@x>", to("Archive")),
        action("a", "Drafts", "<2@x>", to("Other")),
        action("a", "Drafts", "<3@x>", flag),
        action("a", "INBOX", "<4@x>", ActionKind::Delete),
        action("b", "Drafts", "<5@x>", ActionKind::Delete),
    ];
    assert!(leaving_of(&actions).is_empty());
}

// ---- recipients of a cached message

#[test]
fn a_recipient_list_splits_at_the_commas_that_end_an_address() {
    let list = r#""Doe, Jo" <jo@example.org>, x@example.org,Cy <cy@example.org>"#;
    assert_eq!(
        split_list(list),
        [
            r#""Doe, Jo" <jo@example.org>"#,
            "x@example.org",
            "Cy <cy@example.org>"
        ]
    );
    assert!(split_list("  ").is_empty());
}

#[test]
fn a_cached_message_shows_its_recipients_and_tints_by_the_first() {
    let (line, key) = recipients_of("bob@example.org, Cy <cy@example.org>");
    assert_eq!(line, "To: bob@example.org, Cy");
    assert_eq!(key, "bob@example.org");
    let (line, key) = recipients_of("");
    assert_eq!(line, "To: (no recipients)");
    assert_eq!(key, "");
}

fn with_server(server: ServerCopy, updated: i64) -> Draft {
    draft("a", server, updated)
}

#[test]
fn a_cached_row_of_an_earlier_copy_hides_the_draft() {
    let mut server = copy(Some("<b@x>"), None, 5_000);
    server.earlier = vec!["<a@x>".into()];
    assert!(rows_of(with_server(server, 5_000), &["<a@x>"], &[]).is_empty());
}

#[test]
fn an_earlier_copy_that_is_leaving_hides_the_draft() {
    let mut server = copy(Some("<b@x>"), None, 5_000);
    server.earlier = vec![" <a@x>\n".into()];
    assert!(rows_of(with_server(server, 5_000), &[], &["<a@x>"]).is_empty());
}

#[test]
fn the_import_original_does_not_hide_a_detached_copy() {
    let mut server = copy(None, None, 0);
    server.imported_from = Some("<o@x>".into());
    server.detached = true;
    assert_eq!(
        rows_of(with_server(server, 5_000), &["<o@x>"], &[]).len(),
        1
    );
}

#[test]
fn an_incomplete_import_gives_no_row() {
    let mut server = copy(None, None, 0);
    server.incomplete = true;
    assert!(rows_of(with_server(server, 5_000), &[], &[]).is_empty());
}

#[test]
fn an_open_draft_that_is_leaving_keeps_its_row() {
    let d = draft("a", copy(Some("<m@x>"), None, 5_000), 5_000);
    let rows = local_rows(
        &one(d),
        ("a", FolderId(7)),
        (&set(&[]), &set(&["<m@x>"]), &set(&["d1"])),
        &mut LocalIds::default(),
    );
    assert_eq!(rows.len(), 1);
}

#[test]
fn a_cached_name_with_a_comma_stays_one_recipient() {
    // `Address::display()` stores names unquoted.
    assert_eq!(
        split_list("Doe, Jo <jo@example.org>, x@example.org"),
        ["Doe, Jo <jo@example.org>", "x@example.org"]
    );
    let (line, key) = recipients_of("Doe, Jo <jo@example.org>, Cy <cy@example.org>");
    assert_eq!(line, "To: Doe, Jo, Cy");
    assert_eq!(key, "jo@example.org");
}

#[test]
fn a_cached_name_that_looks_like_an_address_stays_with_its_address() {
    assert_eq!(
        split_list(r#""jo@x, Inc" <a@b.org>, c@d.org"#),
        [r#""jo@x, Inc" <a@b.org>"#, "c@d.org"]
    );
    assert_eq!(
        split_list(r#""A \", B" <a@b.org>, "C, D" <c@d.org>"#),
        [r#""A \", B" <a@b.org>"#, r#""C, D" <c@d.org>"#]
    );
    let (line, key) = recipients_of(r#""jo@x, Inc" <a@b.org>, c@d.org"#);
    assert_eq!(line, "To: a@b.org, c@d.org");
    assert_eq!(key, "a@b.org");
}

#[test]
fn a_quote_in_a_cached_name_does_not_swallow_the_commas() {
    assert_eq!(
        split_list(r#"O"Neil, Pat <p@example.org>, q@example.org"#),
        [r#"O"Neil, Pat <p@example.org>"#, "q@example.org"]
    );
    assert_eq!(
        split_list(r#"A\"B <a@example.org>, c@example.org"#),
        [r#"A\"B <a@example.org>"#, "c@example.org"]
    );
}
