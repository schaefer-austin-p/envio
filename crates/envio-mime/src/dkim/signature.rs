//! The `DKIM-Signature` header (RFC 6376 3.5).

use super::super::align::{MAX_LABEL, MAX_NAME, host_name, signing_domain};
use super::tags::{base64_bytes, colon_list, tag_list};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Algorithm {
    RsaSha256,
    Ed25519Sha256,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Canon {
    Simple,
    Relaxed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Signature {
    pub algorithm: Algorithm,
    pub header_canon: Canon,
    pub body_canon: Canon,
    /// `d=`, the normalised ASCII name (see `align::signing_domain`).
    pub domain: String,
    /// `s=`, lowercased.
    pub selector: String,
    /// `h=`, lowercased, in order.
    pub signed_headers: Vec<String>,
    /// `bh=`, decoded.
    pub body_hash: Vec<u8>,
    /// `b=`, decoded.
    pub signature: Vec<u8>,
    /// `l=`
    pub body_length: Option<u64>,
    /// `i=`
    pub agent: Option<String>,
    /// `x=`
    pub expires: Option<u64>,
}

impl Signature {
    /// `content-type` is in `h=` (decision 9; checked by T5, not here).
    pub fn signs_content_type(&self) -> bool {
        self.signed_headers.iter().any(|h| h == "content-type")
    }
}

/// Digits only, as RFC 6376 writes `l=`, `x=` and `t=`.
fn number(value: &str) -> Option<u64> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

/// An optional numeric tag: absent is `Ok(None)`, malformed is `Err`.
fn optional_number(value: Option<&str>) -> Result<Option<u64>, ()> {
    match value {
        None => Ok(None),
        Some(v) => number(v).map(Some).ok_or(()),
    }
}

fn canon(name: &str) -> Option<Canon> {
    match name {
        "simple" => Some(Canon::Simple),
        "relaxed" => Some(Canon::Relaxed),
        _ => None,
    }
}

/// `c=`: header and body canonicalisation; the body defaults to simple.
fn canon_pair(value: Option<&str>) -> Option<(Canon, Canon)> {
    match value {
        None => Some((Canon::Simple, Canon::Simple)),
        Some(c) => match c.split_once('/') {
            Some((h, b)) => Some((canon(h)?, canon(b)?)),
            None => Some((canon(c)?, Canon::Simple)),
        },
    }
}

/// `h=`: lowercased header names, which must include `from`.
fn header_names(value: &str) -> Option<Vec<String>> {
    let names: Vec<String> = colon_list(value).map(str::to_ascii_lowercase).collect();
    let well_formed = names
        .iter()
        .all(|h| !h.is_empty() && h.bytes().all(|b| b.is_ascii_graphic()));
    (well_formed && names.iter().any(|h| h == "from")).then_some(names)
}

/// `i=`: `[local]@host` with exactly one `@` (`host_name` rejects a second), the host a name equal to `d`
/// or under it. Returns the address with the host normalised.
fn agent_inside(agent: &str, domain: &str) -> Option<String> {
    let (local, host) = agent.split_once('@')?;
    let host = host_name(host)?;
    let inside = host == domain || host.ends_with(&format!(".{domain}"));
    inside.then(|| format!("{local}@{host}"))
}

/// A selector is a DNS name in the key lookup, so it must be a plain one.
fn selector(value: &str) -> Option<String> {
    let s = value.to_ascii_lowercase();
    let ok = s.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= MAX_LABEL
            && label
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    });
    ok.then_some(s)
}

/// None when a required tag is missing, `v` is not 1, `a` is unsupported
/// (rsa-sha1 included), `h` lacks `from`, `i` is outside `d`, or a value
/// does not decode. (`h` lacking `content-type` still parses; T5's
/// candidate filter rejects it, so the RFC 8463 vectors stay usable.)
pub(crate) fn signature(header_value: &str) -> Option<Signature> {
    let tags = tag_list(header_value)?;
    let get = |name: &str| {
        tags.iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    };
    if get("v")? != "1" {
        return None;
    }
    let algorithm = match get("a")? {
        "rsa-sha256" => Algorithm::RsaSha256,
        "ed25519-sha256" => Algorithm::Ed25519Sha256,
        _ => return None,
    };
    let (header_canon, body_canon) = canon_pair(get("c"))?;
    if let Some(q) = get("q")
        && !colon_list(q).any(|m| m == "dns/txt")
    {
        return None;
    }
    // The normalised name: the key lookup queries exactly this.
    let domain = signing_domain(get("d")?)?;
    let selector = selector(get("s")?)?;
    // The key lookup name `<s>._domainkey.<d>` must fit a DNS name.
    if selector.len() + "._domainkey.".len() + domain.len() > MAX_NAME {
        return None;
    }
    let signed_headers = header_names(get("h")?)?;
    let body_hash = base64_bytes(get("bh")?).filter(|b| !b.is_empty())?;
    let signature = base64_bytes(get("b")?).filter(|b| !b.is_empty())?;
    let body_length = optional_number(get("l")).ok()?;
    let expires = optional_number(get("x")).ok()?;
    let agent = match get("i") {
        Some(i) => Some(agent_inside(i, &domain)?),
        None => None,
    };
    Some(Signature {
        algorithm,
        header_canon,
        body_canon,
        domain,
        selector,
        signed_headers,
        body_hash,
        signature,
        body_length,
        agent,
        expires,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// RFC 8463 A.3, header values only.
    const ED25519: &str = "v=1; a=ed25519-sha256; c=relaxed/relaxed;\r\n d=football.example.com; i=@football.example.com;\r\n q=dns/txt; s=brisbane; t=1528637909; h=from : to :\r\n subject : date : message-id : from : subject : date;\r\n bh=2jUSOH9NhtVGCQWNr9BrIAPreKQjO6Sn7XIkfJVOzv8=;\r\n b=/gCrinpcQOoIfuHNQIbq4pgh9kyIK3AQUdt9OdqQehSwhEIug4D11Bus\r\n Fa3bT3FY5OsU7ZbnKELq+eXdp1Q1Dw==";
    const RSA: &str = "v=1; a=rsa-sha256; c=relaxed/relaxed;\r\n d=football.example.com; i=@football.example.com;\r\n q=dns/txt; s=test; t=1528637909; h=from : to : subject :\r\n date : message-id : from : subject : date;\r\n bh=2jUSOH9NhtVGCQWNr9BrIAPreKQjO6Sn7XIkfJVOzv8=;\r\n b=F45dVWDfMbQDGHJFlXUNB2HKfbCeLRyhDXgFpEL8GwpsRe0IeIixNTe3\r\n DhCVlUrSjV4BwcVcOF6+FF3Zo9Rpo1tFOeS9mPYQTnGdaSGsgeefOsk2Jz\r\n dA+L10TeYt9BgDfQNZtKdN1WO//KgIqXP7OdEFE4LjFYNcUxZQ4FADY+8=";

    #[test]
    fn the_rfc_8463_signatures_parse() {
        let ed = signature(ED25519).unwrap();
        assert_eq!(ed.algorithm, Algorithm::Ed25519Sha256);
        assert_eq!(
            (ed.header_canon, ed.body_canon),
            (Canon::Relaxed, Canon::Relaxed)
        );
        assert_eq!(ed.domain, "football.example.com");
        assert_eq!(ed.selector, "brisbane");
        assert_eq!(
            ed.signed_headers,
            [
                "from",
                "to",
                "subject",
                "date",
                "message-id",
                "from",
                "subject",
                "date"
            ]
        );
        assert_eq!(ed.body_hash.len(), 32);
        assert_eq!(ed.signature.len(), 64);
        assert_eq!(ed.agent.as_deref(), Some("@football.example.com"));
        assert_eq!((ed.body_length, ed.expires), (None, None));
        assert!(!ed.signs_content_type());

        let rsa = signature(RSA).unwrap();
        assert_eq!(rsa.algorithm, Algorithm::RsaSha256);
        assert_eq!(rsa.selector, "test");
        assert_eq!(rsa.signature.len(), 128);
    }

    fn with(replace: &str, by: &str) -> String {
        assert!(RSA.contains(replace), "{replace}");
        RSA.replacen(replace, by, 1)
    }

    #[test]
    fn unsupported_or_malformed_signatures_are_none() {
        assert!(signature(&with("a=rsa-sha256", "a=rsa-sha1")).is_none());
        assert!(signature(&with("a=rsa-sha256", "a=ed25519-sha1")).is_none());
        assert!(signature(&with("v=1", "v=2")).is_none());
        assert!(
            signature(&with(
                "bh=2jUSOH9NhtVGCQWNr9BrIAPreKQjO6Sn7XIkfJVOzv8=;",
                ""
            ))
            .is_none()
        );
        assert!(
            signature(&with(
                "bh=2jUSOH9NhtVGCQWNr9BrIAPreKQjO6Sn7XIkfJVOzv8=",
                "bh=!!"
            ))
            .is_none()
        );
        assert!(signature(&with("d=football.example.com;", "")).is_none());
        assert!(signature(&with("s=test;", "")).is_none());
        assert!(signature(&with("s=test", "s=a b")).is_none());
        assert!(signature(&with("c=relaxed/relaxed", "c=fancy")).is_none());
        assert!(signature(&with("q=dns/txt", "q=http")).is_none());
        assert!(signature(&with("t=1528637909;", "t=1528637909; x=soon;")).is_none());
        assert!(signature(&with("t=1528637909;", "t=1528637909; l=-1;")).is_none());
    }

    #[test]
    fn d_must_be_a_real_name_under_a_known_suffix() {
        // d= must be a real name under a known suffix (no `i=`, so that alone
        // cannot be what rejects it).
        let no_i = with(" i=@football.example.com;", "");
        assert!(signature(&no_i).is_some());
        for d in [
            "10.0.0.1",
            "host.invalidtld",
            "1.2.3.4a",
            "a..com",
            "localhost",
        ] {
            let sig = no_i.replace("d=football.example.com", &format!("d={d}"));
            assert!(signature(&sig).is_none(), "{d}");
        }
    }

    #[test]
    fn h_without_from_is_none_and_without_content_type_is_some() {
        let h = "h=from : to : subject :\r\n date : message-id : from : subject : date";
        assert!(signature(&with(h, "h=subject:date")).is_none());
        let sig = signature(&with(h, "h=from:subject")).unwrap();
        assert!(!sig.signs_content_type());
        let sig = signature(&with(h, "h=From : Content-Type")).unwrap();
        assert!(sig.signs_content_type());
        assert_eq!(sig.signed_headers, ["from", "content-type"]);
    }

    #[test]
    fn the_agent_must_be_inside_the_domain() {
        let i = "i=@football.example.com";
        assert!(signature(&with(i, "i=@other.example")).is_none());
        assert!(signature(&with(i, "i=@notfootball.example.com")).is_none());
        assert!(signature(&with(i, "i=@sub.football.example.com")).is_some());
        assert!(signature(&with(i, "i=joe@football.example.com")).is_some());
        assert!(signature(&with(i, "i=joe@Football.Example.COM.")).is_some());
        assert!(signature(&with(i, "i=noat")).is_none());
        assert!(signature(&with(i, "i=@")).is_none());
    }

    #[test]
    fn optional_tags_and_defaults() {
        let sig = signature(&with("c=relaxed/relaxed", "c=relaxed")).unwrap();
        assert_eq!(
            (sig.header_canon, sig.body_canon),
            (Canon::Relaxed, Canon::Simple)
        );
        let sig = signature(&with("c=relaxed/relaxed;", "")).unwrap();
        assert_eq!(
            (sig.header_canon, sig.body_canon),
            (Canon::Simple, Canon::Simple)
        );
        let sig = signature(&with("c=relaxed/relaxed", "c=simple/relaxed")).unwrap();
        assert_eq!(
            (sig.header_canon, sig.body_canon),
            (Canon::Simple, Canon::Relaxed)
        );
        let sig = signature(&with("t=1528637909;", "t=1528637909; x=1528640000; l=120;")).unwrap();
        assert_eq!(
            (sig.expires, sig.body_length),
            (Some(1_528_640_000), Some(120))
        );
        let sig = signature(&with("d=football.example.com", "d=Football.Example.COM.")).unwrap();
        assert_eq!(sig.domain, "football.example.com");
        let sig = signature(&with("s=test", "s=Mail.Test_1")).unwrap();
        assert_eq!(sig.selector, "mail.test_1");
    }

    #[test]
    fn empty_bh_and_b_are_none() {
        let bh = "bh=2jUSOH9NhtVGCQWNr9BrIAPreKQjO6Sn7XIkfJVOzv8=";
        assert!(signature(&with(bh, "bh=")).is_none());
        assert!(signature(&with(bh, "bh= \r\n ")).is_none());
        let at = RSA.rfind("b=").unwrap();
        assert!(signature(&format!("{}b=", &RSA[..at])).is_none());
        assert!(signature(&format!("{}b= ;", &RSA[..at])).is_none());
        assert!(signature(RSA).is_some());
    }

    #[test]
    fn the_agent_is_stored_normalised_and_has_one_at() {
        let i = "i=@football.example.com";
        let sig = signature(&with(i, "i=joe@Football.Example.COM.")).unwrap();
        assert_eq!(sig.agent.as_deref(), Some("joe@football.example.com"));
        for bad in [
            "i=a@b@football.example.com",
            "i=@@football.example.com",
            "i=joe@football..example.com",
            "i=joe@football.example.com:25",
            "i=joe@10.0.0.1",
        ] {
            assert!(signature(&with(i, bad)).is_none(), "{bad}");
        }
    }

    #[test]
    fn dns_length_limits_apply_to_d_and_s() {
        let no_i = with(" i=@football.example.com;", "");
        let l63 = "a".repeat(63);
        let l64 = "a".repeat(64);
        let d = |d: &str| no_i.replace("d=football.example.com", &format!("d={d}"));
        let s = |s: &str| no_i.replace("s=test", &format!("s={s}"));
        assert!(signature(&d(&format!("{l63}.example.com"))).is_some());
        assert!(signature(&d(&format!("{l64}.example.com"))).is_none());
        assert!(signature(&s(&l63)).is_some());
        assert!(signature(&s(&l64)).is_none());
        // The whole query name `<s>._domainkey.<d>` is at most 253 bytes.
        let dom = format!("{l63}.{l63}.{l63}.com"); // 194
        let room = 253 - "._domainkey.".len() - dom.len(); // 47
        assert!(
            signature(
                &no_i
                    .replace("d=football.example.com", &format!("d={dom}"))
                    .replace("s=test", &format!("s={}", "s".repeat(room)))
            )
            .is_some()
        );
        assert!(
            signature(
                &no_i
                    .replace("d=football.example.com", &format!("d={dom}"))
                    .replace("s=test", &format!("s={}", "s".repeat(room + 1)))
            )
            .is_none()
        );
    }

    #[test]
    fn the_tag_list_cap_applies_to_signatures() {
        let pad = |n: usize| format!("{RSA}; z={}", "x".repeat(n));
        let base = pad(0).len();
        assert!(signature(&pad(16384 - base)).is_some());
        assert_eq!(pad(16385 - base).len(), 16385);
        assert!(signature(&pad(16385 - base)).is_none());
    }

    #[test]
    fn the_worst_case_input_is_fast() {
        let mut s = String::from(RSA);
        let mut i = 0;
        while s.len() + 8 <= 16 * 1024 {
            s.push_str(&format!("; t{i}="));
            i += 1;
        }
        let started = std::time::Instant::now();
        assert!(signature(&s).is_some());
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }

    proptest! {
        #[test]
        fn signature_never_panics(s in any::<String>()) {
            let _ = signature(&s);
        }

        #[test]
        fn signature_never_panics_on_tag_soup(
            s in "(v=1|a=rsa-sha256|d=a\\.example\\.com|s=x|h=from|bh=AAAA|b=AAAA|i=@a\\.example\\.com|l=[0-9]{1,30}|x=[0-9]{1,30}|c=[a-z/]{0,15}|;| |\\r\\n|=){0,40}",
        ) {
            let _ = signature(&s);
        }

        /// Whatever parses holds every invariant the verifier relies on.
        #[test]
        fn a_parse_implies_the_invariants(
            a in prop::sample::select(vec!["rsa-sha256", "ed25519-sha256", "rsa-sha1", "x"]),
            d in prop::sample::select(vec![
                "football.example.com", "Football.Example.COM.", "example.com", "co.uk",
                "github.io", "10.0.0.1", "a.b.invalidtld", "bücher.example",
            ]),
            h in prop::sample::select(vec![
                "from:to", "to:subject", "From : Content-Type", "", "from::to", "subject",
            ]),
            i in prop::sample::select(vec![
                "", " i=@football.example.com;", " i=a@b@example.com;", " i=x@other.example;",
                " i=joe@sub.football.example.com;", " i=@xn--bcher-kva.example;",
            ]),
            l in prop::sample::select(vec!["", " l=5;", " l=x;"]),
            c in prop::sample::select(vec!["", " c=relaxed/simple;", " c=bad;"]),
        ) {
            let header = format!(
                "v=1; a={a}; d={d};{i}{l}{c} s=sel; h={h}; bh=AAAA; b=BBBB"
            );
            if let Some(sig) = signature(&header) {
                prop_assert!(["rsa-sha256", "ed25519-sha256"].contains(&a));
                prop_assert!(sig.signed_headers.iter().any(|x| x == "from"));
                prop_assert!(sig.signed_headers.iter().all(|x| !x.is_empty()));
                let again = signing_domain(&sig.domain);
                prop_assert_eq!(again.as_deref(), Some(sig.domain.as_str()));
                prop_assert!(psl::domain(sig.domain.as_bytes()).is_some());
                prop_assert!(!sig.body_hash.is_empty() && !sig.signature.is_empty());
                if let Some(agent) = &sig.agent {
                    prop_assert_eq!(agent.matches('@').count(), 1);
                    let host = agent.rsplit('@').next().unwrap();
                    prop_assert!(agent_inside(agent, &sig.domain).is_some());
                    let under = format!(".{}", sig.domain);
                    prop_assert!(host == sig.domain || host.ends_with(&under));
                    prop_assert_eq!(host, host.to_ascii_lowercase());
                }
            }
        }
    }
}
