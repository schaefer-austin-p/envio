//! DNS SRV lookups with the system resolver configuration.

use hickory_resolver::TokioResolver;
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

/// The SRV records for `name`; empty when there are none or DNS fails
/// (autoconfig then moves on to the next step).
pub async fn lookup(name: &str) -> Vec<SrvRecord> {
    let Ok(builder) = TokioResolver::builder_tokio() else {
        return Vec::new();
    };
    let Ok(resolver) = builder.build() else {
        return Vec::new();
    };
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
}
