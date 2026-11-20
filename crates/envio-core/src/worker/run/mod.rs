//! Running one step of one account, and what a failure does to the account.

mod mail;
mod prefetch;
mod sync;

use envio_net::imap::ImapSession;
use envio_store::FolderId;

use super::Worker;
use super::conn::Conn;
use super::plan::{POLL_MS, Step};
use crate::api::event::Event;
use crate::clock::now_ms;
use crate::health::Response;
use crate::problem::Problem;
use crate::text;

/// Runs `step`, then records the outcome in the account's health and wants.
pub(crate) async fn run_one(worker: &mut Worker<'_>, account: &str, step: Step) {
    let before = worker.status_of(account);
    if let Some(live) = worker.live.get_mut(account)
        && live.health.status == crate::health::AccountStatus::Ready
    {
        live.health.status = crate::health::AccountStatus::Syncing;
    }
    match run_step(worker, account, &step).await {
        Ok(()) => succeeded(worker, account, &step),
        Err(problem) => step_failed(worker, account, &step, &problem),
    }
    worker.report(account, &before);
}

/// What a failed step does: the account's health, the notices, the rebuild.
fn step_failed(worker: &mut Worker<'_>, account: &str, step: &Step, problem: &Problem) {
    // A part of a big folder that failed is not continued first.
    if let Some(live) = worker.live.get_mut(account) {
        live.continuing = None;
    }
    let response = failed(worker, account, step.kind(), problem);
    search_failed(worker, account, step, problem);
    older_failed(worker, account, step, problem);
    if matches!(
        response,
        Response::ItemError | Response::StopDownloads | Response::Rebuild
    ) {
        // Waiting would only repeat it; the next full run tries again.
        finished(worker, account, step);
    }
    if !matches!(problem, Problem::Refused(_))
        || matches!(step, Step::Body(_) | Step::Part(..) | Step::Keywords(_))
    {
        worker.open_failed(Some(account));
    }
    // Prefetch is background work nobody asked for: its failure is no news.
    if let Problem::Refused(reason) = problem
        && !reason.is_empty()
        && !matches!(step, Step::Prefetch(_))
    {
        worker.emit(Event::Notice {
            text: text::notice::refused(reason),
        });
    }
    if response == Response::Rebuild {
        rebuild(worker);
    }
    if response == Response::StopDownloads {
        worker.disk_full_until = Some(now_ms() + POLL_MS);
        worker.emit(Event::Notice {
            text: text::notice::disk_full(),
        });
    }
}

/// The user waits for a server search: no retry, and one unreachable
/// account must not hold the others back. A step before the search (the
/// folder list) that can't connect fails it too.
fn search_failed(worker: &mut Worker<'_>, account: &str, step: &Step, problem: &Problem) {
    let own = matches!(step, Step::SearchServer { .. });
    if let Some(query) = worker.searching(account)
        && (own || !matches!(problem, Problem::Refused(_)))
    {
        if own {
            finished(worker, account, step);
        }
        worker.search_step_failed(account, &query, problem);
    }
}

/// The user waits for a batch of older mail: no retry. A step before it
/// (the folder list) that can't connect fails it too.
fn older_failed(worker: &mut Worker<'_>, account: &str, step: &Step, problem: &Problem) {
    let own = matches!(step, Step::Older(_));
    if worker.older_waiting(account) && (own || !matches!(problem, Problem::Refused(_))) {
        if own {
            finished(worker, account, step);
        }
        worker.older_account_failed(account);
    }
}

pub(crate) async fn run_step(
    worker: &mut Worker<'_>,
    account: &str,
    step: &Step,
) -> Result<(), Problem> {
    if *step == Step::Send {
        return mail::send(worker, account).await;
    }
    let mut session = worker.imap(account).await?;
    let result = match step {
        Step::Folders => sync::folders(worker, account, &mut session).await,
        Step::Body(id) => sync::body(worker, &mut session, *id).await,
        Step::Part(id, index) => sync::part(worker, &mut session, *id, *index).await,
        Step::Keywords(id) => sync::keywords(worker, &mut session, *id).await,
        Step::Older(id) => worker.older_step(&mut session, *id).await,
        Step::SearchServer { folder, query } => {
            sync::search_server(worker, account, &mut session, (*folder, query)).await
        }
        Step::Replay => sync::replay(worker, account, &mut session).await,
        Step::File => mail::file(worker, account, &mut session).await,
        Step::Drafts => mail::drafts(worker, account, &mut session).await,
        Step::Folder(id) => sync::folder(worker, account, &mut session, *id).await,
        Step::Prefetch(id) => prefetch::run(worker, account, &mut session, *id).await,
        Step::Send => Ok(()),
    };
    keep_after(worker, (account, step), session, &result);
    result
}

/// Keeps the session after a step unless the step broke it.
fn keep_after(
    worker: &mut Worker<'_>,
    (account, step): (&str, &Step),
    mut session: ImapSession,
    result: &Result<(), Problem>,
) {
    if !matches!(result, Err(problem) if !matches!(problem, Problem::Refused(_))) {
        let changed = changed_folder(step, &mut session);
        let open = worker.live.get(account).is_some_and(|live| {
            keep_open(
                worker.foreground,
                worker.notifies(account),
                step,
                &live.wants,
            )
        });
        keep(worker, account, session, (changed, open));
    }
}

/// The folder to sync again: the step's, if the server reported a change.
fn changed_folder(step: &Step, session: &mut ImapSession) -> Option<FolderId> {
    match step {
        Step::Folder(id) if session.take_changes() => Some(*id),
        _ => None,
    }
}

/// A session lives on unless the step broke it (a refusal doesn't). If the
/// server reported a change while the step's folder was selected, that
/// folder is synced again.
fn keep(
    worker: &mut Worker<'_>,
    account: &str,
    session: ImapSession,
    (changed, open): (Option<FolderId>, bool),
) {
    if let Some(live) = worker.live.get_mut(account) {
        if let Some(id) = changed {
            live.wants.folders.insert(id);
        }
        live.conn = if open {
            Conn::Ready(session)
        } else {
            Conn::Closed
        };
    }
}

/// Whether the session stays open after `step`. In the foreground always. In
/// the background only bodies the reader waits for are fetched: the session
/// stays while another one is wanted (a burst shares one connection) and
/// closes with the last. An account that notifies also keeps it after its
/// Inbox sync or a body, so IDLE goes on without a new connection.
fn keep_open(foreground: bool, notifies: bool, step: &Step, wants: &super::plan::Wants) -> bool {
    foreground
        || (notifies && matches!(step, Step::Folder(_) | Step::Body(_)))
        || matches!(step, Step::Body(id) if wants.bodies.iter().any(|other| other != id))
}

fn succeeded(worker: &mut Worker<'_>, account: &str, step: &Step) {
    finished(worker, account, step);
    if let Step::Body(id) = step {
        worker.ask_whole_again(*id);
    }
    let disk_full = worker.disk_full(now_ms());
    if let Some(live) = worker.live.get_mut(account) {
        live.health.ok_step(step.kind(), disk_full);
        if live.wants.is_empty() {
            live.listed = false;
        } else if live.health.status == crate::health::AccountStatus::Ready {
            live.health.status = crate::health::AccountStatus::Syncing;
        }
    }
}

/// The step's want is met (or given up): clear it.
fn finished(worker: &mut Worker<'_>, account: &str, step: &Step) {
    let folders = worker.store.db.folders(account).unwrap_or_default();
    let Some(live) = worker.live.get_mut(account) else {
        return;
    };
    match step {
        Step::Folders => {
            live.listed = true;
            live.relist = false;
            live.wants.expand(&folders);
        }
        Step::Body(id) => {
            live.wants.bodies.retain(|b| b != id);
            live.wants.whole.remove(id);
        }
        Step::Keywords(id) => live.wants.keywords.retain(|k| k != id),
        Step::Part(id, index) => live
            .wants
            .parts
            .retain(|p| !(p.id == *id && p.index == *index)),
        Step::Older(id) => {
            live.wants.older.remove(id);
        }
        Step::SearchServer { folder, query } => live
            .wants
            .searches
            .retain(|(f, q)| !(f == folder && q == query)),
        Step::Replay => live.wants.replay = false,
        Step::Send => live.wants.send = false,
        Step::File => live.wants.file = false,
        Step::Drafts => live.wants.drafts = false,
        Step::Prefetch(_) => {}
        Step::Folder(id) => {
            // Parts remain: the same folder runs again, ahead of the others.
            if live.continuing != Some(*id) {
                live.wants.folders.remove(id);
            }
        }
    }
}

/// Applies a problem to the account's health. Any problem but a refusal
/// loses the connection.
pub(crate) fn failed(
    worker: &mut Worker<'_>,
    account: &str,
    kind: &'static str,
    problem: &Problem,
) -> Response {
    let now = now_ms();
    let Some(live) = worker.live.get_mut(account) else {
        return Response::ItemError;
    };
    let response = live.health.failed_step(kind, problem, (account, now));
    worker
        .store
        .token_cells
        .set_paused(account, super::conn::paused_problem(&live.health.status));
    if !matches!(problem, Problem::Refused(_)) {
        live.conn = Conn::Closed;
        live.listed = false;
    }
    log::warn!("account {account}: {response:?}");
    response
}

/// The cache is damaged: start over from the server.
fn rebuild(worker: &mut Worker<'_>) {
    match worker.store.rebuild() {
        Ok(()) => {
            // Row ids did not survive: nothing waiting can be answered.
            worker.open_failed(None);
            worker.emit(Event::Notice {
                text: text::notice::cache_rebuilt(),
            });
            for (key, live) in &mut worker.live {
                // Folder ids did not survive.
                live.continuing = None;
                live.prefetch_skip.clear();
                live.prefetch_off.clear();
                live.wants = super::plan::Wants {
                    full: true,
                    ..super::plan::Wants::default()
                };
                worker.sink.emit(Event::FoldersChanged {
                    account: key.clone(),
                });
            }
            worker.refresh_unread();
        }
        Err(error) => {
            log::error!("rebuilding the cache: {error}");
            worker.emit(Event::Notice {
                text: text::notice::rebuild_failed(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use super::rebuild;
    use crate::api::event::Event;
    use crate::worker::testing::{Sink, open, seed};

    #[test]
    fn a_folder_with_parts_left_keeps_its_want() {
        use super::finished;
        use crate::worker::plan::Step;

        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let account = seeded.account.clone();
        let id = seeded.folder;
        let live = worker.live.get_mut(&account).unwrap();
        live.wants.folders.insert(id);
        live.continuing = Some(id);
        finished(&mut worker, &account, &Step::Folder(id));
        let live = worker.live.get_mut(&account).unwrap();
        assert!(live.wants.folders.contains(&id), "dropped between parts");
        // The last part: nothing continues, the want is met.
        live.continuing = None;
        finished(&mut worker, &account, &Step::Folder(id));
        let live = worker.live.get_mut(&account).unwrap();
        assert!(!live.wants.folders.contains(&id));
    }

    #[test]
    fn a_forward_waiting_on_a_partial_body_asks_again_after_the_step() {
        use super::succeeded;
        use crate::api::command::{ComposeId, StartFrom};
        use crate::worker::plan::Step;
        use envio_store::BodyState;

        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let (account, id) = (seeded.account.clone(), seeded.message);
        worker
            .store
            .db
            .set_body_state(id, BodyState::Partial)
            .unwrap();
        // A body step for the open message is running, not for the whole one.
        worker.live.get_mut(&account).unwrap().wants.bodies.push(id);
        worker.start_compose(ComposeId(7), StartFrom::Forward { id }, (None, None));
        assert_eq!(worker.composing.get(&id).map(Vec::len), Some(1));
        // The step ends with the partial body: the forward needs the rest.
        worker.body_fetched(id, Some(b"Subject: x\r\n\r\nhi"), false);
        succeeded(&mut worker, &account, &Step::Body(id));
        let live = &worker.live[&account];
        assert!(live.wants.bodies.contains(&id), "the ask was dropped");
        assert!(live.wants.whole.contains(&id));
        assert_eq!(worker.composing.get(&id).map(Vec::len), Some(1));
        // The whole message arrives (the cache stays partial): answered.
        worker.body_fetched(id, Some(b"Subject: x\r\n\r\nhi"), true);
        succeeded(&mut worker, &account, &Step::Body(id));
        assert!(worker.composing.is_empty(), "the forward was not answered");
        assert!(worker.live[&account].wants.bodies.is_empty());
        assert!(worker.live[&account].wants.whole.is_empty());
    }

    #[test]
    fn the_whole_message_of_a_big_one_is_not_shown_in_the_reader() {
        use super::succeeded;
        use crate::api::command::ImagesChoice;
        use crate::worker::plan::Step;

        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let (account, id) = (seeded.account.clone(), seeded.message);
        let big = envio_store::NewMessage {
            uid: 1,
            size: envio_sync::driver::PARTIAL_ABOVE + 1,
            envelope: envio_store::Envelope::default(),
            flags: envio_store::Flags::default(),
            preview: String::new(),
        };
        worker.store.db.upsert_message(seeded.folder, &big).unwrap();
        worker.open_message(id, ImagesChoice::Default, 300);
        assert!(worker.opening.contains_key(&id));
        // A forward's whole step answers the open too.
        worker
            .live
            .get_mut(&account)
            .unwrap()
            .wants
            .whole
            .insert(id);
        worker.body_fetched(id, Some(b"Subject: x\r\n\r\nthe whole 40 MB"), true);
        succeeded(&mut worker, &account, &Step::Body(id));
        let shown = sink
            .events()
            .into_iter()
            .any(|e| matches!(e, Event::MessageOpened { .. }));
        assert!(!shown, "the whole message was rendered in the reader");
        // The reader still waits, and gets the partial body next.
        assert!(worker.opening.contains_key(&id));
        let live = &worker.live[&account];
        assert!(live.wants.bodies.contains(&id));
        assert!(!live.wants.whole.contains(&id));
    }

    #[test]
    fn a_failed_rebuild_is_noticed() {
        let dir = tempfile::tempdir().unwrap();
        let (config, _) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let garbage = config.cache_dir.join("garbage");
        fs::write(&garbage, b"not a database, long enough to be read as one").unwrap();
        fs::rename(&garbage, worker.store.paths.db()).unwrap();
        // Moving the damaged file aside needs a writable folder.
        fs::set_permissions(&config.cache_dir, fs::Permissions::from_mode(0o500)).unwrap();
        let as_root = fs::write(config.cache_dir.join("probe"), b"x").is_ok();
        rebuild(&mut worker);
        fs::set_permissions(&config.cache_dir, fs::Permissions::from_mode(0o700)).unwrap();
        if as_root {
            eprintln!("skipped: running as root, read-only has no effect");
            return;
        }
        let notices: Vec<String> = sink
            .events()
            .into_iter()
            .filter_map(|e| match e {
                Event::Notice { text } => Some(text),
                _ => None,
            })
            .collect();
        assert_eq!(notices, [crate::text::notice::rebuild_failed()]);
    }
}

#[cfg(test)]
mod keep_tests {
    use super::*;
    use crate::worker::plan::Wants;
    use envio_store::MessageRowId;

    fn wanting(ids: &[i64]) -> Wants {
        Wants {
            bodies: ids.iter().map(|id| MessageRowId(*id)).collect(),
            ..Wants::default()
        }
    }

    #[test]
    fn the_foreground_always_keeps_the_session() {
        assert!(keep_open(true, false, &Step::Replay, &Wants::default()));
    }

    #[test]
    fn a_background_burst_of_bodies_shares_a_session_and_closes_with_the_last() {
        let two = wanting(&[1, 2]);
        assert!(keep_open(false, false, &Step::Body(MessageRowId(1)), &two));
        assert!(!keep_open(
            false,
            false,
            &Step::Body(MessageRowId(2)),
            &wanting(&[2])
        ));
        assert!(!keep_open(false, false, &Step::Replay, &two));
    }

    #[test]
    fn a_notifying_account_keeps_its_session_after_a_background_folder_sync() {
        let none = Wants::default();
        let step = Step::Folder(envio_store::FolderId(2));
        assert!(keep_open(false, true, &step, &none));
        assert!(!keep_open(false, false, &step, &none));
        assert!(!keep_open(false, true, &Step::Replay, &none));
        // A body opened in the background does not cost a reconnect.
        assert!(keep_open(false, true, &Step::Body(MessageRowId(1)), &none));
        assert!(!keep_open(
            false,
            false,
            &Step::Body(MessageRowId(1)),
            &none
        ));
    }
}
