//! Finding an account's servers from its address: Thunderbird's ISPDB,
//! the provider's autoconfig file, the well-known URL, DNS SRV, then a
//! guess for the manual form. The order and choices are in `plan`; the
//! lookups sit behind [`Lookups`] so tests run without a network.

mod fetch;
pub mod hosting;
mod mx;
pub mod plan;
mod srv;
pub mod xml;

use std::future::Future;
use std::sync::OnceLock;
use std::time::Duration;

use futures::StreamExt;
use futures::stream::FuturesOrdered;
use hickory_resolver::TokioResolver;

use crate::error::{NetError, Result};
use crate::tls::Roots;
use plan::{Candidates, Service};

pub use hosting::{Hosting, MxRecord, hosting_of};
pub use plan::{Found, Setting, Source, SrvRecord};

/// All of one `detect`, every lookup included. When it runs out, the
/// answer is the guess.
pub const DETECT_TIMEOUT: Duration = Duration::from_secs(60);

/// The kinds of lookup autoconfig makes. Failures are "nothing
/// here": autoconfig moves on to the next source.
pub trait Lookups: Sync {
    /// The body at an https URL, or None.
    fn fetch(&self, url: &str) -> impl Future<Output = Option<String>> + Send;
    /// The SRV records for a name; empty if none.
    fn srv(&self, name: &str) -> impl Future<Output = Vec<SrvRecord>> + Send;
    /// The MX records for a name; empty if none.
    fn mx(&self, name: &str) -> impl Future<Output = Vec<MxRecord>> + Send;
}

/// The real thing: HTTPS checked against `roots`, DNS from the system.
/// Make one per detect: it builds its DNS resolver on the first SRV
/// query and reuses it for the rest.
pub struct Internet {
    roots: Roots,
    limit: Duration,
    resolver: OnceLock<Option<TokioResolver>>,
}

impl Internet {
    pub fn new(roots: Roots) -> Self {
        Self::with_fetch_limit(roots, fetch::FETCH_TIMEOUT)
    }

    /// As `new`, with another limit on each fetch (redirects included).
    /// Tests use a short one.
    pub fn with_fetch_limit(roots: Roots, limit: Duration) -> Self {
        Self {
            roots,
            limit,
            resolver: OnceLock::new(),
        }
    }

    /// The resolver, built on the first call and the same one after.
    fn resolver(&self) -> Option<&TokioResolver> {
        self.resolver.get_or_init(crate::dns::resolver).as_ref()
    }
}

impl Lookups for Internet {
    async fn fetch(&self, url: &str) -> Option<String> {
        fetch::get(url, &self.roots, self.limit)
            .await
            .ok()
            .flatten()
    }

    async fn srv(&self, name: &str) -> Vec<SrvRecord> {
        match self.resolver() {
            Some(resolver) => srv::lookup(resolver, name).await,
            None => Vec::new(),
        }
    }

    async fn mx(&self, name: &str) -> Vec<MxRecord> {
        match self.resolver() {
            Some(resolver) => mx::lookup(resolver, name).await,
            None => Vec::new(),
        }
    }
}

/// The settings for `email`, within [`DETECT_TIMEOUT`]. Only an address
/// without a domain is an error; when every lookup comes up empty or
/// runs out of time, the answer is the guess.
pub async fn detect(email: &str, lookups: &impl Lookups) -> Result<Found> {
    detect_within(email, lookups, DETECT_TIMEOUT).await
}

/// As [`detect`], with `budget` in place of [`DETECT_TIMEOUT`].
pub async fn detect_within(email: &str, lookups: &impl Lookups, budget: Duration) -> Result<Found> {
    Ok(detect_with_hosting(email, lookups, budget).await?.found)
}

/// What `detect_with_hosting` found: the settings, and who hosts the
/// domain's mail by its MX records.
#[derive(Debug, Clone)]
pub struct Detection {
    pub found: Found,
    pub hosting: Hosting,
}

/// As [`detect_within`], also asking for the domain's MX records at the
/// same time (limited to `MX_TIMEOUT`).
pub async fn detect_with_hosting(
    email: &str,
    lookups: &impl Lookups,
    budget: Duration,
) -> Result<Detection> {
    detect_with_limits(email, lookups, budget, mx::MX_TIMEOUT).await
}

/// As [`detect_with_hosting`], with `mx_limit` in place of the MX
/// timeout. Each side has its own limit, so a settled `find` survives an
/// MX timeout and the reverse; the whole stays within `budget`. Public
/// for the tests, which use short limits.
#[doc(hidden)]
pub async fn detect_with_limits(
    email: &str,
    lookups: &impl Lookups,
    budget: Duration,
    mx_limit: Duration,
) -> Result<Detection> {
    let domain =
        plan::domain_of(email).ok_or_else(|| NetError::Refused("not an email address".into()))?;
    let (found, records) = futures::join!(
        tokio::time::timeout(budget, find(&domain, email, lookups)),
        tokio::time::timeout(mx_limit.min(budget), lookups.mx(&domain)),
    );
    Ok(Detection {
        found: found
            .ok()
            .flatten()
            .unwrap_or_else(|| plan::guess(&domain, email)),
        hosting: hosting::hosting_of(&records.unwrap_or_default()),
    })
}

/// The config files, then SRV; None when neither settles on a pair.
/// The lookups of each step run at once, so one that waits on a timeout
/// doesn't hold up the others; the files still count in their order.
/// The cost: when the ISPDB answers, the provider and well-known hosts
/// have been asked too (only the domain goes out either way), and their
/// answers are dropped.
async fn find(domain: &str, email: &str, lookups: &impl Lookups) -> Option<Found> {
    let mut files: FuturesOrdered<_> = plan::config_urls(domain)
        .into_iter()
        .map(|(source, url)| async move { (source, lookups.fetch(&url).await) })
        .collect();
    while let Some((source, body)) = files.next().await {
        let Some(body) = body else {
            continue;
        };
        if let Some(found) = xml::parse(&body, email).and_then(|c| plan::best(&c, source, domain)) {
            return Some(found);
        }
    }
    let answers = futures::future::join_all(plan::srv_queries(domain).into_iter().map(
        |(name, service, security)| async move { (service, security, lookups.srv(&name).await) },
    ))
    .await;
    let mut candidates = Candidates::default();
    for (service, security, records) in answers {
        let settings = plan::srv_candidates(records, security, email);
        match service {
            Service::Imap => candidates.imap.extend(settings),
            Service::Smtp => candidates.smtp.extend(settings),
        }
    }
    plan::best(&candidates, Source::Srv, domain)
}

#[cfg(test)]
mod tests {
    use hickory_resolver::config::ResolveHosts;

    use super::*;

    #[tokio::test]
    async fn one_resolver_serves_every_query_and_skips_the_hosts_file() {
        let internet = Internet::new(Roots::empty());
        let first = internet.resolver().expect("resolv.conf readable");
        let second = internet.resolver().expect("resolv.conf readable");
        assert!(std::ptr::eq(first, second), "a second resolver was built");
        assert_eq!(first.options().use_hosts_file, ResolveHosts::Never);
    }
}
