//! DNS SRV lookups with the system resolver configuration.

use hickory_resolver::TokioResolver;
use hickory_resolver::config::{ResolveHosts, ResolverOpts};
use hickory_resolver::proto::rr::RData;

use super::plan::SrvRecord;
use crate::connect;

/// The name as a fully qualified one. The trailing dot stops the resolver
/// from trying the search domains of resolv.conf.
pub fn fqdn(name: &str) -> String {
    if name.ends_with('.') {
        name.to_owned()
    } else {
        format!("{name}.")
    }
}

/// A resolver for SRV lookups, from the system's resolv.conf; None when
/// that can't be read (autoconfig then finds no SRV records). Build one
/// per detect and reuse it for every query.
pub fn resolver() -> Option<TokioResolver> {
    let mut builder = TokioResolver::builder_tokio().ok()?;
    configure(builder.options_mut());
    builder.build().ok()
}

/// SRV records never come from /etc/hosts, so the resolver doesn't read
/// it. Reading it costs time and memory in proportion to its size, and an
/// ad-block hosts file on a phone has a million lines (#97).
fn configure(options: &mut ResolverOpts) {
    options.use_hosts_file = ResolveHosts::Never;
}

/// The SRV records for `name`; empty when there are none or DNS fails
/// (autoconfig then moves on to the next step).
pub async fn lookup(resolver: &TokioResolver, name: &str) -> Vec<SrvRecord> {
    let Ok(Ok(answer)) =
        tokio::time::timeout(connect::CONNECT_TIMEOUT, resolver.srv_lookup(fqdn(name))).await
    else {
        return Vec::new();
    };
    answer
        .answers()
        .iter()
        .filter_map(|record| match &record.data {
            RData::SRV(srv) => Some(SrvRecord {
                priority: srv.priority,
                weight: srv.weight,
                port: srv.port,
                target: srv.target.to_ascii(),
            }),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_fully_qualified_once() {
        assert_eq!(fqdn("_imaps._tcp.example.org"), "_imaps._tcp.example.org.");
        assert_eq!(fqdn("_imaps._tcp.example.org."), "_imaps._tcp.example.org.");
    }

    #[test]
    fn the_hosts_file_is_never_read() {
        let mut options = ResolverOpts::default();
        configure(&mut options);
        assert_eq!(options.use_hosts_file, ResolveHosts::Never);
    }
}
