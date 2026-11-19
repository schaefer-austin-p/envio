//! DNS lookups with the system resolver configuration: SRV for autoconfig
//! (see `autoconfig::srv`), TXT for DKIM keys. This module never parses
//! what a record holds.

use std::future::Future;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use hickory_resolver::TokioResolver;
use hickory_resolver::config::{ResolveHosts, ResolverOpts};
use hickory_resolver::net::{DnsError, NetError, NoRecords};
use hickory_resolver::proto::op::ResponseCode;
use hickory_resolver::proto::rr::{Name, RData, Record};

/// Records kept per answer.
const MAX_RECORDS: usize = 8;
/// Bytes of record text kept per answer.
const MAX_BYTES: usize = 8 * 1024;
/// How long one TXT lookup may take in all: it covers every attempt of
/// the resolver (see `ATTEMPT_TIMEOUT`), so the retry and the next
/// nameserver get their turn.
const TXT_TIMEOUT: Duration = Duration::from_secs(10);
/// Hickory's wait for one nameserver answer; with `ATTEMPTS` tries it
/// stays under `TXT_TIMEOUT`.
const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(4);
const ATTEMPTS: usize = 2;
/// The resolver is rebuilt after a failed lookup at most this often: the
/// network may have changed (Wi-Fi to cellular, new resolv.conf), but a
/// dead network must not cost a rebuild per lookup.
const MIN_REBUILD_GAP: Duration = Duration::from_secs(30);

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
    options.timeout = ATTEMPT_TIMEOUT;
    options.attempts = ATTEMPTS;
}

/// The system resolver (resolv.conf, hosts file off, #97), 10 s timeout.
/// The resolver is built on the first lookup. A failed build is retried
/// at the next lookup, and a resolver that gave an `Unavailable` answer is
/// rebuilt (rate-limited) in case the network changed under it.
pub struct SystemTxt {
    pool: Pool<TokioResolver>,
}

/// The resolver of a `SystemTxt` and when to rebuild it. Generic over the
/// resolver, with the builder and the clock injected, so that the tests
/// count builds without a network.
struct Pool<R> {
    build: Box<dyn Fn() -> Option<R> + Send + Sync>,
    clock: Box<dyn Fn() -> Instant + Send + Sync>,
    slot: Mutex<Slot<R>>,
}

struct Slot<R> {
    resolver: Option<R>,
    built_at: Option<Instant>,
    /// The last lookup with this resolver was `Unavailable`.
    failed: bool,
}

/// Whether to build a resolver now. Pure, so the policy is testable.
/// A missing resolver (never built, or the build failed) is built at
/// every call; a present one only after an `Unavailable` answer and not
/// within `MIN_REBUILD_GAP` of the last build.
fn rebuild_due(has_resolver: bool, built_at: Option<Instant>, failed: bool, now: Instant) -> bool {
    if !has_resolver {
        return true;
    }
    failed && built_at.is_none_or(|at| now.saturating_duration_since(at) >= MIN_REBUILD_GAP)
}

impl<R: Clone> Pool<R> {
    fn new(
        build: impl Fn() -> Option<R> + Send + Sync + 'static,
        clock: impl Fn() -> Instant + Send + Sync + 'static,
    ) -> Self {
        Self {
            build: Box::new(build),
            clock: Box::new(clock),
            slot: Mutex::new(Slot {
                resolver: None,
                built_at: None,
                failed: false,
            }),
        }
    }

    /// The slot, also when a thread panicked while holding the lock: the
    /// fields are plain values that are always consistent.
    fn lock(&self) -> std::sync::MutexGuard<'_, Slot<R>> {
        self.slot.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The resolver to use now, rebuilt when the policy says so.
    fn current(&self) -> Option<R> {
        let mut slot = self.lock();
        let now = (self.clock)();
        if rebuild_due(slot.resolver.is_some(), slot.built_at, slot.failed, now) {
            slot.resolver = (self.build)();
            slot.built_at = Some(now);
            slot.failed = false;
        }
        slot.resolver.clone()
    }

    fn mark_failed(&self) {
        self.lock().failed = true;
    }

    /// One TXT lookup: the current resolver (none is `Unavailable`), the
    /// query, and a mark for a rebuild when the answer is `Unavailable`.
    async fn lookup<F, Fut>(&self, name: &str, query: F) -> TxtAnswer
    where
        F: FnOnce(R, Name) -> Fut,
        Fut: Future<Output = Option<Result<Vec<Record>, NetError>>>,
    {
        let Some(resolver) = self.current() else {
            return TxtAnswer::Unavailable;
        };
        let answer = txt_with(name, |name| query(resolver, name)).await;
        if answer == TxtAnswer::Unavailable {
            self.mark_failed();
        }
        answer
    }
}

impl SystemTxt {
    pub fn new() -> Self {
        Self {
            pool: Pool::new(resolver, Instant::now),
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
        Box::pin(self.pool.lookup(name, |resolver, query| async move {
            tokio::time::timeout(TXT_TIMEOUT, resolver.txt_lookup(query))
                .await
                .ok()
                .map(|result| result.map(|lookup| lookup.answers().to_vec()))
        }))
    }
}

/// The lookup around a query function: `query` gets the fully qualified
/// name and returns the answer records, the error, or None on timeout.
async fn txt_with<F, Fut>(name: &str, query: F) -> TxtAnswer
where
    F: FnOnce(Name) -> Fut,
    Fut: Future<Output = Option<Result<Vec<Record>, NetError>>>,
{
    let Some(name) = query_name(name) else {
        return TxtAnswer::Unavailable;
    };
    match query(name).await {
        Some(Ok(records)) => answer_from(records.iter().filter_map(|record| match &record.data {
            // The answer holds the CNAME records of the chain too.
            RData::TXT(txt) => Some((
                txt.txt_data.iter().map(|s| s.to_vec()).collect::<Vec<_>>(),
                record.ttl,
            )),
            _ => None,
        })),
        Some(Err(error)) => classify(&error),
        None => TxtAnswer::Unavailable,
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
///
/// Known limit: hickory also reports `NoRecordsFound` with `NoError` when
/// it gives up on a CNAME chain (a loop or the hop limit), and that cannot
/// be told from NODATA by the error alone. It is accepted: `NoRecord`
/// means "no key", so the message counts as unsigned, never as verified.
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

    // The rebuild policy.
    #[test]
    fn a_missing_resolver_is_built_at_every_call() {
        let now = Instant::now();
        assert!(rebuild_due(false, None, false, now));
        // A failed build leaves no resolver and stamps the time: still due.
        assert!(rebuild_due(false, Some(now), false, now));
    }

    #[test]
    fn a_healthy_resolver_is_kept() {
        let built = Instant::now();
        let later = built + Duration::from_secs(3600);
        assert!(!rebuild_due(true, Some(built), false, later));
    }

    #[test]
    fn an_unavailable_answer_triggers_a_rebuild_after_the_gap() {
        let built = Instant::now();
        assert!(rebuild_due(
            true,
            Some(built),
            true,
            built + MIN_REBUILD_GAP
        ));
        assert!(rebuild_due(true, None, true, built));
    }

    #[test]
    fn rebuilds_are_rate_limited() {
        let built = Instant::now();
        let soon = built + MIN_REBUILD_GAP - Duration::from_millis(1);
        assert!(!rebuild_due(true, Some(built), true, soon));
    }

    #[test]
    fn the_attempts_fit_inside_the_outer_timeout() {
        let mut options = ResolverOpts::default();
        configure(&mut options);
        assert_eq!(options.timeout, ATTEMPT_TIMEOUT);
        assert_eq!(options.attempts, ATTEMPTS);
        // With room to spare for scheduling, not just equal.
        assert!(options.timeout * options.attempts as u32 + Duration::from_secs(1) <= TXT_TIMEOUT);
        assert!(options.attempts >= 2, "a retry must be possible");
    }

    // The glue around the resolver call.
    fn txt_record(text: &str, ttl: u32) -> Record {
        Record::from_rdata(
            Name::from_ascii("k.example.com.").unwrap(),
            ttl,
            RData::TXT(hickory_resolver::proto::rr::rdata::TXT::new(vec![
                text.into(),
            ])),
        )
    }

    fn cname_record() -> Record {
        Record::from_rdata(
            Name::from_ascii("sel.example.com.").unwrap(),
            3600,
            RData::CNAME(hickory_resolver::proto::rr::rdata::CNAME(
                Name::from_ascii("k.example.com.").unwrap(),
            )),
        )
    }

    #[tokio::test]
    async fn a_timeout_is_unavailable() {
        let answer = txt_with("a.example.com", |_| async { None }).await;
        assert_eq!(answer, TxtAnswer::Unavailable);
    }

    #[tokio::test]
    async fn a_cname_chain_with_a_txt_record_gives_records() {
        let answer = txt_with("sel.example.com", |name| async move {
            assert_eq!(name.to_ascii(), "sel.example.com.");
            Some(Ok(vec![cname_record(), txt_record("v=DKIM1; p=AB", 120)]))
        })
        .await;
        assert_eq!(
            answer,
            TxtAnswer::Records {
                texts: vec!["v=DKIM1; p=AB".into()],
                ttl: 120
            }
        );
    }

    #[tokio::test]
    async fn an_answer_with_only_a_cname_is_no_record() {
        let answer = txt_with("sel.example.com", |_| async {
            Some(Ok(vec![cname_record()]))
        })
        .await;
        assert_eq!(answer, TxtAnswer::NoRecord);
    }

    #[tokio::test]
    async fn a_bad_name_is_unavailable_and_never_queried() {
        let long = format!("{}.example.com", "a".repeat(64));
        let answer = txt_with(&long, |_| async {
            panic!("a bad name must not reach the resolver");
        })
        .await;
        assert_eq!(answer, TxtAnswer::Unavailable);
    }

    #[tokio::test]
    async fn a_lookup_error_goes_through_classify() {
        let nx = txt_with("a.example.com", |_| async {
            Some(Err(no_records(ResponseCode::NXDomain)))
        })
        .await;
        assert_eq!(nx, TxtAnswer::NoRecord);
        let fail = txt_with("a.example.com", |_| async {
            Some(Err(NetError::Dns(DnsError::ResponseCode(
                ResponseCode::ServFail,
            ))))
        })
        .await;
        assert_eq!(fail, TxtAnswer::Unavailable);
    }

    // The pool around the resolver, with a counting builder and a clock
    // the test moves. The "resolver" is the number of the build.
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Rig {
        pool: Pool<usize>,
        builds: Arc<AtomicUsize>,
        now: Arc<Mutex<Instant>>,
        queries: Arc<Mutex<Vec<usize>>>,
    }

    impl Rig {
        /// `builds_ok`: whether the builder succeeds.
        fn new(builds_ok: bool) -> Self {
            let builds = Arc::new(AtomicUsize::new(0));
            let now = Arc::new(Mutex::new(Instant::now()));
            let (b, c) = (builds.clone(), now.clone());
            let pool = Pool::new(
                move || {
                    let n = b.fetch_add(1, Ordering::SeqCst) + 1;
                    builds_ok.then_some(n)
                },
                move || *c.lock().unwrap(),
            );
            Self {
                pool,
                builds,
                now,
                queries: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn advance(&self, by: Duration) {
            *self.now.lock().unwrap() += by;
        }

        fn builds(&self) -> usize {
            self.builds.load(Ordering::SeqCst)
        }

        /// A lookup whose query answers `result` (None: timeout) and
        /// records which resolver it ran on.
        async fn lookup(&self, result: Option<Result<Vec<Record>, NetError>>) -> TxtAnswer {
            let queries = self.queries.clone();
            self.pool
                .lookup("a.example.com", |resolver, _| async move {
                    queries.lock().unwrap().push(resolver);
                    result
                })
                .await
        }

        async fn ok(&self) -> TxtAnswer {
            self.lookup(Some(Ok(vec![txt_record("v=DKIM1", 60)]))).await
        }

        async fn timeout(&self) -> TxtAnswer {
            self.lookup(None).await
        }
    }

    #[tokio::test]
    async fn the_resolver_is_built_once_while_lookups_work() {
        let rig = Rig::new(true);
        for _ in 0..3 {
            assert!(matches!(rig.ok().await, TxtAnswer::Records { .. }));
        }
        assert_eq!(rig.builds(), 1);
        assert_eq!(*rig.queries.lock().unwrap(), vec![1, 1, 1]);
    }

    #[tokio::test]
    async fn a_nxdomain_does_not_mark_the_resolver_failed() {
        let rig = Rig::new(true);
        rig.lookup(Some(Err(no_records(ResponseCode::NXDomain))))
            .await;
        rig.advance(MIN_REBUILD_GAP * 2);
        rig.ok().await;
        assert_eq!(rig.builds(), 1);
    }

    #[tokio::test]
    async fn after_a_failure_the_next_call_rebuilds_once() {
        let rig = Rig::new(true);
        assert_eq!(rig.timeout().await, TxtAnswer::Unavailable);
        rig.advance(MIN_REBUILD_GAP);
        assert!(matches!(rig.ok().await, TxtAnswer::Records { .. }));
        assert_eq!(rig.builds(), 2);
        // The new resolver ran the second query, and a success after it
        // does not rebuild again.
        rig.advance(MIN_REBUILD_GAP * 2);
        rig.ok().await;
        assert_eq!(rig.builds(), 2);
        assert_eq!(*rig.queries.lock().unwrap(), vec![1, 2, 2]);
    }

    #[tokio::test]
    async fn no_rebuild_within_the_gap_of_the_last_build() {
        let rig = Rig::new(true);
        rig.timeout().await;
        rig.advance(MIN_REBUILD_GAP - Duration::from_secs(1));
        rig.timeout().await;
        rig.advance(Duration::from_secs(5));
        rig.timeout().await;
        // Only the lookups before the gap ran on build 1; the failure is
        // still marked, so the first call past the gap rebuilds.
        assert_eq!(rig.builds(), 2);
        assert_eq!(*rig.queries.lock().unwrap(), vec![1, 1, 2]);
    }

    #[tokio::test]
    async fn a_missing_resolver_is_rebuilt_on_every_call() {
        let rig = Rig::new(false);
        for expected in 1..=3 {
            assert_eq!(rig.ok().await, TxtAnswer::Unavailable);
            assert_eq!(rig.builds(), expected);
        }
        // No time passes, and no query ran.
        assert!(rig.queries.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_poisoned_lock_does_not_panic() {
        let rig = Rig::new(true);
        rig.ok().await;
        std::thread::scope(|scope| {
            let handle = scope.spawn(|| {
                let _guard = rig.pool.slot.lock().unwrap();
                panic!("poison the lock");
            });
            assert!(handle.join().is_err());
        });
        assert!(rig.pool.slot.is_poisoned());
        assert!(matches!(rig.ok().await, TxtAnswer::Records { .. }));
        assert_eq!(rig.timeout().await, TxtAnswer::Unavailable);
        rig.advance(MIN_REBUILD_GAP);
        assert!(matches!(rig.ok().await, TxtAnswer::Records { .. }));
        assert_eq!(rig.builds(), 2);
    }
}
