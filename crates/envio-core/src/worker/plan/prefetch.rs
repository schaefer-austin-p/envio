//! Downloading recent Inbox bodies before they are opened (#102).

use super::{Env, Step};

/// How far back a message is worth its body: inside the shortest sync
/// window the body sweep keeps.
pub const PREFETCH_DAYS: u32 = 7;
/// Bigger messages wait for the open (RFC822.SIZE, bytes).
pub const PREFETCH_MAX_SIZE: u32 = 256 * 1024;
/// Bodies one step downloads: a message the user opens meanwhile waits for
/// at most this many.
pub const PREFETCH_BATCH: u32 = 10;

/// The prefetch step, if one is due: in the foreground, the account not
/// paused, storage not full, and the Inbox has bodies left.
pub(super) fn step(env: &Env) -> Option<Step> {
    let folder = env.prefetch?;
    (env.foreground && !env.paused && !env.disk_full).then_some(Step::Prefetch(folder))
}

#[cfg(test)]
mod tests {
    use super::super::{Wants, steps};
    use super::*;
    use envio_store::{Folder, FolderId, FolderRole, MessageRowId};

    fn folders() -> Vec<Folder> {
        let folder = |id, path: &str, role| Folder {
            id: FolderId(id),
            account: "a".into(),
            path: path.into(),
            role,
            sync: None,
        };
        vec![
            folder(2, "INBOX", FolderRole::Inbox),
            folder(3, "Sent", FolderRole::Sent),
        ]
    }

    fn env() -> Env {
        Env {
            foreground: true,
            prefetch: Some(FolderId(2)),
            ..Env::default()
        }
    }

    #[test]
    fn it_alone_needs_no_folder_list() {
        assert_eq!(
            steps(&Wants::default(), &folders(), &env()),
            [Step::Prefetch(FolderId(2))]
        );
    }

    #[test]
    fn it_comes_after_everything_else() {
        let wants = Wants {
            full: true,
            bodies: vec![MessageRowId(7)],
            ..Wants::default()
        };
        let all = steps(&wants, &folders(), &env());
        assert_eq!(all.first(), Some(&Step::Folders));
        assert_eq!(all.last(), Some(&Step::Prefetch(FolderId(2))));
        let at = |wanted: &Step| all.iter().position(|s| s == wanted).unwrap();
        for before in [
            Step::Body(MessageRowId(7)),
            Step::Replay,
            Step::Send,
            Step::File,
            Step::Folder(FolderId(2)),
        ] {
            assert!(at(&before) < at(&Step::Prefetch(FolderId(2))), "{before:?}");
        }
    }

    #[test]
    fn a_body_asked_for_meanwhile_still_comes_first() {
        let wants = Wants {
            bodies: vec![MessageRowId(7)],
            ..Wants::default()
        };
        assert_eq!(
            steps(&wants, &folders(), &env()),
            [
                Step::Folders,
                Step::Body(MessageRowId(7)),
                Step::Prefetch(FolderId(2))
            ]
        );
    }

    #[test]
    fn it_is_absent_when_paused_full_background_or_without_candidates() {
        for off in [
            Env {
                paused: true,
                ..env()
            },
            Env {
                disk_full: true,
                ..env()
            },
            Env {
                foreground: false,
                ..env()
            },
            Env {
                prefetch: None,
                ..env()
            },
        ] {
            assert!(steps(&Wants::default(), &folders(), &off).is_empty());
        }
    }
}
