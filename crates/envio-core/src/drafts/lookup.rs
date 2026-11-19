//! Which local draft belongs to a server message. Pure over a listing.

use envio_store::Draft;
use envio_sync::plan::message_id::usable_message_id;

/// The local draft whose server copy is, was or was imported from
/// `message_id` (see `ServerCopy::names`), in `account`. An unusable
/// Message-ID (empty, `<>`) matches nothing.
pub(crate) fn draft_for(
    drafts: &[(String, Draft)],
    account: &str,
    message_id: &str,
) -> Option<String> {
    if !usable_message_id(message_id) {
        return None;
    }
    drafts
        .iter()
        .find(|(_, d)| d.account == account && d.server.names(message_id))
        .map(|(id, _)| id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(account: &str, server: envio_store::ServerCopy) -> Draft {
        Draft {
            account: account.into(),
            server,
            ..Draft::default()
        }
    }

    fn listing() -> Vec<(String, Draft)> {
        vec![
            (
                "a".into(),
                draft(
                    "me",
                    envio_store::ServerCopy {
                        message_id: Some("<m1@example.org>".into()),
                        ..Default::default()
                    },
                ),
            ),
            (
                "b".into(),
                draft(
                    "me",
                    envio_store::ServerCopy {
                        pending_id: Some("<p1@example.org>".into()),
                        ..Default::default()
                    },
                ),
            ),
            (
                "c".into(),
                draft(
                    "me",
                    envio_store::ServerCopy {
                        imported_from: Some("<o1@example.org>".into()),
                        ..Default::default()
                    },
                ),
            ),
            (
                "d".into(),
                draft(
                    "other",
                    envio_store::ServerCopy {
                        message_id: Some("<m2@example.org>".into()),
                        ..Default::default()
                    },
                ),
            ),
            ("e".into(), draft("me", envio_store::ServerCopy::default())),
        ]
    }

    #[test]
    fn matches_message_id_pending_id_and_imported_from() {
        let l = listing();
        assert_eq!(
            draft_for(&l, "me", "<m1@example.org>").as_deref(),
            Some("a")
        );
        assert_eq!(
            draft_for(&l, "me", "<p1@example.org>").as_deref(),
            Some("b")
        );
        assert_eq!(
            draft_for(&l, "me", "<o1@example.org>").as_deref(),
            Some("c")
        );
    }

    #[test]
    fn matches_a_message_id_the_draft_had_two_uploads_ago() {
        let l = vec![(
            "g".into(),
            draft(
                "me",
                envio_store::ServerCopy {
                    message_id: Some("<now@example.org>".into()),
                    earlier: vec!["<old@example.org>".into(), "<mid@example.org>".into()],
                    ..Default::default()
                },
            ),
        )];
        assert_eq!(
            draft_for(&l, "me", "<old@example.org>").as_deref(),
            Some("g")
        );
        assert_eq!(
            draft_for(&l, "me", "<mid@example.org>").as_deref(),
            Some("g")
        );
        assert_eq!(draft_for(&l, "me", "<other@example.org>"), None);
    }

    #[test]
    fn ignores_surrounding_whitespace() {
        let l = listing();
        assert_eq!(
            draft_for(&l, "me", "  <m1@example.org>\r\n").as_deref(),
            Some("a")
        );
    }

    #[test]
    fn misses_another_account_and_unknown_ids() {
        let l = listing();
        assert_eq!(draft_for(&l, "me", "<m2@example.org>"), None);
        assert_eq!(draft_for(&l, "other", "<m1@example.org>"), None);
        assert_eq!(draft_for(&l, "me", "<nope@example.org>"), None);
    }

    #[test]
    fn an_unusable_id_never_matches() {
        let mut l = listing();
        l.push((
            "f".into(),
            draft(
                "me",
                envio_store::ServerCopy {
                    message_id: Some("<>".into()),
                    pending_id: Some(String::new()),
                    ..Default::default()
                },
            ),
        ));
        assert_eq!(draft_for(&l, "me", "<>"), None);
        assert_eq!(draft_for(&l, "me", ""), None);
        assert_eq!(draft_for(&l, "me", "  "), None);
    }
}
