//! DNS SRV lookups with the system resolver configuration.

use hickory_resolver::TokioResolver;
use hickory_resolver::proto::rr::RData;

use super::plan::SrvRecord;
use crate::connect;

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
        tokio::time::timeout(connect::CONNECT_TIMEOUT, resolver.srv_lookup(name)).await
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
