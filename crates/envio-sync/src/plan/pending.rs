//! What the journal's pending actions mean for one folder while it syncs:
//! a pending flag change wins over the server's flags, and a message with
//! a pending Move or Delete stays out of the cache, so sync never undoes
//! what the user just did. Pure.

use std::collections::{BTreeMap, BTreeSet};

use envio_store::{Action, ActionKind, Flags};

#[derive(Debug, Default)]
pub struct Pending {
    /// (add, remove) per UID, oldest first.
    flags: BTreeMap<u32, Vec<(Flags, Flags)>>,
    hidden: BTreeSet<u32>,
    /// Every row with such an id is hidden, though replay acts on the first
    /// match only: which of two copies the user meant is not known. A
    /// second copy shows again once replay has settled the action.
    /// Message-IDs of Moves and Deletes journalled under an earlier
    /// UIDVALIDITY: their UIDs mean nothing now, the messages are still
    /// on their way out.
    hidden_ids: BTreeSet<String>,
}

impl Pending {
    /// The pending actions on messages in `path` as of `uidvalidity`.
    /// Actions from before a UIDVALIDITY change name other UIDs; they are
    /// found by Message-ID when replayed, and until then hide by Message-ID.
    pub fn of<'a>(
        actions: impl IntoIterator<Item = &'a Action>,
        account: &str,
        path: &str,
        uidvalidity: u32,
    ) -> Self {
        let mut pending = Self::default();
        let here = actions
            .into_iter()
            .filter(|action| action.target.account == account && action.target.folder == path);
        for action in here {
            if action.target.uidvalidity != uidvalidity {
                if let (ActionKind::Move { .. } | ActionKind::Delete, Some(id)) =
                    (&action.kind, &action.target.message_id)
                {
                    pending.hidden_ids.insert(id.clone());
                }
                continue;
            }
            let uid = action.target.uid;
            match &action.kind {
                ActionKind::SetFlags { add, remove } => {
                    pending.flags.entry(uid).or_default().push((*add, *remove));
                }
                ActionKind::Move { .. } | ActionKind::Delete => {
                    pending.hidden.insert(uid);
                }
            }
        }
        pending
    }

    /// The flags to cache for `uid`: the server's, with pending changes on
    /// top in the order the user made them.
    pub fn flags(&self, uid: u32, server: Flags) -> Flags {
        self.flags.get(&uid).map_or(server, |changes| {
            changes.iter().fold(server, |flags, (add, remove)| {
                flags.with(*add).without(*remove)
            })
        })
    }

    /// The message is on its way out of this folder.
    pub fn is_hidden(&self, uid: u32) -> bool {
        self.hidden.contains(&uid)
    }

    /// Like [`Self::is_hidden`] for a fetched row: also true when a Move or
    /// Delete journalled under an older UIDVALIDITY names its Message-ID.
    pub fn hides(&self, uid: u32, message_id: Option<&str>) -> bool {
        self.is_hidden(uid) || message_id.is_some_and(|id| self.hidden_ids.contains(id))
    }
}

#[cfg(test)]
mod tests {
    use envio_store::MessageRef;

    use super::*;

    fn action(folder: &str, uidvalidity: u32, uid: u32, kind: ActionKind) -> Action {
        Action {
            seq: 0,
            at_ms: 0,
            target: MessageRef {
                account: "1700".into(),
                folder: folder.into(),
                uidvalidity,
                uid,
                message_id: None,
            },
            kind,
        }
    }

    fn set(add: Flags, remove: Flags) -> ActionKind {
        ActionKind::SetFlags { add, remove }
    }

    #[test]
    fn pending_flag_changes_win_in_order() {
        let actions = [
            action("INBOX", 7, 3, set(Flags::SEEN, Flags::empty())),
            action("INBOX", 7, 3, set(Flags::FLAGGED, Flags::SEEN)),
            action("INBOX", 7, 4, ActionKind::Delete),
        ];
        let pending = Pending::of(&actions, "1700", "INBOX", 7);
        assert_eq!(
            pending.flags(3, Flags::ANSWERED),
            Flags::ANSWERED.with(Flags::FLAGGED)
        );
        assert_eq!(pending.flags(9, Flags::SEEN), Flags::SEEN);
        assert!(pending.is_hidden(4));
        assert!(!pending.is_hidden(3));
    }

    #[test]
    fn an_old_uidvalidity_move_or_delete_hides_by_message_id() {
        let mut old = action("INBOX", 6, 3, ActionKind::Delete);
        old.target.message_id = Some("<a@b>".into());
        let mut flag = action("INBOX", 6, 4, set(Flags::SEEN, Flags::empty()));
        flag.target.message_id = Some("<c@d>".into());
        let mut moved = action(
            "INBOX",
            5,
            8,
            ActionKind::Move {
                to: "Archive".into(),
            },
        );
        moved.target.message_id = Some("<m@v>".into());
        let actions = [old, flag, moved];
        let pending = Pending::of(&actions, "1700", "INBOX", 7);
        assert!(pending.hides(9, Some("<a@b>")));
        assert!(pending.hides(9, Some("<m@v>")));
        assert!(!pending.hides(9, Some("<c@d>")));
        assert!(!pending.hides(9, None));
        assert!(!pending.is_hidden(3));
    }

    #[test]
    fn actions_elsewhere_or_from_an_old_uidvalidity_do_not_count() {
        let actions = [
            action("Archive", 7, 3, ActionKind::Delete),
            action("INBOX", 6, 3, ActionKind::Delete),
            action(
                "INBOX",
                7,
                3,
                ActionKind::Move {
                    to: "Archive".into(),
                },
            ),
        ];
        let pending = Pending::of(&actions[..2], "1700", "INBOX", 7);
        assert!(!pending.is_hidden(3));
        assert!(!Pending::of(&actions, "1800", "INBOX", 7).is_hidden(3));
        assert!(Pending::of(&actions, "1700", "INBOX", 7).is_hidden(3));
    }
}
