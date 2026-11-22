//! DNS MX lookups with the system resolver configuration (#384).

use std::time::Duration;

use hickory_resolver::TokioResolver;
use hickory_resolver::proto::rr::RData;

use super::hosting::MxRecord;
use crate::dns::fqdn;

/// One resolver attempt (4 s) plus margin, so its second attempt is cut
/// off. A lost packet gives "not Microsoft", which only costs today's
/// flow; a dead resolver must not make every Continue wait the full 8 s.
pub const MX_TIMEOUT: Duration = Duration::from_secs(5);

/// The MX records for `name`; empty when there are none or DNS fails
/// (the domain is then "not known to be Microsoft").
pub async fn lookup(resolver: &TokioResolver, name: &str) -> Vec<MxRecord> {
    let Ok(Ok(answer)) = tokio::time::timeout(MX_TIMEOUT, resolver.mx_lookup(fqdn(name))).await
    else {
        return Vec::new();
    };
    answer
        .answers()
        .iter()
        .filter_map(|record| match &record.data {
            RData::MX(mx) => Some(MxRecord {
                preference: mx.preference,
                host: mx.exchange.to_ascii(),
            }),
            _ => None,
        })
        .collect()
}
