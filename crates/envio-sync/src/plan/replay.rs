//! How a journal action goes to the server, and what its result means.
//! Pure.

use envio_net::NetError;
use envio_store::{Action, ActionKind, Flags, MessageRef};

use crate::error::{ErrorKind, net_kind};
use crate::plan::message_id::search_id;
use crate::schedule::{Key, Schedule, UNDO_MS};

/// The Undo window has passed and no retry wait is running.
pub fn is_due(action: &Action, schedule: &Schedule, now_ms: i64) -> bool {
    now_ms >= action.at_ms + UNDO_MS && schedule.is_due(&Key::Action(action.seq), now_ms)
}

/// How to find the action's message in its folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Find {
    /// Same UIDVALIDITY: the UID still names it.
    Uid(u32),
    /// UIDVALIDITY changed: search by Message-ID.
    ByMessageId(String),
    /// UIDVALIDITY changed and the message has no usable Message-ID: it can't be
    /// found, so the action is dropped.
    Lost,
}

pub fn find(target: &MessageRef, uid_validity: u32) -> Find {
    if target.uidvalidity == uid_validity {
        return Find::Uid(target.uid);
    }
    match target.message_id.as_deref().and_then(search_id) {
        Some(id) => Find::ByMessageId(id.to_owned()),
        None => Find::Lost,
    }
}

/// What to do on the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerOp {
    SetFlags {
        add: Flags,
        remove: Flags,
    },
    Move {
        to: String,
    },
    /// Delete for good (in Trash, or no Trash folder).
    Expunge,
}

/// Delete moves to Trash, except in Trash itself or when the account has
/// no Trash folder.
pub fn server_op(action: &Action, trash: Option<&str>) -> ServerOp {
    match &action.kind {
        ActionKind::SetFlags { add, remove } => ServerOp::SetFlags {
            add: *add,
            remove: *remove,
        },
        ActionKind::Move { to } => ServerOp::Move { to: to.clone() },
        ActionKind::Delete => match trash {
            Some(trash) if trash != action.target.folder => ServerOp::Move {
                to: trash.to_owned(),
            },
            _ => ServerOp::Expunge,
        },
    }
}

/// How a move runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveRoute {
    /// The server has MOVE: one atomic command.
    Direct,
    /// No MOVE: COPY then delete. A retry after a cut between the two would
    /// copy twice, so first look in the target for this Message-ID; if it's
    /// there, only delete the original.
    CheckTarget(String),
    /// No MOVE and no Message-ID to check by.
    Copy,
}

pub fn move_route(has_move: bool, message_id: Option<&str>) -> MoveRoute {
    if has_move {
        return MoveRoute::Direct;
    }
    match message_id.and_then(search_id) {
        Some(id) => MoveRoute::CheckTarget(id.to_owned()),
        None => MoveRoute::Copy,
    }
}

/// After this many failures in a row a Protocol-failing action is dropped.
pub const GIVE_UP_AFTER: u32 = 10;

/// An action the server keeps answering with garbage is given up on so it
/// doesn't block its message forever. Temporary failures never give up.
pub fn give_up(kind: ErrorKind, failures_so_far: u32) -> bool {
    kind == ErrorKind::Protocol && failures_so_far + 1 >= GIVE_UP_AFTER
}

/// What a server result means for the action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// It reached the server: mark it done.
    Done,
    /// Refused for good: mark it done and undo it locally, with a notice.
    Revert,
    /// Temporary, protocol or local failure: retry after a backoff, unless
    /// `give_up` says the ceiling is reached. The session is gone.
    Retry(ErrorKind),
    /// Sign-in or certificate: pause the account. The action stays pending.
    Stop(ErrorKind),
}

pub fn outcome(result: Result<(), &NetError>) -> Outcome {
    let Err(error) = result else {
        return Outcome::Done;
    };
    match net_kind(error) {
        ErrorKind::Refused => Outcome::Revert,
        kind @ (ErrorKind::Temporary | ErrorKind::Protocol | ErrorKind::Local) => {
            Outcome::Retry(kind)
        }
        kind @ (ErrorKind::SignIn | ErrorKind::Certificate) => Outcome::Stop(kind),
    }
}

#[cfg(test)]
mod tests {
    use envio_net::{Fingerprint, Untrusted};

    use super::*;

    fn action(folder: &str, kind: ActionKind) -> Action {
        Action {
            seq: 4,
            at_ms: 10_000,
            target: MessageRef {
                account: "1700".into(),
                folder: folder.into(),
                uidvalidity: 7,
                uid: 42,
                message_id: Some("<m@example.org>".into()),
            },
            kind,
        }
    }

    #[test]
    fn an_action_waits_out_the_undo_window_and_its_backoff() {
        let delete = action("INBOX", ActionKind::Delete);
        let mut schedule = Schedule::default();
        assert!(!is_due(&delete, &schedule, 14_999));
        assert!(is_due(&delete, &schedule, 15_000));
        let retry = schedule.failed(Key::Action(4), 15_000);
        assert!(!is_due(&delete, &schedule, retry - 1));
        assert!(is_due(&delete, &schedule, retry));
    }

    #[test]
    fn a_message_is_found_by_uid_or_after_a_reset_by_message_id() {
        let mut target = action("INBOX", ActionKind::Delete).target;
        assert_eq!(find(&target, 7), Find::Uid(42));
        assert_eq!(
            find(&target, 8),
            Find::ByMessageId("<m@example.org>".into())
        );
        target.message_id = None;
        assert_eq!(find(&target, 8), Find::Lost);
        target.message_id = Some("<>".into());
        assert_eq!(
            find(&target, 8),
            Find::Lost,
            "an unusable id matches anything"
        );
        assert_eq!(find(&target, 7), Find::Uid(42), "the uid needs no id");
    }

    #[test]
    fn delete_goes_to_trash_unless_it_is_in_trash_or_there_is_none() {
        let inbox = action("INBOX", ActionKind::Delete);
        assert_eq!(
            server_op(&inbox, Some("Trash")),
            ServerOp::Move { to: "Trash".into() }
        );
        assert_eq!(server_op(&inbox, None), ServerOp::Expunge);
        let trash = action("Trash", ActionKind::Delete);
        assert_eq!(server_op(&trash, Some("Trash")), ServerOp::Expunge);
        let flag = action(
            "INBOX",
            ActionKind::SetFlags {
                add: Flags::FLAGGED,
                remove: Flags::empty(),
            },
        );
        assert_eq!(
            server_op(&flag, Some("Trash")),
            ServerOp::SetFlags {
                add: Flags::FLAGGED,
                remove: Flags::empty()
            }
        );
    }

    #[test]
    fn without_move_the_target_is_checked_first() {
        assert_eq!(move_route(true, Some("<m@x>")), MoveRoute::Direct);
        assert_eq!(
            move_route(false, Some("<m@x>")),
            MoveRoute::CheckTarget("<m@x>".into())
        );
        assert_eq!(move_route(false, None), MoveRoute::Copy);
        assert_eq!(move_route(false, Some("<>")), MoveRoute::Copy);
    }

    #[test]
    fn results_map_to_the_spec_responses() {
        assert_eq!(outcome(Ok(())), Outcome::Done);
        assert_eq!(
            outcome(Err(&NetError::Refused("NO".into()))),
            Outcome::Revert
        );
        assert_eq!(
            outcome(Err(&NetError::Timeout)),
            Outcome::Retry(ErrorKind::Temporary)
        );
        assert_eq!(
            outcome(Err(&NetError::Protocol("?".into()))),
            Outcome::Retry(ErrorKind::Protocol)
        );
        assert_eq!(
            outcome(Err(&NetError::SignIn("no".into()))),
            Outcome::Stop(ErrorKind::SignIn)
        );
        assert_eq!(
            outcome(Err(&NetError::NoStartTls)),
            Outcome::Stop(ErrorKind::Certificate)
        );
    }

    #[test]
    fn sign_in_and_certificate_errors_stop_and_never_retry() {
        let fp = |byte| Fingerprint::from_bytes([byte; 32]);
        let cases = [
            (NetError::SignIn("no".into()), ErrorKind::SignIn),
            (
                NetError::Untrusted {
                    presented: fp(1),
                    reason: Untrusted::UnknownIssuer,
                },
                ErrorKind::Certificate,
            ),
            (
                NetError::PinChanged {
                    pinned: fp(1),
                    presented: fp(2),
                },
                ErrorKind::Certificate,
            ),
            (NetError::Tls("x".into()), ErrorKind::Certificate),
            (NetError::NoStartTls, ErrorKind::Certificate),
        ];
        for (error, kind) in cases {
            assert_eq!(outcome(Err(&error)), Outcome::Stop(kind), "{error:?}");
        }
    }

    #[test]
    fn give_up_only_for_protocol_at_the_ceiling() {
        assert!(!give_up(ErrorKind::Protocol, 8));
        assert!(give_up(ErrorKind::Protocol, 9));
        assert!(!give_up(ErrorKind::Temporary, 100));
    }

    #[test]
    fn busy_and_network_errors_retry() {
        let io = NetError::Io(std::io::Error::other("reset"));
        assert_eq!(outcome(Err(&io)), Outcome::Retry(ErrorKind::Temporary));
        assert_eq!(
            outcome(Err(&NetError::Busy("BYE".into()))),
            Outcome::Retry(ErrorKind::Temporary)
        );
    }
}
