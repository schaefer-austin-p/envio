//! Search: the question asked last and its answers.

use envio_core::{Command, MessageRow, SearchScope};

use super::{Output, Ui, change};
use crate::models::Model;
use crate::signals::Signal;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SearchState {
    scope: SearchScope,
    query: String,
    server: bool,
}

impl SearchState {
    pub fn new(scope: SearchScope, query: String, server: bool) -> Self {
        Self {
            scope,
            query,
            server,
        }
    }
}

impl Ui {
    /// Ask a search; the outputs clear the selection marks of rows being
    /// left behind.
    pub fn search(
        &mut self,
        scope: SearchScope,
        query: String,
        server: bool,
    ) -> (Command, Vec<Output>) {
        let outs = self.clear_marks();
        self.mirror.track(&outs);
        self.search = Some(SearchState::new(scope.clone(), query.clone(), server));
        let command = if server {
            Command::SearchServer { scope, query }
        } else {
            Command::Search { scope, query }
        };
        (command, outs)
    }

    /// An answer to anything but the last question is dropped.
    pub(super) fn search_results(&mut self, got: &SearchState, rows: &[MessageRow]) -> Vec<Output> {
        if self.search.as_ref() != Some(got) {
            return Vec::new();
        }
        let page = rows.iter().map(|r| self.shown(r)).collect();
        vec![change(Model::Search, self.found.reset(page))]
    }

    pub(super) fn search_failed(
        &self,
        scope: &SearchScope,
        query: &str,
        text: String,
    ) -> Vec<Output> {
        let state = self.search.as_ref();
        if state.is_some_and(|s| s.scope == *scope && s.query == query) {
            vec![Output::Signal(Signal::SearchFailed(text))]
        } else {
            Vec::new()
        }
    }
}
