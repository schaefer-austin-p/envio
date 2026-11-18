//! Which drafts need their server copy written, replaced or deleted. Pure.

use envio_store::{Draft, ServerCopy};

/// Nothing typed, nothing attached: not worth a server copy.
pub fn is_blank(draft: &Draft) -> bool {
    draft.to.is_empty()
        && draft.cc.is_empty()
        && draft.bcc.is_empty()
        && draft.subject.trim().is_empty()
        && draft.body.trim().is_empty()
        && draft.attachments.is_empty()
}

/// Failures of one save after which it is left alone until it changes.
pub const GIVE_UP: u32 = 5;

/// The server's copy is not the saved text: compared by the save's stamp
/// being different, not older, so a clock that stepped back does not hide
/// later saves. A blank draft that was never uploaded stays local; one that
/// was is replaced, so the server never keeps text the user has since
/// removed. A save that failed `GIVE_UP` times waits for the next change.
pub fn needs_upload(draft: &Draft) -> bool {
    let server = &draft.server;
    let current = server.message_id.is_some() && server.uploaded_ms == draft.updated_at_ms;
    let given_up = server.failed_ms == draft.updated_at_ms && server.failures >= GIVE_UP;
    !current && !given_up && !(is_blank(draft) && server.message_id.is_none())
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
    fn a_never_uploaded_draft_needs_upload() {
        assert!(needs_upload(&draft(5, None, 0)));
    }

    #[test]
    fn an_uploaded_draft_needs_upload_only_after_a_newer_save() {
        assert!(!needs_upload(&draft(5, Some("<a@b>"), 5)));
        assert!(needs_upload(&draft(6, Some("<a@b>"), 5)));
    }

    #[test]
    fn a_save_after_the_clock_stepped_back_is_uploaded() {
        // Uploaded at 9, then saved again at 5: the stamps differ.
        assert!(needs_upload(&draft(5, Some("<a@b>"), 9)));
    }

    #[test]
    fn a_save_that_failed_repeatedly_waits_for_the_next_change() {
        let mut failing = draft(5, None, 0);
        failing.server.failed_ms = 5;
        failing.server.failures = GIVE_UP - 1;
        assert!(needs_upload(&failing));
        failing.server.failures = GIVE_UP;
        assert!(!needs_upload(&failing));
        failing.updated_at_ms = 6;
        assert!(needs_upload(&failing));
    }

    #[test]
    fn a_blank_draft_is_uploaded_only_to_replace_a_copy() {
        let mut blank = draft(5, None, 0);
        blank.body.clear();
        assert!(!needs_upload(&blank));
        blank.server.message_id = Some("<a@b>".into());
        assert!(needs_upload(&blank));
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
}
