//! DNS lookups with the system resolver configuration: SRV for autoconfig
//! (see `autoconfig::srv`), TXT for DKIM keys. This module never parses
//! what a record holds.

use std::sync::OnceLock;
use std::time::Duration;

use futures::future::BoxFuture;
use hickory_resolver::TokioResolver;
use hickory_resolver::config::{ResolveHosts, ResolverOpts};
use hickory_resolver::net::{DnsError, NetError, NoRecords};
use hickory_resolver::proto::op::ResponseCode;
use hickory_resolver::proto::rr::{Name, RData};

/// Records kept per answer.
const MAX_RECORDS: usize = 8;
/// Bytes of record text kept per answer.
const MAX_BYTES: usize = 8 * 1024;
/// How long one TXT lookup may take.
const TXT_TIMEOUT: Duration = Duration::from_secs(5);

/// What a TXT lookup gave: each record's character-strings joined in
/// order (a DKIM key is split into 255-byte strings), the smallest TTL in
/// seconds; or no such name/record; or a failure (timeout, SERVFAIL, no
/// resolver).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TxtAnswer {
    Records { texts: Vec<String>, ttl: u32 },
    NoRecord,
    Unavailable,
}

/// Looks up the TXT records of a name.
pub trait TxtLookup: Send + Sync {
    fn txt<'a>(&'a self, name: &'a str) -> BoxFuture<'a, TxtAnswer>;
}

/// The name as a fully qualified one. The trailing dot stops the resolver
/// from trying the search domains of resolv.conf.
pub fn fqdn(name: &str) -> String {
    if name.ends_with('.') {
        name.to_owned()
    } else {
        format!("{name}.")
    }
}

/// A resolver from the system's resolv.conf; None when that can't be read
/// (autoconfig then finds no SRV records, TXT lookups are Unavailable).
/// Build one per user of it and reuse it for every query.
pub fn resolver() -> Option<TokioResolver> {
    let mut builder = TokioResolver::builder_tokio().ok()?;
    configure(builder.options_mut());
    builder.build().ok()
}

/// Neither SRV nor TXT records come from /etc/hosts, so the resolver
/// doesn't read it. Reading it costs time and memory in proportion to its
/// size, and an ad-block hosts file on a phone has a million lines (#97).
fn configure(options: &mut ResolverOpts) {
    options.use_hosts_file = ResolveHosts::Never;
}

/// The system resolver (resolv.conf, hosts file off, #97), 5 s timeout.
/// The resolver is built on the first lookup and shared after that.
pub struct SystemTxt {
    resolver: OnceLock<Option<TokioResolver>>,
}

impl SystemTxt {
    pub fn new() -> Self {
        Self {
            resolver: OnceLock::new(),
        }
    }
}

impl Default for SystemTxt {
    fn default() -> Self {
        Self::new()
    }
}

impl TxtLookup for SystemTxt {
    fn txt<'a>(&'a self, name: &'a str) -> BoxFuture<'a, TxtAnswer> {
        Box::pin(async move {
            let Some(resolver) = self.resolver.get_or_init(resolver).as_ref() else {
                return TxtAnswer::Unavailable;
            };
            let Some(query) = query_name(name) else {
                return TxtAnswer::Unavailable;
            };
            match tokio::time::timeout(TXT_TIMEOUT, resolver.txt_lookup(query)).await {
                Ok(Ok(lookup)) => {
                    // The answer holds the CNAME records of the chain too.
                    let raw = lookup
                        .answers()
                        .iter()
                        .filter_map(|record| match &record.data {
                            RData::TXT(txt) => Some((
                                txt.txt_data.iter().map(|s| s.to_vec()).collect::<Vec<_>>(),
                                record.ttl,
                            )),
                            _ => None,
                        });
                    answer_from(raw)
                }
                Ok(Err(error)) => classify(&error),
                Err(_elapsed) => TxtAnswer::Unavailable,
            }
        })
    }
}

/// The name to query: fully qualified, so no search domain is tried.
fn query_name(name: &str) -> Option<Name> {
    Name::from_ascii(fqdn(name)).ok()
}

/// One record's character-strings joined in order; None when the result
/// is not valid UTF-8.
fn join_strings(strings: &[&[u8]]) -> Option<String> {
    // Joined before decoding: a 255-byte cut may fall inside a character.
    String::from_utf8(strings.concat()).ok()
}

/// The answer for the TXT records of a response, each as its
/// character-strings and TTL. Records that are not UTF-8 are skipped; at
/// most 8 records and 8 KiB of text are kept, in order.
fn answer_from(records: impl IntoIterator<Item = (Vec<Vec<u8>>, u32)>) -> TxtAnswer {
    let mut texts = Vec::new();
    let mut bytes = 0usize;
    let mut ttl = u32::MAX;
    for (strings, record_ttl) in records {
        if texts.len() == MAX_RECORDS {
            break;
        }
        let parts: Vec<&[u8]> = strings.iter().map(Vec::as_slice).collect();
        let Some(text) = join_strings(&parts) else {
            continue;
        };
        bytes += text.len();
        if bytes > MAX_BYTES {
            break;
        }
        ttl = ttl.min(record_ttl);
        texts.push(text);
    }
    if texts.is_empty() {
        TxtAnswer::NoRecord
    } else {
        TxtAnswer::Records { texts, ttl }
    }
}

/// What a failed lookup means: NXDOMAIN and NODATA are `NoRecord`,
/// anything else (SERVFAIL, refused, I/O) is `Unavailable`.
fn classify(error: &NetError) -> TxtAnswer {
    match error {
        NetError::Dns(DnsError::NoRecordsFound(NoRecords {
            response_code: ResponseCode::NXDomain | ResponseCode::NoError,
            ..
        })) => TxtAnswer::NoRecord,
        _ => TxtAnswer::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hickory_resolver::proto::op::Query;
    use hickory_resolver::proto::rr::RecordType;

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

    #[test]
    fn the_query_name_is_fully_qualified() {
        let name = query_name("brisbane._domainkey.example.com").unwrap();
        assert!(name.is_fqdn());
        assert_eq!(name.to_ascii(), "brisbane._domainkey.example.com.");
        assert!(query_name("a.example.com.").unwrap().is_fqdn());
    }

    #[test]
    fn strings_are_joined_in_order() {
        assert_eq!(
            join_strings(&[b"v=DKIM1; k=rsa; p=AB", b"CD", b"EF"]).as_deref(),
            Some("v=DKIM1; k=rsa; p=ABCDEF")
        );
        assert_eq!(join_strings(&[b"one"]).as_deref(), Some("one"));
        assert_eq!(join_strings(&[]).as_deref(), Some(""));
    }

    #[test]
    fn invalid_utf8_is_not_joined() {
        assert_eq!(join_strings(&[b"ok", &[0xff, 0xfe]]), None);
    }

    #[test]
    fn a_character_string_cut_inside_a_char_still_joins() {
        // "é" is 0xC3 0xA9; DNS strings are cut at 255 bytes, not at chars.
        assert_eq!(join_strings(&[&[0xc3], &[0xa9]]).as_deref(), Some("é"));
    }

    fn rec(text: &str, ttl: u32) -> (Vec<Vec<u8>>, u32) {
        (vec![text.as_bytes().to_vec()], ttl)
    }

    #[test]
    fn records_keep_their_order_and_the_smallest_ttl() {
        let answer = answer_from([
            (vec![b"a".to_vec(), b"b".to_vec()], 300),
            rec("c", 60),
            rec("d", 900),
        ]);
        assert_eq!(
            answer,
            TxtAnswer::Records {
                texts: vec!["ab".into(), "c".into(), "d".into()],
                ttl: 60
            }
        );
    }

    #[test]
    fn at_most_eight_records_are_kept() {
        let answer = answer_from((0..20u32).map(|i| rec(&format!("r{i}"), 100 + i)));
        let TxtAnswer::Records { texts, ttl } = answer else {
            panic!("expected records");
        };
        assert_eq!(texts.len(), 8);
        assert_eq!(texts[0], "r0");
        assert_eq!(texts[7], "r7");
        assert_eq!(ttl, 100);
        // The TTL is that of the kept records only.
        let answer = answer_from((0..20u32).map(|i| rec("x", 500 - i)));
        assert_eq!(
            answer,
            TxtAnswer::Records {
                texts: vec!["x".into(); 8],
                ttl: 493
            }
        );
    }

    #[test]
    fn exactly_eight_records_are_all_kept() {
        let TxtAnswer::Records { texts, .. } = answer_from((0..8).map(|_| rec("x", 1))) else {
            panic!("expected records");
        };
        assert_eq!(texts.len(), 8);
    }

    #[test]
    fn at_most_8_kib_are_kept() {
        let big = "a".repeat(4096);
        let TxtAnswer::Records { texts, .. } =
            answer_from([rec(&big, 1), rec(&big, 1), rec("b", 1)])
        else {
            panic!("expected records");
        };
        // 4096 + 4096 is exactly the cap; one more byte is over.
        assert_eq!(texts.len(), 2);
        let TxtAnswer::Records { texts, .. } =
            answer_from([rec(&big, 1), rec(&"a".repeat(4097), 1), rec("b", 1)])
        else {
            panic!("expected records");
        };
        assert_eq!(texts, vec![big]);
    }

    #[test]
    fn a_split_record_counts_its_joined_length() {
        let half = vec![b'a'; 4097];
        assert_eq!(
            answer_from([(vec![half.clone(), half], 1)]),
            TxtAnswer::NoRecord
        );
    }

    #[test]
    fn records_that_are_not_utf8_are_skipped() {
        let answer = answer_from([
            (vec![vec![0xff]], 5),
            rec("good", 700),
            (vec![b"x".to_vec(), vec![0xc3]], 9),
        ]);
        assert_eq!(
            answer,
            TxtAnswer::Records {
                texts: vec!["good".into()],
                ttl: 700
            }
        );
    }

    #[test]
    fn skipped_records_do_not_count_against_the_caps() {
        let mut records: Vec<_> = (0..8).map(|_| (vec![vec![0xff]], 1)).collect();
        records.push(rec("last", 1));
        assert_eq!(
            answer_from(records),
            TxtAnswer::Records {
                texts: vec!["last".into()],
                ttl: 1
            }
        );
    }

    #[test]
    fn an_answer_without_usable_records_is_no_record() {
        assert_eq!(answer_from([]), TxtAnswer::NoRecord);
        assert_eq!(answer_from([(vec![vec![0xff]], 1)]), TxtAnswer::NoRecord);
    }

    fn no_records(code: ResponseCode) -> NetError {
        let query = Query::query(Name::from_ascii("a.example.com.").unwrap(), RecordType::TXT);
        NetError::Dns(DnsError::NoRecordsFound(NoRecords::new(query, code)))
    }

    #[test]
    fn nxdomain_and_nodata_are_no_record() {
        assert_eq!(
            classify(&no_records(ResponseCode::NXDomain)),
            TxtAnswer::NoRecord
        );
        assert_eq!(
            classify(&no_records(ResponseCode::NoError)),
            TxtAnswer::NoRecord
        );
    }

    #[test]
    fn other_failures_are_unavailable() {
        assert_eq!(
            classify(&no_records(ResponseCode::ServFail)),
            TxtAnswer::Unavailable
        );
        assert_eq!(
            classify(&NetError::Dns(DnsError::ResponseCode(
                ResponseCode::ServFail
            ))),
            TxtAnswer::Unavailable
        );
        assert_eq!(
            classify(&NetError::Dns(DnsError::ResponseCode(
                ResponseCode::Refused
            ))),
            TxtAnswer::Unavailable
        );
    }
}
