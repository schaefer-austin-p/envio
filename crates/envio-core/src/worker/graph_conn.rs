//! Connecting a Graph account: no login handshake, only a client over the
//! account's Graph token and the uid map in the cache file.

use envio_net::graph::GraphClient;
use envio_net::oauth::ProviderKind;
use envio_store::GraphMap;
use envio_sync::graph::GraphServer;

use super::Worker;
use super::backend::Backend;
use super::conn::paused_problem;
use super::graph_bearer::CoreBearer;
use crate::cause;
use crate::problem::{self, Problem};

#[cfg(test)]
thread_local! {
    /// Where the Graph client connects in tests: the mock server's host, port
    /// and roots, in place of graph.microsoft.com.
    pub(crate) static TEST_HOST: std::cell::RefCell<Option<(u16, envio_net::Roots)>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
mod tests;

impl Worker<'_> {
    /// A Graph backend for `account`. A paused account is not given one,
    /// like `login_for`.
    pub(crate) fn graph_backend(&self, account: &str) -> Result<Backend, Problem> {
        if let Some(problem) = self
            .live
            .get(account)
            .and_then(|live| paused_problem(&live.health.status))
        {
            return Err(problem);
        }
        let handle = self
            .store
            .token_cells
            .handle(account, ProviderKind::MicrosoftGraph)
            .ok_or_else(|| {
                cause::log_stage(cause::NO_PROVIDER);
                Problem::SignIn
            })?;
        let map = GraphMap::open(&self.store.paths.db()).map_err(|e| problem::from_store(&e))?;
        let bearer = Box::new(CoreBearer::new(handle));
        Ok(Backend::Graph(GraphServer::new(
            self.graph_client(bearer),
            map,
        )))
    }

    fn graph_client(&self, bearer: Box<CoreBearer>) -> GraphClient {
        #[cfg(test)]
        if let Some((port, roots)) = TEST_HOST.with(|h| h.borrow().clone()) {
            return GraphClient::with_host(envio_net::graph::mock::HOST, port, roots, bearer);
        }
        GraphClient::new(self.roots.clone(), bearer)
    }
}
