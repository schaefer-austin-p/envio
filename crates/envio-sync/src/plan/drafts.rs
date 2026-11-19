//! Which drafts need their server copy written, replaced or deleted. Pure.

use std::collections::BTreeSet;

use envio_store::{Action, ActionKind, Draft, Folder, FolderRole, ServerCopy};

use super::refusal::retry_delay_ms;

/// Nothing typed, nothing attached: not worth a server copy.
pub fn is_blank(draft: &Draft) -> bool {
    draft.to.is_empty()
        && draft.cc.is_empty()
        && draft.bcc.is_empty()
        && draft.subject.trim().is_empty()
        && draft.body.trim().is_empty()
        && draft.attachments.is_empty()
}

/// The server's copy is not the saved text: compared by the save's stamp
/// being different, not older, so a clock that stepped back does not hide
/// later saves. A blank draft that has no server copy stays local; one that
/// has one (or a cut upload's copy, which may hold the removed text) is
/// replaced, so the server never keeps text the user has since removed. A
/// save that failed waits `retry_delay_ms` after its last failure (a new
/// save, a manual refresh, or a clock that stepped back ends the wait).
pub fn needs_upload(draft: &Draft, now_ms: i64) -> bool {
    let server = &draft.server;
    let current = server.message_id.is_some() && server.uploaded_ms == draft.updated_at_ms;
    let has_copy = server.message_id.is_some() || server.pending_id.is_some();
    !server.incomplete
        && !current
        && !waiting(server, draft.updated_at_ms, now_ms)
        && !(is_blank(draft) && !has_copy)
}

/// This save failed and its wait has not run out. `failed_at_ms` of 0 (a
/// refresh lifted the wait) or after `now_ms` (the clock stepped back) is
/// not waiting.
fn waiting(server: &ServerCopy, saved_ms: i64, now_ms: i64) -> bool {
    server.failed_ms == saved_ms
        && server.failures > 0
        && server.failed_at_ms > 0
        && server.failed_at_ms <= now_ms
        && now_ms - server.failed_at_ms < retry_delay_ms(server.failures)
}

/// A copy from an upload that was cut: it may hold older text, so it goes
/// before a new copy is appended (never kept as "the" copy).
pub fn cut_copy(server: &ServerCopy) -> Option<&str> {
    server.pending_id.as_deref()
}

/// The copy `new` replaces, once `new` is on the server.
pub fn replaced<'a>(server: &'a ServerCopy, new: &str) -> Option<&'a str> {
    server.message_id.as_deref().filter(|old| *old != new)
}

/// Whether the cache knows the folders an action names well enough for
/// `drafts_to_forget` to judge it (after a rebuilt cache it may not yet).
pub fn can_judge(action: &Action, folders: &[Folder]) -> bool {
    let known = |path: &str| folders.iter().any(|f| f.path == path);
    known(&action.target.folder)
        && match &action.kind {
            ActionKind::Move { to } => known(to),
            _ => true,
        }
}

/// The Message-ID of the Drafts-folder message an action removes for good
/// (a Delete, or a Move to Trash), or `None` for any other action. The one
/// test `drafts_to_forget` and the Drafts list's "leaving" rule share.
pub fn leaves_drafts<'a>(action: &'a Action, folders: &[Folder]) -> Option<&'a str> {
    let role_of = |path: &str| folders.iter().find(|f| f.path == path).map(|f| f.role);
    let leaves = match &action.kind {
        ActionKind::Delete => true,
        ActionKind::Move { to } => role_of(to) == Some(FolderRole::Trash),
        ActionKind::SetFlags { .. } => false,
    };
    let target = &action.target;
    if leaves && role_of(&target.folder) == Some(FolderRole::Drafts) {
        target.message_id.as_deref()
    } else {
        None
    }
}

/// Ids (with the Message-ID of the deleted copy) of the local drafts whose server copy the user has just deleted (or
/// moved to Trash) in the Drafts folder: their files must go, or Upload or
/// Edit would bring the draft back. `done` are actions the server has now
/// taken, so an Undo is over. A draft stays when its window is open, or
/// when it has a save the server copy lacks (unsent work wins; it is then
/// uploaded again).
pub fn drafts_to_forget(
    done: &[Action],
    folders: &[Folder],
    drafts: &[(String, Draft)],
    open: &BTreeSet<String>,
) -> Vec<(String, String)> {
    let mut ids = Vec::new();
    for action in done {
        let Some(message_id) = leaves_drafts(action, folders) else {
            continue;
        };
        let target = &action.target;
        for (id, draft) in drafts {
            let server = &draft.server;
            if draft.account == target.account
                && server.message_id.as_deref() == Some(message_id)
                && server.uploaded_ms == draft.updated_at_ms
                && !open.contains(id)
                && !ids.iter().any(|(known, _)| known == id)
            {
                ids.push((id.clone(), message_id.to_owned()));
            }
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(updated: i64, id: Option<&str>, uploaded: i64) -> Draft {
        Draft {
            body: "hi".into(),
            updated_at_ms: updated,
            server: ServerCopy {
                message_id: id.map(str::to_owned),
                pending_id: None,
                uploaded_ms: uploaded,
                ..ServerCopy::default()
            },
            ..Draft::default()
        }
    }

    #[test]
    fn an_incomplete_import_is_never_uploaded() {
        let mut half = draft(5, None, 0);
        assert!(needs_upload(&half, NOW));
        half.server.incomplete = true;
        assert!(!needs_upload(&half, NOW));
    }

    #[test]
    fn a_never_uploaded_draft_needs_upload() {
        assert!(needs_upload(&draft(5, None, 0), NOW));
    }

    #[test]
    fn an_uploaded_draft_needs_upload_only_after_a_newer_save() {
        assert!(!needs_upload(&draft(5, Some("<a@b>"), 5), NOW));
        assert!(needs_upload(&draft(6, Some("<a@b>"), 5), NOW));
    }

    #[test]
    fn a_save_after_the_clock_stepped_back_is_uploaded() {
        // Uploaded at 9, then saved again at 5: the stamps differ.
        assert!(needs_upload(&draft(5, Some("<a@b>"), 9), NOW));
    }

    const MIN: i64 = 60_000;
    const NOW: i64 = 2_000_000;

    fn failed(failures: u32, at_ms: i64) -> Draft {
        let mut failing = draft(5, None, 0);
        failing.server.failed_ms = 5;
        failing.server.failures = failures;
        failing.server.failed_at_ms = at_ms;
        failing
    }

    #[test]
    fn a_failed_save_waits_the_delay_and_is_then_due() {
        let at = 1_000_000;
        let one = failed(1, at);
        assert!(!needs_upload(&one, at));
        assert!(!needs_upload(&one, at + 5 * MIN - 1));
        assert!(needs_upload(&one, at + 5 * MIN));
        // The second failure waits twice as long, up to an hour.
        let two = failed(2, at);
        assert!(!needs_upload(&two, at + 10 * MIN - 1));
        assert!(needs_upload(&two, at + 10 * MIN));
        let many = failed(40, at);
        assert!(!needs_upload(&many, at + 60 * MIN - 1));
        assert!(needs_upload(&many, at + 60 * MIN));
    }

    #[test]
    fn a_failed_save_is_never_given_up_on() {
        let at = 1_000_000;
        assert!(needs_upload(&failed(1_000, at), at + 60 * MIN));
    }

    #[test]
    fn a_new_save_a_refresh_or_a_stepped_back_clock_ends_the_wait() {
        let at = 1_000_000;
        let mut one = failed(1, at);
        assert!(!needs_upload(&one, at + MIN));
        // A newer save.
        one.updated_at_ms = 6;
        assert!(needs_upload(&one, at + MIN));
        // A manual refresh zeroed the stamp.
        let mut cleared = failed(1, at);
        cleared.server.failed_at_ms = 0;
        assert!(needs_upload(&cleared, at + MIN));
        // The clock is before the failure.
        assert!(needs_upload(&failed(1, at), at - 1));
    }

    #[test]
    fn a_blank_draft_is_uploaded_only_to_replace_a_copy() {
        let mut blank = draft(5, None, 0);
        blank.body.clear();
        assert!(!needs_upload(&blank, NOW));
        blank.server.message_id = Some("<a@b>".into());
        assert!(needs_upload(&blank, NOW));
    }

    #[test]
    fn a_blank_draft_with_a_cut_copy_is_uploaded_to_replace_it() {
        let mut blank = draft(5, None, 0);
        blank.body.clear();
        blank.server.pending_id = Some("<cut@x>".into());
        assert!(needs_upload(&blank, NOW));
    }

    #[test]
    fn the_replaced_copy_is_the_old_one_only() {
        let server = ServerCopy {
            message_id: Some("<old@x>".into()),
            pending_id: Some("<cut@x>".into()),
            uploaded_ms: 1,
            ..ServerCopy::default()
        };
        assert_eq!(replaced(&server, "<new@x>"), Some("<old@x>"));
        assert_eq!(replaced(&server, "<old@x>"), None);
        assert_eq!(cut_copy(&server), Some("<cut@x>"));
    }
    fn folder(path: &str, role: FolderRole) -> Folder {
        Folder {
            id: envio_store::FolderId(1),
            account: "a".into(),
            path: path.into(),
            role,
            sync: None,
        }
    }

    fn folders() -> Vec<Folder> {
        vec![
            folder("Drafts", FolderRole::Drafts),
            folder("Trash", FolderRole::Trash),
            folder("Archive", FolderRole::Archive),
            folder("INBOX", FolderRole::Inbox),
        ]
    }

    fn action(folder: &str, message_id: Option<&str>, kind: ActionKind) -> Action {
        Action {
            seq: 1,
            at_ms: 0,
            target: envio_store::MessageRef {
                account: "a".into(),
                folder: folder.into(),
                uidvalidity: 1,
                uid: 1,
                message_id: message_id.map(str::to_owned),
            },
            kind,
        }
    }

    fn mine(updated: i64, uploaded: i64) -> Vec<(String, Draft)> {
        let mut d = draft(updated, Some("<m@x>"), uploaded);
        d.account = "a".into();
        vec![("d1".into(), d)]
    }

    fn forget(done: &[Action], drafts: &[(String, Draft)]) -> Vec<String> {
        drafts_to_forget(done, &folders(), drafts, &BTreeSet::new())
            .into_iter()
            .map(|(id, _)| id)
            .collect()
    }

    #[test]
    fn leaves_drafts_gives_the_id_of_a_delete_or_move_to_trash_only() {
        let f = folders();
        let to = |p: &str| ActionKind::Move { to: p.into() };
        let id = |a: &Action| leaves_drafts(a, &f).map(str::to_owned);
        let some = Some("<m@x>".to_owned());
        assert_eq!(
            id(&action("Drafts", Some("<m@x>"), ActionKind::Delete)),
            some
        );
        assert_eq!(id(&action("Drafts", Some("<m@x>"), to("Trash"))), some);
        assert_eq!(id(&action("Drafts", Some("<m@x>"), to("Archive"))), None);
        assert_eq!(id(&action("Drafts", Some("<m@x>"), to("Nowhere"))), None);
        let flag = ActionKind::SetFlags {
            add: envio_store::Flags::SEEN,
            remove: envio_store::Flags::empty(),
        };
        assert_eq!(id(&action("Drafts", Some("<m@x>"), flag)), None);
        assert_eq!(
            id(&action("INBOX", Some("<m@x>"), ActionKind::Delete)),
            None
        );
        assert_eq!(id(&action("Drafts", None, ActionKind::Delete)), None);
    }

    #[test]
    fn a_delete_in_drafts_forgets_the_draft_with_that_message_id() {
        let done = [action("Drafts", Some("<m@x>"), ActionKind::Delete)];
        assert_eq!(forget(&done, &mine(5, 5)), ["d1"]);
    }

    #[test]
    fn a_move_to_trash_forgets_it_but_a_move_elsewhere_does_not() {
        let to = |p: &str| ActionKind::Move { to: p.into() };
        let trash = [action("Drafts", Some("<m@x>"), to("Trash"))];
        assert_eq!(forget(&trash, &mine(5, 5)), ["d1"]);
        let archive = [action("Drafts", Some("<m@x>"), to("Archive"))];
        assert!(forget(&archive, &mine(5, 5)).is_empty());
    }

    #[test]
    fn only_a_drafts_folder_message_with_a_matching_id_counts() {
        let elsewhere = [action("INBOX", Some("<m@x>"), ActionKind::Delete)];
        assert!(forget(&elsewhere, &mine(5, 5)).is_empty());
        let other = [action("Drafts", Some("<o@x>"), ActionKind::Delete)];
        assert!(forget(&other, &mine(5, 5)).is_empty());
        let none = [action("Drafts", None, ActionKind::Delete)];
        assert!(forget(&none, &mine(5, 5)).is_empty());
        let flag = ActionKind::SetFlags {
            add: envio_store::Flags::SEEN,
            remove: envio_store::Flags::empty(),
        };
        assert!(forget(&[action("Drafts", Some("<m@x>"), flag)], &mine(5, 5)).is_empty());
    }

    #[test]
    fn a_draft_with_newer_unsent_edits_is_kept() {
        let done = [action("Drafts", Some("<m@x>"), ActionKind::Delete)];
        assert!(forget(&done, &mine(6, 5)).is_empty());
    }

    #[test]
    fn a_draft_of_another_account_with_the_same_message_id_is_kept() {
        let done = [action("Drafts", Some("<m@x>"), ActionKind::Delete)];
        let mut others = mine(5, 5);
        others[0].1.account = "b".into();
        assert!(forget(&done, &others).is_empty());
    }

    #[test]
    fn a_draft_in_an_open_window_is_kept() {
        let done = [action("Drafts", Some("<m@x>"), ActionKind::Delete)];
        let open = BTreeSet::from(["d1".to_owned()]);
        assert!(drafts_to_forget(&done, &folders(), &mine(5, 5), &open).is_empty());
    }
}
