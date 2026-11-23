//! What is saved and what waits to be: no threads, so it can be tested alone.
//! The mutex is held for the bookkeeping and for the file write of one draft
//! (small, a few ms), never across a network call and never while an
//! attachment is copied (that can take as long as the file is big).

use std::collections::{BTreeMap, BTreeSet};
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use envio_store::{Draft, Drafts};

use crate::api::command::{ComposeId, DraftInput};
use crate::api::event::Event;
use crate::clock::now_ms;
use crate::compose::{self, SaveThrottle};
use crate::problem::{self, Problem};
use crate::text;

pub(crate) struct DraftBook {
    dir: PathBuf,
    /// Opened on first use, so a data folder that is not there yet (the
    /// store retries) does not stop the app from starting.
    drafts: Mutex<Option<Arc<Drafts>>>,
    state: Mutex<BookState>,
    /// Tells the draft thread to look at its clock again.
    waker: Mutex<Option<Box<dyn Fn() + Send>>>,
}

#[derive(Default)]
struct BookState {
    throttle: SaveThrottle,
    /// The stored draft of each compose window that has one.
    ids: BTreeMap<ComposeId, String>,
    /// The draft of a compose whose window closed with the draft kept: a
    /// Send still queued behind the close (it goes through the worker) must
    /// find and delete it.
    closed: BTreeMap<ComposeId, String>,
    /// The account each open compose window writes from.
    accounts: BTreeMap<ComposeId, String>,
    /// Composes whose message is queued, for the whole session: nothing more
    /// is saved for them, even after the window closed.
    sent: BTreeSet<ComposeId>,
    /// Composes whose last save failed (the user was told once).
    save_failed: BTreeSet<ComposeId>,
    /// Accounts being or already removed: their drafts are not saved.
    removed: BTreeSet<String>,
    /// Accounts with a window that closed since the worker last asked: their
    /// drafts may be ready to upload, or have a server copy to delete.
    closed_accounts: BTreeSet<String>,
    /// Composes whose close was asked for but not done yet (the draft thread
    /// may still be saving): their drafts count as closed.
    closing: BTreeSet<ComposeId>,
}

impl BookState {
    /// The compose's draft, open or closed.
    fn id_for(&self, compose: ComposeId) -> Option<String> {
        self.ids
            .get(&compose)
            .or_else(|| self.closed.get(&compose))
            .cloned()
    }
}

impl DraftBook {
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_owned(),
            drafts: Mutex::new(None),
            state: Mutex::new(BookState::default()),
            waker: Mutex::new(None),
        }
    }

    /// The drafts folder, shared so only one handle ever opens it (opening
    /// clears temp files a save in flight may be using). The first open of
    /// the process also removes attachment copies of drafts that are gone;
    /// no later one does, so a restarted worker never takes copies a draft
    /// still lists.
    pub fn drafts(&self) -> envio_store::Result<Arc<Drafts>> {
        let mut slot = self.drafts.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(drafts) = slot.as_ref() {
            return Ok(Arc::clone(drafts));
        }
        let drafts = Arc::new(Drafts::open(&self.dir)?);
        if let Err(error) = drafts.sweep_files() {
            log::warn!("sweeping draft files: {error}");
        }
        *slot = Some(Arc::clone(&drafts));
        Ok(drafts)
    }

    // A panic elsewhere must not stop saving: the state is plain data.
    fn lock(&self) -> MutexGuard<'_, BookState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn set_waker(&self, wake: impl Fn() + Send + 'static) {
        *self.waker.lock().unwrap_or_else(PoisonError::into_inner) = Some(Box::new(wake));
    }

    /// Like `changed`, for a caller that is not the draft thread (it is
    /// asleep until the next message or due time, so it is woken).
    pub fn requeue(&self, compose: ComposeId, draft: DraftInput, now_ms: i64) {
        self.changed(compose, draft, now_ms);
        let waker = self.waker.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(wake) = waker.as_ref() {
            wake();
        }
    }

    /// Drafts of windows that are open: kept off the server until closed.
    pub fn open_drafts(&self) -> BTreeSet<String> {
        let st = self.lock();
        st.ids
            .iter()
            .filter(|(compose, _)| !st.closing.contains(compose))
            .map(|(_, id)| id.clone())
            .collect()
    }

    /// A close was asked for: the account is reported to the worker and the
    /// draft stops counting as open at once, without waiting for the draft
    /// thread. A save still queued behind the close is uploaded by the next
    /// run (its text is newer than the copy).
    pub fn closing(&self, compose: ComposeId) {
        let mut st = self.lock();
        if let Some(account) = st.accounts.get(&compose).cloned() {
            st.closed_accounts.insert(account);
        }
        st.closing.insert(compose);
    }

    /// The accounts whose windows closed since the last call.
    pub fn take_closed_accounts(&self) -> BTreeSet<String> {
        std::mem::take(&mut self.lock().closed_accounts)
    }

    /// A compose window opened for `account`.
    pub fn opened(&self, compose: ComposeId, account: &str) {
        let mut st = self.lock();
        // An id is unique among open windows only: a reused one is new.
        st.closed.remove(&compose);
        st.accounts.insert(compose, account.to_owned());
    }

    /// The user changed the draft: it is due `DRAFT_SAVE_MS` after the first
    /// unsaved change.
    pub fn changed(&self, compose: ComposeId, draft: DraftInput, now_ms: i64) {
        let mut st = self.lock();
        // A sent message must not come back as a draft, nor a removed
        // account's.
        if st.sent.contains(&compose) || st.removed.contains(&draft.account) {
            return;
        }
        st.accounts.insert(compose, draft.account.clone());
        st.throttle.changed(compose, draft, now_ms);
    }

    /// Saves the drafts whose second is up.
    pub fn save_due(&self, now_ms: i64) -> Vec<Event> {
        let mut st = self.lock();
        let mut events = Vec::new();
        for (compose, draft) in st.throttle.due(now_ms) {
            self.persist(&mut st, (compose, draft), &mut events);
        }
        events
    }

    /// Writes the compose's pending change now and returns its draft id, if
    /// it has one. Send calls this so the draft is on disk with every change.
    pub fn flush(&self, compose: ComposeId) -> Result<Option<String>, String> {
        let mut st = self.lock();
        if let Some(draft) = st.throttle.take(compose) {
            match self.store_guarded(&mut st, compose, &draft) {
                Ok(_) => {
                    st.save_failed.remove(&compose);
                }
                Err(text) => {
                    st.throttle.retry(compose, draft, now_ms());
                    return Err(text);
                }
            }
        }
        Ok(st.id_for(compose))
    }

    /// Saves every draft still waiting for its second.
    pub fn flush_all(&self) -> Vec<Event> {
        let mut st = self.lock();
        let mut events = Vec::new();
        for (compose, draft) in st.throttle.take_all() {
            self.persist(&mut st, (compose, draft), &mut events);
        }
        events
    }

    /// The window closed: keep the draft (saved now), or delete it.
    pub fn close(&self, compose: ComposeId, discard: bool) -> Vec<Event> {
        let mut st = self.lock();
        let mut events = Vec::new();
        let pending = st.throttle.take(compose);
        let was_sent = st.sent.contains(&compose);
        st.closing.remove(&compose);
        st.save_failed.remove(&compose);
        let account = st.accounts.remove(&compose);
        if let Some(account) = &account {
            st.closed_accounts.insert(account.clone());
        }
        // The Drafts list shows a closed draft's text and a discarded one's
        // absence; a window that never had a draft changes nothing.
        let had_draft = st.id_for(compose).is_some() || pending.is_some();
        if discard {
            let id = st
                .ids
                .remove(&compose)
                .or_else(|| st.closed.remove(&compose));
            let fallback = pending.as_ref().and_then(|d| d.draft_id.clone());
            if let Some(id) = id.or(fallback)
                && let Err(e) = self.drafts().and_then(|drafts| drafts.delete(&id))
            {
                log::warn!("deleting a draft: {e}");
            }
        } else {
            let saved = match pending.filter(|_| !was_sent) {
                Some(draft) => self.persist(&mut st, (compose, draft), &mut events),
                None => true,
            };
            if saved && let Some(id) = st.ids.remove(&compose) {
                st.closed.insert(compose, id);
            }
        }
        if had_draft && let Some(account) = account {
            events.push(Event::LocalDraftsChanged { account });
        }
        events
    }

    /// Copies `path` into the compose's draft folder, first creating the
    /// draft when it has none. `Attached` or `AttachFailed`, after a
    /// `DraftSaved` when the draft was created.
    ///
    /// The copy runs without the state lock, so saves go on meanwhile. The
    /// draft may be deleted or its message sent while it runs: then the lock
    /// is taken again, the copy is dropped and the attach fails, so no entry
    /// or file is left for a draft that is gone. (A crash mid-copy leaves a
    /// partial file in the draft's folder; the next start's sweep only
    /// removes folders of drafts that are gone, so that file stays until
    /// the draft is deleted.)
    ///
    /// The events go to `emit`. The alive check and the final event are one
    /// critical section under the state lock, and `close` and `sent` take
    /// that lock too: a draft deleted after the check has `Attached` emitted
    /// before its delete (which removes the copy), and one deleted before it
    /// never gets `Attached`. `emit` must not block (the sinks queue).
    pub fn attach_emit(&self, compose: ComposeId, path: &Path, emit: &mut dyn FnMut(Event)) {
        let failed = |text: String| Event::AttachFailed { compose, text };
        let id = {
            let mut st = self.lock();
            if st.sent.contains(&compose) {
                emit(failed(text::compose::draft_not_saved()));
                return;
            }
            let mut events = Vec::new();
            let drafted = self.draft_for(&mut st, compose, &mut events);
            events.into_iter().for_each(&mut *emit);
            match drafted {
                Ok(id) => id,
                Err(text) => {
                    emit(failed(text));
                    return;
                }
            }
        };
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let drafts = self.drafts();
        let copy = match &drafts {
            Ok(drafts) => drafts.keep_attachment(&id, path),
            Err(e) => Err(envio_store::StoreError::Io(std::io::Error::other(
                e.to_string(),
            ))),
        };
        let st = self.lock();
        let alive =
            !st.sent.contains(&compose) && st.id_for(compose).as_deref() == Some(id.as_str());
        if !alive {
            if let Ok(drafts) = &drafts {
                drafts.forget_copy(&id, copy.as_ref().ok().map(PathBuf::as_path));
            }
            emit(failed(text::compose::draft_not_saved()));
            return;
        }
        emit(match copy {
            Ok(copy) => Event::Attached {
                compose,
                name: copy
                    .file_name()
                    .map_or(name, |n| n.to_string_lossy().into_owned()),
                size: std::fs::metadata(&copy).map_or(0, |m| m.len()),
                path: copy,
            },
            Err(e) => {
                log::warn!("copying an attachment: {e}");
                failed(text::compose::attachment_unreadable(&name))
            }
        });
        drop(st);
    }

    /// `attach_emit`, the events collected.
    #[cfg(test)]
    pub fn attach(&self, compose: ComposeId, path: &Path) -> Vec<Event> {
        let mut events = Vec::new();
        self.attach_emit(compose, path, &mut |e| events.push(e));
        events
    }

    /// The message is queued: nothing more is saved for the compose. Returns
    /// the draft to delete, if there is one.
    pub fn sent(&self, compose: ComposeId) -> Option<String> {
        let mut st = self.lock();
        st.sent.insert(compose);
        st.throttle.take(compose);
        let id = st.ids.remove(&compose);
        id.or_else(|| st.closed.remove(&compose))
    }

    pub fn is_sent(&self, compose: ComposeId) -> bool {
        self.lock().sent.contains(&compose)
    }

    pub fn id_of(&self, compose: ComposeId) -> Option<String> {
        self.lock().ids.get(&compose).cloned()
    }

    /// The compose edits a draft that already exists: saves go to its file.
    pub fn adopt(&self, compose: ComposeId, draft_id: String) {
        let mut st = self.lock();
        st.closed.remove(&compose);
        st.ids.insert(compose, draft_id);
    }

    /// The compose that holds `draft_id` open, if any. A window whose close
    /// is still being saved counts: reading the file now could miss its
    /// last save.
    pub fn holder(&self, draft_id: &str) -> Option<ComposeId> {
        let st = self.lock();
        st.ids
            .iter()
            .find(|(_, id)| id.as_str() == draft_id)
            .map(|(compose, _)| *compose)
    }

    pub fn next_due(&self) -> Option<i64> {
        self.lock().throttle.next_due()
    }

    /// An account is going: its pending saves, and any later ones, are
    /// dropped.
    pub fn drop_account(&self, account: &str) {
        let mut st = self.lock();
        st.throttle.drop_account(account);
        st.removed.insert(account.to_owned());
    }

    /// The compose's draft id; creates the draft (from its pending change, or
    /// empty) when it has none.
    fn draft_for(
        &self,
        st: &mut BookState,
        compose: ComposeId,
        events: &mut Vec<Event>,
    ) -> Result<String, String> {
        if let Some(id) = st.ids.get(&compose) {
            return Ok(id.clone());
        }
        let draft = match st.throttle.take(compose) {
            Some(draft) => draft,
            None => DraftInput {
                account: st.accounts.get(&compose).cloned().unwrap_or_default(),
                ..DraftInput::default()
            },
        };
        if draft.account.is_empty() {
            return Err(text::compose::draft_not_saved());
        }
        if !self.persist(st, (compose, draft), events) {
            return Err(text::compose::draft_not_saved());
        }
        st.ids
            .get(&compose)
            .cloned()
            .ok_or_else(text::compose::draft_not_saved)
    }

    /// Saves the draft; when that fails it stays pending and is tried again
    /// a second later, so closing the window afterwards still saves it.
    fn persist(
        &self,
        st: &mut BookState,
        (compose, draft): (ComposeId, DraftInput),
        events: &mut Vec<Event>,
    ) -> bool {
        match self.store_guarded(st, compose, &draft) {
            Ok(created) => {
                st.save_failed.remove(&compose);
                if let Some(draft_id) = created {
                    events.push(Event::DraftSaved { compose, draft_id });
                    events.push(Event::LocalDraftsChanged {
                        account: draft.account.clone(),
                    });
                }
                events.push(Event::DraftStored {
                    compose,
                    at_ms: now_ms(),
                });
                true
            }
            Err(text) => {
                st.throttle.retry(compose, draft, now_ms());
                // Once per run of failures: it is retried every second.
                if st.save_failed.insert(compose) {
                    events.push(Event::Notice { text });
                }
                false
            }
        }
    }

    /// `store`, where a panic counts as a failed write: the change stays
    /// pending and the next tick tries again.
    fn store_guarded(
        &self,
        st: &mut BookState,
        compose: ComposeId,
        draft: &DraftInput,
    ) -> Result<Option<String>, String> {
        panic::catch_unwind(AssertUnwindSafe(|| self.store(st, compose, draft))).unwrap_or_else(
            |_| {
                log::error!("saving a draft panicked; it stays pending");
                Err(text::compose::draft_not_saved())
            },
        )
    }

    /// Writes the draft: created the first time, replaced after. The id when
    /// it was created; the user's text when the write failed.
    fn store(
        &self,
        st: &mut BookState,
        compose: ComposeId,
        draft: &DraftInput,
    ) -> Result<Option<String>, String> {
        if st.removed.contains(&draft.account) {
            return Ok(None);
        }
        let stored: Draft = compose::to_store(draft, now_ms());
        // An empty id is no id: a new compose may carry "" for it.
        let known = st
            .id_for(compose)
            .or_else(|| draft.draft_id.clone().filter(|id| !id.is_empty()));
        let result = self.drafts().and_then(|drafts| match &known {
            Some(id) => drafts.save(id, &stored).map(|()| None),
            None => drafts.create(&stored).map(Some),
        });
        match result {
            Ok(created) => {
                if let Some(id) = known.or_else(|| created.clone()) {
                    st.ids.insert(compose, id);
                }
                Ok(created)
            }
            Err(error) => {
                log::warn!("saving a draft: {error}");
                Err(match problem::from_store(&error) {
                    Problem::DiskFull => text::notice::disk_full(),
                    _ => text::compose::draft_not_saved(),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compose::DRAFT_SAVE_MS;

    const A: ComposeId = ComposeId(1);
    const B: ComposeId = ComposeId(2);

    fn book(dir: &Path) -> DraftBook {
        DraftBook::new(&dir.join("drafts"))
    }

    fn input(body: &str) -> DraftInput {
        DraftInput {
            account: "acct".into(),
            body: body.into(),
            ..DraftInput::default()
        }
    }

    fn saved(events: &[Event]) -> usize {
        events
            .iter()
            .filter(|e| matches!(e, Event::DraftSaved { .. }))
            .count()
    }

    #[test]
    fn continuous_changes_save_every_second() {
        let dir = tempfile::tempdir().unwrap();
        let book = book(dir.path());
        book.changed(A, input("a"), 0);
        for ms in (100..DRAFT_SAVE_MS).step_by(100) {
            book.changed(A, input(&ms.to_string()), ms);
        }
        assert_eq!(book.next_due(), Some(DRAFT_SAVE_MS));
        assert!(book.save_due(DRAFT_SAVE_MS - 1).is_empty());
        assert_eq!(saved(&book.save_due(DRAFT_SAVE_MS)), 1);
        let drafts = book.drafts().unwrap().list().unwrap();
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].1.body, "900");
        // The next change starts a new second and replaces the same file.
        book.changed(A, input("b"), 1_050);
        assert_eq!(book.next_due(), Some(1_050 + DRAFT_SAVE_MS));
        assert_eq!(saved(&book.save_due(2_050)), 0);
        let drafts = book.drafts().unwrap().list().unwrap();
        assert_eq!((drafts.len(), drafts[0].1.body.as_str()), (1, "b"));
    }

    #[test]
    fn every_save_tells_when_it_happened_not_only_the_first() {
        let dir = tempfile::tempdir().unwrap();
        let book = book(dir.path());
        let stored = |events: &[Event]| {
            events
                .iter()
                .filter(|e| matches!(e, Event::DraftStored { compose, at_ms } if *compose == A && *at_ms > 0))
                .count()
        };
        book.changed(A, input("a"), 0);
        let first = book.save_due(DRAFT_SAVE_MS);
        assert_eq!((saved(&first), stored(&first)), (1, 1), "{first:?}");
        // The second save updates the same draft: no `DraftSaved`, but the
        // footer's time moves.
        book.changed(A, input("b"), 1_500);
        let second = book.save_due(1_500 + DRAFT_SAVE_MS);
        assert_eq!((saved(&second), stored(&second)), (0, 1), "{second:?}");
    }

    #[test]
    fn an_empty_draft_id_is_a_new_draft() {
        // The QML page sends `draftId: ""` until the draft has one.
        let dir = tempfile::tempdir().unwrap();
        let book = book(dir.path());
        let mut draft = input("hello");
        draft.draft_id = Some(String::new());
        book.changed(A, draft, 0);
        let events = book.save_due(DRAFT_SAVE_MS);
        assert_eq!(saved(&events), 1, "{events:?}");
        assert!(!events.iter().any(|e| matches!(e, Event::Notice { .. })));
        assert_eq!(book.drafts().unwrap().list().unwrap().len(), 1);
    }

    #[test]
    fn close_without_discard_saves_now() {
        let dir = tempfile::tempdir().unwrap();
        let book = book(dir.path());
        book.changed(A, input("keep"), 0);
        let events = book.close(A, false);
        assert_eq!(saved(&events), 1);
        assert_eq!(book.next_due(), None);
        assert_eq!(book.drafts().unwrap().list().unwrap()[0].1.body, "keep");
        assert_eq!(book.id_of(A), None);
    }

    #[test]
    fn a_draft_is_open_until_its_window_closes_and_the_close_names_the_account() {
        let dir = tempfile::tempdir().unwrap();
        let book = book(dir.path());
        book.opened(A, &input("x").account);
        book.changed(A, input("x"), 0);
        book.save_due(DRAFT_SAVE_MS);
        assert_eq!(book.open_drafts().len(), 1);
        assert!(book.take_closed_accounts().is_empty());
        book.close(A, false);
        assert!(book.open_drafts().is_empty());
        assert_eq!(
            book.take_closed_accounts().into_iter().collect::<Vec<_>>(),
            [input("x").account]
        );
        assert!(book.take_closed_accounts().is_empty(), "taken once");
    }

    #[test]
    fn a_close_asked_for_is_known_before_the_draft_thread_has_done_it() {
        let dir = tempfile::tempdir().unwrap();
        let book = book(dir.path());
        book.opened(A, &input("x").account);
        book.changed(A, input("x"), 0);
        book.save_due(DRAFT_SAVE_MS);
        book.closing(A);
        assert!(book.open_drafts().is_empty());
        assert_eq!(book.take_closed_accounts().len(), 1);
    }

    #[test]
    fn discard_deletes_the_draft_and_its_files() {
        let dir = tempfile::tempdir().unwrap();
        let book = book(dir.path());
        let source = dir.path().join("note.txt");
        std::fs::write(&source, b"hi").unwrap();
        book.changed(A, input("x"), 0);
        let events = book.attach(A, &source);
        let Some(Event::Attached { path, .. }) = events.last() else {
            panic!("not attached: {events:?}");
        };
        assert!(path.exists());
        let folder = path.parent().unwrap().to_owned();
        // Only the Drafts list hears of it.
        let told = book.close(A, true);
        assert!(matches!(told[..], [Event::LocalDraftsChanged { .. }]));
        assert!(book.drafts().unwrap().list().unwrap().is_empty());
        assert!(!folder.exists());
        assert_eq!(book.next_due(), None);
    }

    #[test]
    fn no_save_after_sent() {
        let dir = tempfile::tempdir().unwrap();
        let book = book(dir.path());
        book.changed(A, input("a"), 0);
        assert!(book.flush(A).unwrap().is_some());
        assert!(book.id_of(A).is_some());
        let id = book.sent(A);
        assert!(id.is_some());
        book.changed(A, input("late"), 10);
        assert_eq!(book.next_due(), None);
        assert!(book.save_due(i64::MAX).is_empty());
        assert!(book.close(A, false).is_empty());
    }

    #[test]
    fn flush_all_returns_one_event_per_saved_draft() {
        let dir = tempfile::tempdir().unwrap();
        let book = book(dir.path());
        book.changed(A, input("a"), 0);
        book.changed(B, input("b"), 5);
        assert_eq!(saved(&book.flush_all()), 2);
        assert_eq!(book.drafts().unwrap().list().unwrap().len(), 2);
        assert!(book.flush_all().is_empty());
    }

    #[test]
    fn attach_creates_the_draft_first() {
        let dir = tempfile::tempdir().unwrap();
        let book = book(dir.path());
        let source = dir.path().join("note.txt");
        std::fs::write(&source, b"hello").unwrap();
        book.opened(A, "acct");
        let events = book.attach(A, &source);
        assert!(matches!(events[0], Event::DraftSaved { .. }));
        assert!(matches!(events[1], Event::LocalDraftsChanged { .. }));
        assert!(matches!(events[2], Event::DraftStored { .. }));
        let Event::Attached {
            path, name, size, ..
        } = &events[3]
        else {
            panic!("not attached: {events:?}");
        };
        assert_eq!((name.as_str(), *size), ("note.txt", 5));
        assert_eq!(std::fs::read(path).unwrap(), b"hello");
        assert_eq!(book.drafts().unwrap().list().unwrap().len(), 1);
        // A second file goes to the same draft.
        assert_eq!(saved(&book.attach(A, &source)), 0);
        assert_eq!(book.drafts().unwrap().list().unwrap().len(), 1);
    }

    #[test]
    fn a_reused_compose_id_starts_a_new_draft() {
        let dir = tempfile::tempdir().unwrap();
        let book = book(dir.path());
        book.changed(A, input("old"), 0);
        assert_eq!(saved(&book.close(A, false)), 1);
        // The app reuses the id for a new window.
        book.opened(A, "acct");
        book.changed(A, input("new"), 10);
        assert_eq!(saved(&book.flush_all()), 1);
        let bodies: Vec<String> = book
            .drafts()
            .unwrap()
            .list()
            .unwrap()
            .into_iter()
            .map(|(_, d)| d.body)
            .collect();
        assert_eq!(bodies.len(), 2);
        assert!(bodies.contains(&"old".to_owned()) && bodies.contains(&"new".to_owned()));
    }

    #[test]
    fn an_unreadable_file_is_not_attached() {
        let dir = tempfile::tempdir().unwrap();
        let book = book(dir.path());
        book.opened(A, "acct");
        let events = book.attach(A, &dir.path().join("missing.txt"));
        assert!(matches!(events.last(), Some(Event::AttachFailed { .. })));
    }

    #[test]
    fn a_removed_accounts_drafts_are_not_saved() {
        let dir = tempfile::tempdir().unwrap();
        let book = book(dir.path());
        book.changed(A, input("a"), 0);
        book.drop_account("acct");
        book.changed(B, input("b"), 0);
        assert_eq!(book.next_due(), None);
        assert!(book.flush_all().is_empty());
    }

    #[test]
    fn a_copy_does_not_hold_the_lock_and_a_draft_deleted_meanwhile_leaves_nothing() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let book = Arc::new(book(dir.path()));
        // A named pipe: the copy blocks until the test writes to it.
        let slow = dir.path().join("slow.bin");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&slow)
                .status()
                .unwrap()
                .success()
        );
        book.opened(A, "acct");
        let copier = {
            let (book, slow) = (Arc::clone(&book), slow.clone());
            std::thread::spawn(move || book.attach(A, &slow))
        };
        std::thread::sleep(std::time::Duration::from_millis(300));
        // The state lock is free while the copy waits.
        let (tx, rx) = std::sync::mpsc::channel();
        {
            let book = Arc::clone(&book);
            std::thread::spawn(move || {
                book.changed(B, input("other"), 0);
                let events = book.save_due(DRAFT_SAVE_MS);
                let flushed = book.flush(B);
                let _ = tx.send((events, flushed));
            });
        }
        let free = rx.recv_timeout(std::time::Duration::from_secs(2));
        // The user discards the draft while its copy runs.
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let closer = {
            let book = Arc::clone(&book);
            std::thread::spawn(move || {
                book.close(A, true);
                let _ = done_tx.send(());
            })
        };
        let discarded = done_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .is_ok();
        std::fs::OpenOptions::new()
            .write(true)
            .open(&slow)
            .unwrap()
            .write_all(b"payload")
            .unwrap();
        let events = copier.join().unwrap();
        closer.join().unwrap();
        let (events_b, flushed) = free.expect("save_due or flush waited for the copy");
        assert_eq!(saved(&events_b), 1);
        assert!(flushed.unwrap().is_some());
        assert!(discarded, "close waited for the copy");
        assert!(matches!(events.last(), Some(Event::AttachFailed { .. })));
        let files = dir.path().join("drafts").join("files");
        assert_eq!(std::fs::read_dir(files).map_or(0, Iterator::count), 0);
        // Only the other compose's draft is left.
        assert_eq!(book.drafts().unwrap().list().unwrap().len(), 1);
    }

    #[test]
    fn a_delete_waits_for_the_attached_event_so_it_removes_the_copy() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let dir = tempfile::tempdir().unwrap();
        let book = Arc::new(book(dir.path()));
        let source = dir.path().join("a.txt");
        std::fs::write(&source, b"x").unwrap();
        book.opened(A, "acct");
        let closed = Arc::new(AtomicBool::new(false));
        let (mut closer, mut blocked_during_emit) = (None, false);
        book.attach_emit(A, &source, &mut |event| {
            if matches!(event, Event::Attached { .. }) {
                let (book, flag) = (Arc::clone(&book), Arc::clone(&closed));
                closer = Some(std::thread::spawn(move || {
                    book.close(A, true);
                    flag.store(true, Ordering::SeqCst);
                }));
                std::thread::sleep(std::time::Duration::from_millis(300));
                blocked_during_emit = !closed.load(Ordering::SeqCst);
            }
        });
        closer.expect("Attached was emitted").join().unwrap();
        assert!(blocked_during_emit, "the delete ran between check and emit");
        let files = dir.path().join("drafts").join("files");
        assert_eq!(std::fs::read_dir(files).map_or(0, Iterator::count), 0);
    }
}
