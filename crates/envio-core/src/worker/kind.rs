//! Which Microsoft provider an address signs in with by default (#384,
//! Decision 13).

use envio_net::oauth::{ProviderKind, consumer_microsoft_domain, provider_kind_for};

use super::Worker;

impl Worker<'_> {
    /// `MicrosoftGraph` for a known consumer domain while the Graph provider
    /// is set up, so one consent saves a Graph account; else the address's
    /// own provider (`provider_kind_for`), as before. An explicit pick from
    /// the wizard never goes through here.
    pub(crate) fn default_kind(&self, email: &str) -> Option<ProviderKind> {
        let consumer = email
            .trim()
            .rsplit_once('@')
            .is_some_and(|(_, d)| consumer_microsoft_domain(d));
        if consumer && self.provider_of(ProviderKind::MicrosoftGraph).is_some() {
            return Some(ProviderKind::MicrosoftGraph);
        }
        provider_kind_for(email)
    }
}
