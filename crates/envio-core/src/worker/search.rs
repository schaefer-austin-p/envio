//! Search: the local one (header and body index) answers at once; "Search
//! on server" is a step per account that has to connect, and its answers
//! are gathered into one `SearchResults`. Queries are never logged.

use std::collections::BTreeSet;

use envio_store::{FolderId, MessageRowId, Within};

use super::Worker;
use super::rows::rows;
use crate::api::command::SearchScope;
use crate::api::event::Event;
use crate::api::view::MessageRow;
use crate::clock::now_ms;
use crate::problem::Problem;
use crate::text;

pub(crate) const SEARCH_LIMIT: usize = 200;

/// A server search in progress: the accounts that have not answered yet and
/// what the others found.
pub(crate) struct ServerSearch {
    scope: SearchScope,
    query: String,
    waiting: BTreeSet<String>,
    rows: Vec<MessageRow>,
    /// At least one account answered.
    answered: bool,
    /// At least one account could not.
    failed: bool,
}

impl Worker<'_> {
    /// The cache's answer, newest first.
    pub(super) fn search(&self, scope: SearchScope, query: String) {
        let limit = u32::try_from(SEARCH_LIMIT).unwrap_or(u32::MAX);
        let within = match &scope {
            SearchScope::All => Within::Everything,
            SearchScope::Account(account) => Within::Account(account),
            SearchScope::Folder(folder) => Within::Folder(*folder),
        };
        match self.store.db.search_in(within, &query, limit) {
            Ok(messages) => {
                let rows = rows(&self.store.db, messages);
                self.emit(Event::SearchResults {
                    scope,
                    query,
                    server: false,
                    rows,
                });
            }
            Err(e) => self.read_failed(&e),
        }
    }

    /// The (account, folder) pairs a server search of `scope` asks. A
    /// folder is `None` when the account's Inbox is not cached yet.
    fn server_targets(&self, scope: &SearchScope) -> Vec<(String, Option<FolderId>)> {
        match scope {
            SearchScope::All => self
                .store
                .accounts
                .list()
                .iter()
                .map(|a| (a.key.clone(), self.inbox_of(&a.key)))
                .collect(),
            SearchScope::Account(account) => vec![(account.clone(), self.inbox_of(account))],
            SearchScope::Folder(f) if self.local_folders.borrow().account_of(*f).is_some() => {
                Vec::new()
            }
            SearchScope::Folder(folder) => match self.store.db.folder(*folder) {
                Ok(f) => vec![(f.account, Some(*folder))],
                Err(e) => {
                    log::warn!("finding the folder to search: {e}");
                    Vec::new()
                }
            },
        }
    }

    /// Starts a server search; it replaces the one before it (that one's
    /// late answers are dropped).
    pub(super) fn search_server(&mut self, scope: SearchScope, query: String) {
        for live in self.live.values_mut() {
            live.wants.searches.clear();
        }
        let targets = self.server_targets(&scope);
        self.server_search = Some(ServerSearch {
            scope,
            query: query.clone(),
            waiting: targets.iter().map(|(account, _)| account.clone()).collect(),
            rows: Vec::new(),
            answered: false,
            failed: false,
        });
        let now = now_ms();
        for (account, folder) in targets {
            let Some(live) = self.live.get(&account) else {
                self.search_failed(&account, text::read::search_unavailable());
                continue;
            };
            // A broken, paused or offline account is not connected for this.
            if live.health.is_paused() || !live.health.can_run(now) {
                let status = self.status_text(&account, &live.health.status);
                self.search_failed(&account, status);
                continue;
            }
            let Some(live) = self.live.get_mut(&account) else {
                continue;
            };
            match folder {
                Some(folder) => live.wants.searches.push((folder, query.clone())),
                None => self.search_failed(&account, text::read::search_unavailable()),
            }
        }
        self.search_check();
    }

    /// A search step found `ids` in `account`.
    pub(super) fn search_found(&mut self, account: &str, query: &str, ids: &[MessageRowId]) {
        let Some(search) = self.server_search.as_mut() else {
            return;
        };
        if search.query != query || !search.waiting.remove(account) {
            return;
        }
        let messages = ids
            .iter()
            .filter_map(|id| self.store.db.message(*id).ok())
            .collect();
        let found = rows(&self.store.db, messages);
        if let Some(search) = self.server_search.as_mut() {
            search.rows.extend(found);
            search.answered = true;
        }
        self.search_check();
    }

    /// The query of the server search still waiting for `account`.
    pub(super) fn searching(&self, account: &str) -> Option<String> {
        let search = self.server_search.as_ref()?;
        search
            .waiting
            .contains(account)
            .then(|| search.query.clone())
    }

    /// A search step of `account` failed with `problem`.
    pub(super) fn search_step_failed(&mut self, account: &str, query: &str, problem: &Problem) {
        if self.server_search.as_ref().is_none_or(|s| s.query != query) {
            return;
        }
        let text = match problem {
            Problem::Refused(reason) if !reason.is_empty() => text::notice::refused(reason),
            Problem::Refused(_) => text::read::search_unavailable(),
            _ => match self.live.get(account) {
                Some(live) => self.status_text(account, &live.health.status),
                None => text::read::search_unavailable(),
            },
        };
        // Nobody waits for it any more: it is not retried.
        if let Some(live) = self.live.get_mut(account) {
            live.wants.searches.clear();
        }
        self.search_failed(account, text);
        self.search_check();
    }

    /// Tells the app `account` could not be asked.
    fn search_failed(&mut self, account: &str, text: String) {
        let Some(search) = self.server_search.as_mut() else {
            return;
        };
        if !search.waiting.remove(account) {
            return;
        }
        search.failed = true;
        let (scope, query) = (search.scope.clone(), search.query.clone());
        self.emit(Event::SearchFailed { scope, query, text });
    }

    /// Answers once every account has: with all the rows found, unless
    /// every account failed (each said so already).
    fn search_check(&mut self) {
        let done = self
            .server_search
            .as_ref()
            .is_some_and(|s| s.waiting.is_empty());
        if !done {
            return;
        }
        let Some(mut search) = self.server_search.take() else {
            return;
        };
        if search.failed && !search.answered {
            return;
        }
        search
            .rows
            .sort_by_key(|row| std::cmp::Reverse((row.cursor.date, row.cursor.id)));
        search.rows.dedup_by_key(|row| row.id);
        self.emit(Event::SearchResults {
            scope: search.scope,
            query: search.query,
            server: true,
            rows: search.rows,
        });
    }
}
