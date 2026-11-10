//! Finding an account's servers from its address: Thunderbird's ISPDB,
//! the provider's autoconfig file, the well-known URL, DNS SRV, then a
//! guess for the manual form. The order and choices are in `plan`; the
//! lookups sit behind [`Lookups`] so tests run without a network.

mod http;
pub mod plan;
mod srv;
pub mod xml;

use std::future::Future;

use crate::error::{NetError, Result};
use crate::tls::Roots;
use plan::{Candidates, Service};

pub use plan::{Found, Setting, Source, SrvRecord};

/// The two kinds of lookup autoconfig makes. Failures are "nothing
/// here": autoconfig moves on to the next source.
pub trait Lookups: Sync {
    /// The body at an https URL, or None.
    fn fetch(&self, url: &str) -> impl Future<Output = Option<String>> + Send;
    /// The SRV records for a name; empty if none.
    fn srv(&self, name: &str) -> impl Future<Output = Vec<SrvRecord>> + Send;
}

/// The real thing: HTTPS checked against `roots`, DNS from the system.
pub struct Internet {
    roots: Roots,
}

impl Internet {
    pub fn new(roots: Roots) -> Self {
        Self { roots }
    }
}

impl Lookups for Internet {
    async fn fetch(&self, url: &str) -> Option<String> {
        http::get(url, &self.roots).await.ok().flatten()
    }

    async fn srv(&self, name: &str) -> Vec<SrvRecord> {
        srv::lookup(name).await
    }
}

/// The settings for `email`. Only an address without a domain is an
/// error; when every lookup comes up empty, the answer is the guess.
pub async fn detect(email: &str, lookups: &impl Lookups) -> Result<Found> {
    let domain =
        plan::domain_of(email).ok_or_else(|| NetError::Refused("not an email address".into()))?;
    for (source, url) in plan::config_urls(&domain) {
        let Some(body) = lookups.fetch(&url).await else {
            continue;
        };
        if let Some(found) = xml::parse(&body, email).and_then(|c| plan::best(&c, source, &domain))
        {
            return Ok(found);
        }
    }
    let mut candidates = Candidates::default();
    for (name, service, security) in plan::srv_queries(&domain) {
        let settings = plan::srv_candidates(lookups.srv(&name).await, security, email);
        match service {
            Service::Imap => candidates.imap.extend(settings),
            Service::Smtp => candidates.smtp.extend(settings),
        }
    }
    Ok(
        plan::best(&candidates, Source::Srv, &domain)
            .unwrap_or_else(|| plan::guess(&domain, email)),
    )
}
