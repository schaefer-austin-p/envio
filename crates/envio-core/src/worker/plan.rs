//! What an account's run does and when: pure, so the timing is tested
//! without a server.

use std::collections::BTreeSet;

use envio_net::imap::IDLE_MAX;
use envio_store::{Folder, FolderId, FolderRole, MessageRowId};
use envio_sync::plan::folders::sync_order;

pub const POLL_MS: i64 = envio_sync::POLL_MS;

/// What an account has been asked for since it last ran.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Wants {
    /// Everything: the folder list, actions, mail, every folder.
    pub full: bool,
    pub folders: BTreeSet<FolderId>,
    pub bodies: Vec<MessageRowId>,
    /// Server searches the user waits for: folder and query.
    pub searches: Vec<(FolderId, String)>,
    pub replay: bool,
    pub send: bool,
    pub file: bool,
}

impl Wants {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Folders,
    Body(MessageRowId),
    /// "Search on server" in one folder (the account is the step's).
    SearchServer {
        folder: FolderId,
        query: String,
    },
    Replay,
    Send,
    File,
    Folder(FolderId),
}

impl Wants {
    /// After the folder list was fetched: a full run becomes its parts, so
    /// each can be done and cleared on its own.
    pub fn expand(&mut self, folders: &[Folder]) {
        if self.full {
            self.full = false;
            self.replay = true;
            self.send = true;
            self.file = true;
            self.folders.extend(folders.iter().map(|f| f.id));
        }
    }
}

/// The steps for `wants`, in order. `full` stands for the folder list,
/// actions, mail and every folder. With `disk_full` nothing is downloaded.
/// `relist` (Sent was refused last time) keeps the folder list even for a
/// run that only sends.
pub fn steps(wants: &Wants, folders: &[Folder], disk_full: bool, relist: bool) -> Vec<Step> {
    let mut out = Vec::new();
    // The user is waiting for these: they go first.
    out.extend(
        wants
            .searches
            .iter()
            .map(|(folder, query)| Step::SearchServer {
                folder: *folder,
                query: query.clone(),
            }),
    );
    if !disk_full {
        out.extend(wants.bodies.iter().map(|id| Step::Body(*id)));
    }
    if wants.full || wants.replay {
        out.push(Step::Replay);
    }
    if wants.full || wants.send {
        out.push(Step::Send);
    }
    if wants.full || wants.file {
        out.push(Step::File);
    }
    if !disk_full {
        out.extend(
            sync_order(folders)
                .into_iter()
                .filter(|id| wants.full || wants.folders.contains(id))
                .map(Step::Folder),
        );
    }
    // A LIST starts every run, so a run that only sends (the user pressed
    // Send; Sent is known) goes without it.
    let only_send = out.iter().all(|step| *step == Step::Send);
    let sent_known = folders.iter().any(|f| f.role == FolderRole::Sent);
    if !out.is_empty() && !(only_send && sent_known && !relist) {
        out.insert(0, Step::Folders);
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    pub foreground: bool,
    pub can_run: bool,
    pub paused: bool,
    pub retry_at_ms: Option<i64>,
    pub next_full_ms: i64,
    pub has_wants: bool,
    pub idle_capable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    Run,
    Idle { until_ms: i64 },
    Sleep { until_ms: Option<i64> },
}

/// What an account does now. When a full run is due (`Run` with nothing
/// wanted) the caller sets `wants.full`.
pub fn next(timing: &Timing, now_ms: i64) -> Next {
    if !timing.foreground || timing.paused {
        return Next::Sleep { until_ms: None };
    }
    if !timing.can_run {
        return Next::Sleep {
            until_ms: timing.retry_at_ms,
        };
    }
    if timing.has_wants || now_ms >= timing.next_full_ms {
        return Next::Run;
    }
    if timing.idle_capable {
        let max = i64::try_from(IDLE_MAX.as_millis()).unwrap_or(i64::MAX);
        return Next::Idle {
            until_ms: timing.next_full_ms.min(now_ms.saturating_add(max)),
        };
    }
    Next::Sleep {
        until_ms: Some(timing.next_full_ms),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use envio_store::FolderRole;

    fn folder(id: i64, path: &str, role: FolderRole) -> Folder {
        Folder {
            id: FolderId(id),
            account: "a".into(),
            path: path.into(),
            role,
            sync: None,
        }
    }

    fn folders() -> Vec<Folder> {
        vec![
            folder(1, "Archive", FolderRole::Archive),
            folder(2, "INBOX", FolderRole::Inbox),
            folder(3, "Sent", FolderRole::Sent),
        ]
    }

    #[test]
    fn wanted_work_runs_before_idle() {
        let t = Timing {
            idle_capable: true,
            has_wants: true,
            ..timing()
        };
        assert_eq!(next(&t, 0), Next::Run);
    }

    #[test]
    fn folders_come_before_replay_and_filing() {
        let wants = Wants {
            replay: true,
            file: true,
            send: true,
            ..Wants::default()
        };
        assert_eq!(
            steps(&wants, &folders(), false, false),
            [Step::Folders, Step::Replay, Step::Send, Step::File]
        );
    }

    #[test]
    fn a_body_request_comes_right_after_folders() {
        let wants = Wants {
            bodies: vec![MessageRowId(7), MessageRowId(8)],
            replay: true,
            folders: [FolderId(1)].into(),
            ..Wants::default()
        };
        assert_eq!(
            steps(&wants, &folders(), false, false),
            [
                Step::Folders,
                Step::Body(MessageRowId(7)),
                Step::Body(MessageRowId(8)),
                Step::Replay,
                Step::Folder(FolderId(1)),
            ]
        );
    }

    #[test]
    fn a_server_search_runs_before_downloads() {
        let wants = Wants {
            bodies: vec![MessageRowId(7)],
            searches: vec![(FolderId(1), "zebra".into())],
            ..Wants::default()
        };
        assert_eq!(
            steps(&wants, &folders(), false, false),
            [
                Step::Folders,
                Step::SearchServer {
                    folder: FolderId(1),
                    query: "zebra".into()
                },
                Step::Body(MessageRowId(7)),
            ]
        );
    }

    #[test]
    fn inbox_first_and_no_duplicate_folders() {
        let wants = Wants {
            full: true,
            folders: [FolderId(1), FolderId(2)].into(),
            ..Wants::default()
        };
        let all = steps(&wants, &folders(), false, false);
        let synced: Vec<Step> = all
            .iter()
            .filter(|s| matches!(s, Step::Folder(_)))
            .cloned()
            .collect();
        assert_eq!(
            synced,
            [
                Step::Folder(FolderId(2)),
                Step::Folder(FolderId(3)),
                Step::Folder(FolderId(1))
            ]
        );
        assert_eq!(all[0], Step::Folders);
    }

    #[test]
    fn a_send_alone_skips_the_folder_list_when_sent_is_known() {
        let wants = Wants {
            send: true,
            ..Wants::default()
        };
        assert_eq!(steps(&wants, &folders(), false, false), [Step::Send]);
        // Sent was refused last time: the list comes first again.
        assert_eq!(
            steps(&wants, &folders(), false, true),
            [Step::Folders, Step::Send]
        );
        // Folders not cached yet: the list comes first.
        assert_eq!(
            steps(&wants, &[], false, false),
            [Step::Folders, Step::Send]
        );
    }

    #[test]
    fn nothing_wanted_is_no_steps() {
        assert!(steps(&Wants::default(), &folders(), false, false).is_empty());
    }

    #[test]
    fn disk_full_drops_downloads_keeps_send_and_replay() {
        let wants = Wants {
            full: true,
            bodies: vec![MessageRowId(7)],
            ..Wants::default()
        };
        assert_eq!(
            steps(&wants, &folders(), true, false),
            [Step::Folders, Step::Replay, Step::Send, Step::File]
        );
    }

    #[test]
    fn a_full_run_becomes_its_parts() {
        let mut wants = Wants {
            full: true,
            ..Wants::default()
        };
        wants.expand(&folders());
        assert!(!wants.full && wants.replay && wants.send && wants.file);
        assert_eq!(wants.folders.len(), 3);
    }

    fn timing() -> Timing {
        Timing {
            foreground: true,
            can_run: true,
            paused: false,
            retry_at_ms: None,
            next_full_ms: 10_000,
            has_wants: false,
            idle_capable: false,
        }
    }

    #[test]
    fn background_sleeps() {
        let t = Timing {
            foreground: false,
            has_wants: true,
            ..timing()
        };
        assert_eq!(next(&t, 0), Next::Sleep { until_ms: None });
    }

    #[test]
    fn paused_sleeps_without_a_wake() {
        let t = Timing {
            paused: true,
            can_run: false,
            retry_at_ms: Some(5),
            has_wants: true,
            ..timing()
        };
        assert_eq!(next(&t, 0), Next::Sleep { until_ms: None });
    }

    #[test]
    fn offline_waits_until_its_retry() {
        let t = Timing {
            can_run: false,
            retry_at_ms: Some(5_000),
            has_wants: true,
            ..timing()
        };
        assert_eq!(
            next(&t, 0),
            Next::Sleep {
                until_ms: Some(5_000)
            }
        );
    }

    #[test]
    fn idle_when_nothing_is_wanted() {
        let t = Timing {
            idle_capable: true,
            ..timing()
        };
        assert_eq!(next(&t, 0), Next::Idle { until_ms: 10_000 });
    }

    #[test]
    fn idle_is_capped_at_idle_max() {
        let t = Timing {
            idle_capable: true,
            next_full_ms: 10 * 3_600_000,
            ..timing()
        };
        let max = i64::try_from(envio_net::imap::IDLE_MAX.as_millis()).unwrap();
        assert_eq!(
            next(&t, 1_000),
            Next::Idle {
                until_ms: 1_000 + max
            }
        );
    }

    #[test]
    fn no_idle_sleeps_until_the_next_full_run() {
        assert_eq!(
            next(&timing(), 0),
            Next::Sleep {
                until_ms: Some(10_000)
            }
        );
    }

    #[test]
    fn a_due_full_run_runs() {
        assert_eq!(next(&timing(), 10_000), Next::Run);
        let t = Timing {
            has_wants: true,
            idle_capable: true,
            ..timing()
        };
        assert_eq!(next(&t, 0), Next::Run);
    }
}
