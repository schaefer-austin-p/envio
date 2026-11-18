//! What an account's run does and when: pure, so the timing is tested
//! without a server.

use std::collections::BTreeSet;

use envio_net::imap::IDLE_MAX;
use envio_store::{Folder, FolderId, FolderRole, MessageRowId};
use envio_sync::plan::folders::sync_order;

use crate::problem::Problem;

mod prefetch;

pub use prefetch::step as prefetch_step;
pub use prefetch::{PREFETCH_BATCH, PREFETCH_DAYS, PREFETCH_MAX_SIZE};

pub const POLL_MS: i64 = envio_sync::POLL_MS;

// The renewal is `min(IDLE_MAX, POLL_MS)`: the poll interval must stay the
// shorter, or the 29 minute bound would be the one that applies.
const _: () = assert!(POLL_MS <= IDLE_MAX.as_millis() as i64);

/// What an account has been asked for since it last ran.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Wants {
    /// Everything: the folder list, actions, mail, every folder.
    pub full: bool,
    pub folders: BTreeSet<FolderId>,
    pub bodies: Vec<MessageRowId>,
    /// Bodies in `bodies` that must come whole, however big (a forward
    /// carries the attachments).
    pub whole: BTreeSet<MessageRowId>,
    /// Attachments of big messages (stored without them) the user asked to
    /// save: fetched from the server one by one.
    pub parts: Vec<PartWant>,
    /// Folders the user asked a batch of older mail for.
    pub older: BTreeSet<FolderId>,
    /// Server searches the user waits for: folder and query.
    pub searches: Vec<(FolderId, String)>,
    pub replay: bool,
    pub send: bool,
    pub file: bool,
    /// Upload changed drafts to the Drafts folder and delete the server
    /// copies of sent or discarded ones.
    pub drafts: bool,
}

/// One attachment of a partial message the user asked to save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartWant {
    pub id: MessageRowId,
    /// The attachment's index in the message's list.
    pub index: usize,
    /// Its IMAP section and decoded size, from the stub.
    pub section: String,
    pub size: usize,
    /// The file name to write it under.
    pub name: String,
    /// Where to write it.
    pub dir: std::path::PathBuf,
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
    /// One attachment (by index) of a message stored without its
    /// attachments, fetched to be saved.
    Part(MessageRowId, usize),
    /// One batch of older mail ("Load older") in a folder.
    Older(FolderId),
    /// "Search on server" in one folder (the account is the step's).
    SearchServer {
        folder: FolderId,
        query: String,
    },
    Replay,
    Send,
    File,
    /// Drafts to and from the server's Drafts folder.
    Drafts,
    Folder(FolderId),
    /// A batch of recent Inbox bodies, downloaded before they are opened.
    Prefetch(FolderId),
}

impl Step {
    /// The step's kind for the backoff: which sort of work keeps failing.
    pub fn kind(&self) -> &'static str {
        match self {
            Step::Folders => "folders",
            Step::Body(_) => "body",
            Step::Part(..) => "part",
            Step::Older(_) => "older",
            Step::SearchServer { .. } => "search",
            Step::Replay => "replay",
            Step::Send => "send",
            Step::File => "file",
            Step::Drafts => "drafts",
            Step::Folder(_) => "folder",
            Step::Prefetch(_) => "prefetch",
        }
    }
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
            self.drafts = true;
            self.folders.extend(folders.iter().map(|f| f.id));
        }
    }
}

/// What the account is in, beside what it wants.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Env {
    pub disk_full: bool,
    /// Sent was refused last time: keep the folder list even for a run that
    /// only sends.
    pub relist: bool,
    pub foreground: bool,
    pub paused: bool,
    /// The folder whose bodies are worth downloading, if it has some left.
    pub prefetch: Option<FolderId>,
}

/// The steps for `wants`, in order. `full` stands for the folder list,
/// actions, mail and every folder. With `disk_full` nothing is downloaded.
/// `relist` (Sent was refused last time) keeps the folder list even for a
/// run that only sends.
pub fn steps(wants: &Wants, folders: &[Folder], env: &Env) -> Vec<Step> {
    let (disk_full, relist) = (env.disk_full, env.relist);
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
        out.extend(wants.parts.iter().map(|p| Step::Part(p.id, p.index)));
        out.extend(wants.older.iter().map(|id| Step::Older(*id)));
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
    // After the folders: a draft that cannot be uploaded never holds up
    // the Inbox.
    if wants.full || wants.drafts {
        out.push(Step::Drafts);
    }
    // A LIST starts every run, so a run that only sends or syncs drafts
    // (the folder it needs is known) goes without it.
    let known = |role| folders.iter().any(|f| f.role == role);
    let quick = out.iter().all(|step| match step {
        Step::Send => known(FolderRole::Sent),
        Step::Drafts => known(FolderRole::Drafts),
        _ => false,
    });
    if !out.is_empty() && !(quick && !relist) {
        out.insert(0, Step::Folders);
    }
    // Last: nothing the user or the sync waits for is held up by it. It
    // needs no folder list, the Inbox is known.
    out.extend(prefetch::step(env));
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
    /// The account raises alerts: in the background it keeps Inbox IDLE.
    pub notifies: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    Run,
    Idle { until_ms: i64 },
    Sleep { until_ms: Option<i64> },
}

/// What an account does now. When a full run is due (`Run` with nothing
/// wanted) the caller sets `wants.full`. In the background only an account
/// that notifies works, and only on its Inbox: it does not run full polls,
/// it waits in IDLE, and `has_wants` then means the Inbox changed. Each
/// IDLE ends after at most `POLL_MS` so the next SELECT can find mail from
/// the gap before IDLE (#48).
pub fn next(timing: &Timing, now_ms: i64) -> Next {
    if timing.paused || !(timing.foreground || timing.notifies) {
        return Next::Sleep { until_ms: None };
    }
    if !timing.can_run {
        return Next::Sleep {
            until_ms: timing.retry_at_ms,
        };
    }
    if timing.has_wants || (timing.foreground && now_ms >= timing.next_full_ms) {
        return Next::Run;
    }
    if timing.idle_capable {
        // Mail that lands between the SELECT and the IDLE command is not
        // reported by the server (#48). Ending IDLE every `POLL_MS` re-SELECTs
        // the Inbox, and `changed_since_sync` then finds it, so the gap costs
        // at most one poll even in the background.
        let max = i64::try_from(IDLE_MAX.as_millis()).unwrap_or(i64::MAX);
        let renew = now_ms.saturating_add(max.min(POLL_MS));
        return Next::Idle {
            until_ms: if timing.foreground {
                timing.next_full_ms.min(renew)
            } else {
                renew
            },
        };
    }
    Next::Sleep {
        until_ms: timing.foreground.then_some(timing.next_full_ms),
    }
}

/// Whether the account may be taken into IDLE: its server is not known to
/// lack it, and there is an Inbox. Unknown (never connected) is tried.
pub fn may_idle(capable: Option<bool>, has_inbox: bool) -> bool {
    capable != Some(false) && has_inbox
}

/// A refused Inbox SELECT before IDLE in the background is retried later
/// with backoff, like any other failure: nobody is looking to be told.
pub fn idle_problem(problem: Problem, foreground: bool) -> Problem {
    match problem {
        Problem::Refused(_) if !foreground => Problem::Temporary,
        other => other,
    }
}

/// The steps of a background wake: the Inbox, if it is wanted, and nothing
/// else (no folder list, other folders, actions or bodies).
pub fn background_steps(wants: &Wants, folders: &[Folder]) -> Vec<Step> {
    folders
        .iter()
        .find(|f| f.role == FolderRole::Inbox && wants.folders.contains(&f.id))
        .map(|f| Step::Folder(f.id))
        .into_iter()
        .collect()
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

    fn env() -> Env {
        Env {
            foreground: true,
            ..Env::default()
        }
    }

    fn env_with(change: impl FnOnce(&mut Env)) -> Env {
        let mut env = env();
        change(&mut env);
        env
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
            steps(&wants, &folders(), &env()),
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
            steps(&wants, &folders(), &env()),
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
    fn load_older_runs_after_bodies_and_before_replay() {
        let wants = Wants {
            bodies: vec![MessageRowId(7)],
            older: [FolderId(2)].into(),
            replay: true,
            ..Wants::default()
        };
        assert_eq!(
            steps(&wants, &folders(), &env()),
            [
                Step::Folders,
                Step::Body(MessageRowId(7)),
                Step::Older(FolderId(2)),
                Step::Replay,
            ]
        );
        // Nothing is downloaded while storage is full.
        assert!(
            !steps(&wants, &folders(), &env_with(|e| e.disk_full = true))
                .contains(&Step::Older(FolderId(2)))
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
            steps(&wants, &folders(), &env()),
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
        let all = steps(&wants, &folders(), &env());
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
        assert_eq!(steps(&wants, &folders(), &env()), [Step::Send]);
        // Sent was refused last time: the list comes first again.
        assert_eq!(
            steps(&wants, &folders(), &env_with(|e| e.relist = true)),
            [Step::Folders, Step::Send]
        );
        // Folders not cached yet: the list comes first.
        assert_eq!(steps(&wants, &[], &env()), [Step::Folders, Step::Send]);
    }

    #[test]
    fn a_draft_run_skips_the_list_when_the_drafts_folder_is_known() {
        let wants = Wants {
            drafts: true,
            ..Wants::default()
        };
        let mut known = folders();
        known.push(folder(4, "Drafts", FolderRole::Drafts));
        assert_eq!(steps(&wants, &known, &env()), [Step::Drafts]);
        assert_eq!(
            steps(&wants, &folders(), &env()),
            [Step::Folders, Step::Drafts]
        );
    }

    #[test]
    fn nothing_wanted_is_no_steps() {
        assert!(steps(&Wants::default(), &folders(), &env()).is_empty());
    }

    #[test]
    fn drafts_come_after_every_folder_so_they_never_hold_up_the_inbox() {
        let wants = Wants {
            full: true,
            ..Wants::default()
        };
        let list = steps(&wants, &folders(), &env());
        let drafts = list.iter().position(|s| *s == Step::Drafts).unwrap();
        let last_folder = list
            .iter()
            .rposition(|s| matches!(s, Step::Folder(_)))
            .unwrap();
        assert!(drafts > last_folder, "{list:?}");
    }

    #[test]
    fn disk_full_drops_downloads_keeps_send_and_replay() {
        let wants = Wants {
            full: true,
            bodies: vec![MessageRowId(7)],
            ..Wants::default()
        };
        assert_eq!(
            steps(&wants, &folders(), &env_with(|e| e.disk_full = true)),
            [
                Step::Folders,
                Step::Replay,
                Step::Send,
                Step::File,
                Step::Drafts
            ]
        );
    }

    #[test]
    fn a_full_run_becomes_its_parts() {
        let mut wants = Wants {
            full: true,
            ..Wants::default()
        };
        wants.expand(&folders());
        assert!(!wants.full && wants.replay && wants.send && wants.file && wants.drafts);
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
            notifies: false,
        }
    }

    #[test]
    fn background_plans_inbox_idle_for_a_notifying_account() {
        let t = Timing {
            foreground: false,
            notifies: true,
            idle_capable: true,
            next_full_ms: 10,
            ..timing()
        };
        // A full run that is long overdue does not matter: no polling, but
        // IDLE ends every POLL_MS so mail from the SELECT-to-IDLE gap is
        // found by the next SELECT (#48).
        assert_eq!(
            next(&t, 1_000),
            Next::Idle {
                until_ms: 1_000 + POLL_MS
            }
        );
    }

    #[test]
    fn background_sleeps_for_an_account_with_notify_off() {
        let t = Timing {
            foreground: false,
            notifies: false,
            idle_capable: true,
            ..timing()
        };
        assert_eq!(next(&t, 0), Next::Sleep { until_ms: None });
    }

    #[test]
    fn background_without_idle_does_not_poll() {
        let t = Timing {
            foreground: false,
            notifies: true,
            next_full_ms: 10,
            ..timing()
        };
        assert_eq!(next(&t, 1_000), Next::Sleep { until_ms: None });
    }

    #[test]
    fn background_wake_plans_an_inbox_only_sync() {
        let wants = Wants {
            full: true,
            replay: true,
            send: true,
            file: true,
            folders: [FolderId(1), FolderId(2), FolderId(3)].into(),
            bodies: [MessageRowId(9)].into(),
            ..Wants::default()
        };
        assert_eq!(
            background_steps(&wants, &folders()),
            vec![Step::Folder(FolderId(2))]
        );
        let other = Wants {
            folders: [FolderId(1)].into(),
            ..Wants::default()
        };
        assert!(background_steps(&other, &folders()).is_empty());
    }

    #[test]
    fn a_refused_idle_select_is_retried_in_the_background_only() {
        use crate::problem::Problem;
        let refused = || Problem::Refused("NO".into());
        assert!(matches!(idle_problem(refused(), false), Problem::Temporary));
        assert!(matches!(idle_problem(refused(), true), Problem::Refused(_)));
        assert!(matches!(
            idle_problem(Problem::SignIn, false),
            Problem::SignIn
        ));
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
    fn idle_is_capped_at_the_poll_interval() {
        let t = Timing {
            idle_capable: true,
            next_full_ms: 10 * 3_600_000,
            ..timing()
        };
        assert_eq!(
            next(&t, 1_000),
            Next::Idle {
                until_ms: 1_000 + POLL_MS
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
