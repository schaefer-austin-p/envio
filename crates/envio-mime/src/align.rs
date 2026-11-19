//! Organisational domains and DMARC's relaxed alignment (RFC 7489 3.1.1),
//! over the Public Suffix List including its private section.

use std::net::IpAddr;

/// The organisational domain (public suffix plus one label), lowercased,
/// trailing dot removed; None for an IP literal in any common spelling
/// (v4, v6, bracketed, `IPv6:`-tagged, with a `%zone`), a single label, an
/// empty string, a malformed host name or a public suffix itself.
///
/// The answer is for the IDNA-normalised ASCII name (A-labels, lowercase).
/// Anything that then uses the domain, such as the DKIM key lookup in
/// `<selector>._domainkey.<domain>`, must query that normalised name, not the
/// raw text of the header.
pub fn org_domain(host: &str) -> Option<String> {
    let ascii = host_name(host)?;
    let domain = psl::domain(ascii.as_bytes())?;
    std::str::from_utf8(domain.as_bytes())
        .ok()
        .map(str::to_owned)
}

/// The normalised ASCII host name: trimmed, one trailing dot dropped, IDNA
/// applied (lowercase, A-labels), and checked as a plausible DNS name with
/// at least two labels. None for an IP literal in any spelling.
pub(crate) fn host_name(host: &str) -> Option<String> {
    let host = host.trim().trim_end_matches('.');
    if looks_like_ip(host) {
        return None;
    }
    let ascii = idna::domain_to_ascii(host).ok()?;
    // U+3002 and U+FF0E map to '.', so a trailing one shows up only now.
    let ascii = ascii.trim_end_matches('.');
    valid_host(ascii).then(|| ascii.to_owned())
}

/// A domain a DKIM signature may claim (`d=`): a [`host_name`] whose public
/// suffix is on the Public Suffix List. Hosts under an unknown TLD such as
/// `1.2.3.4a` still have an [`org_domain`] (DMARC alignment is a pure string
/// rule), but no signature may use them, so no key lookup is made for them.
pub(crate) fn signing_domain(host: &str) -> Option<String> {
    let ascii = host_name(host)?;
    psl::suffix(ascii.as_bytes())?.is_known().then_some(ascii)
}

/// An IP literal as mail headers spell it: optional brackets, an optional
/// `IPv6:` tag, an optional `%zone`.
fn looks_like_ip(host: &str) -> bool {
    let bare = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    let bare = match bare.get(..5) {
        Some(tag) if tag.eq_ignore_ascii_case("IPv6:") => &bare[5..],
        _ => bare,
    };
    let bare = bare.split('%').next().unwrap_or(bare);
    bare.parse::<IpAddr>().is_ok()
}

/// A plausible DNS name with at least two labels: every label non-empty and
/// only `[a-z0-9-_]`, and a last label with an ASCII letter (so `1.2.3`,
/// `0x7f.1` and other numeric forms are not names).
fn valid_host(ascii: &str) -> bool {
    let mut count = 0;
    let mut last = "";
    for label in ascii.split('.') {
        let ok = !label.is_empty()
            && label
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
        if !ok {
            return false;
        }
        count += 1;
        last = label;
    }
    count >= 2 && last.bytes().any(|b| b.is_ascii_lowercase())
}

/// DMARC relaxed alignment: both have the same org domain. Names are
/// compared in their IDNA-normalised ASCII form (see [`org_domain`]).
pub fn aligned(signing: &str, from: &str) -> bool {
    match (org_domain(signing), org_domain(from)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subdomains_align_with_the_org_domain() {
        assert!(aligned("mail.example.com", "example.com"));
        assert!(aligned("mail.example.com", "news.example.com"));
        assert!(aligned("example.com", "example.com"));
    }

    #[test]
    fn different_orgs_do_not_align() {
        assert!(!aligned("example.com", "example.net"));
        assert!(!aligned("evilexample.com", "example.com"));
    }

    #[test]
    fn multi_label_public_suffix() {
        assert!(aligned("example.co.uk", "mail.example.co.uk"));
        assert!(!aligned("a.co.uk", "b.co.uk"));
        assert_eq!(
            org_domain("x.mail.example.co.uk").as_deref(),
            Some("example.co.uk")
        );
    }

    #[test]
    fn private_section_suffixes_separate_tenants() {
        assert!(!aligned("evil.duckdns.org", "victim.duckdns.org"));
        assert!(!aligned("a.github.io", "b.github.io"));
        assert!(aligned("x.a.github.io", "a.github.io"));
    }

    #[test]
    fn no_org_domain_for_ips_single_labels_and_suffixes() {
        for h in [
            "10.0.0.1",
            "[2001:db8::1]",
            "2001:db8::1",
            "localhost",
            "com",
            "co.uk",
            "",
        ] {
            assert_eq!(org_domain(h), None, "{h:?}");
        }
        assert!(!aligned("10.0.0.1", "10.0.0.1"));
        assert!(!aligned("[2001:db8::1]", "[2001:db8::1]"));
        assert!(!aligned("localhost", "localhost"));
        assert!(!aligned("co.uk", "co.uk"));
        assert!(!aligned("", ""));
        assert!(!aligned("example.com", ""));
    }

    #[test]
    fn case_and_trailing_dot_do_not_matter() {
        assert!(aligned("Example.COM.", "example.com"));
        assert_eq!(
            org_domain("MAIL.Example.Com.").as_deref(),
            Some("example.com")
        );
    }

    #[test]
    fn a_label_and_u_label_are_the_same_domain() {
        assert!(aligned("mail.bücher.example", "xn--bcher-kva.example"));
        assert!(aligned("XN--BCHER-KVA.example.", "bücher.example"));
        assert_eq!(
            org_domain("bücher.example").as_deref(),
            Some("xn--bcher-kva.example")
        );
    }

    #[test]
    fn ip_forms_beyond_ipaddr_parse_have_no_org_domain() {
        for h in [
            "[IPv6:::ffff:1.2.3.4]",
            "IPv6:::ffff:1.2.3.4",
            "1.2.3.4%eth0",
            "::ffff:1.2.3.4%eth0",
            "[fe80::1%eth0]",
            "1.2.3",
            "999.1.2.3",
            "1.2.3.4.5",
            "0x7f.1",
            "[::1].",
            "[::1]",
        ] {
            assert_eq!(org_domain(h), None, "{h:?}");
            assert!(!aligned(h, h), "{h:?}");
        }
        assert!(!aligned("[IPv6:::ffff:1.2.3.4]", "[IPv6:::ffff:5.6.3.4]"));
    }

    #[test]
    fn malformed_hostnames_have_no_org_domain() {
        for h in [
            "a..example.com",
            ".example.com",
            "mail.example.com:25",
            "evil.com@victim.com",
            "exa mple.com",
            "example.com/x",
        ] {
            assert_eq!(org_domain(h), None, "{h:?}");
        }
        assert!(!aligned("evil.com@victim.com", "victim.com"));
        assert!(!aligned("mail.example.com:25", "example.com"));
    }

    #[test]
    fn idna_dots_are_trailing_dots() {
        assert!(aligned("example.com\u{3002}", "example.com"));
        assert!(aligned("example.com\u{ff0e}", "example.com"));
        assert_eq!(
            org_domain("example.com\u{3002}").as_deref(),
            Some("example.com")
        );
    }

    #[test]
    fn psl_wildcard_and_exception_rules() {
        assert_eq!(org_domain("x.a.b.ck").as_deref(), Some("a.b.ck"));
        assert_eq!(org_domain("a.b.ck").as_deref(), Some("a.b.ck"));
        assert_eq!(org_domain("www.ck").as_deref(), Some("www.ck"));
        assert_eq!(org_domain("sub.www.ck").as_deref(), Some("www.ck"));
        assert_eq!(org_domain("b.ck"), None);
        assert_eq!(org_domain("ck"), None);
        assert!(!aligned("a.b.ck", "c.b.ck"));
    }

    #[test]
    fn host_name_is_the_normalised_ascii_name() {
        assert_eq!(
            host_name("Mail.Example.COM.").as_deref(),
            Some("mail.example.com")
        );
        assert_eq!(
            host_name("bücher.example\u{3002}").as_deref(),
            Some("xn--bcher-kva.example")
        );
        for h in [
            "10.0.0.1",
            "[::1]",
            "localhost",
            "a..b.com",
            "a@b.com",
            "b.com:25",
            "",
        ] {
            assert_eq!(host_name(h), None, "{h:?}");
        }
    }

    #[test]
    fn signing_domain_needs_a_known_suffix() {
        assert_eq!(
            signing_domain("Football.Example.com.").as_deref(),
            Some("football.example.com")
        );
        assert_eq!(
            signing_domain("a.github.io").as_deref(),
            Some("a.github.io")
        );
        assert_eq!(
            signing_domain("x.example.co.uk").as_deref(),
            Some("x.example.co.uk")
        );
        // Unknown TLDs: org_domain still answers, signing_domain does not.
        assert_eq!(org_domain("1.2.3.4a").as_deref(), Some("3.4a"));
        for h in [
            "1.2.3.4a",
            "example.invalidtld",
            "host.internal",
            "0x7f.1",
            "[::1]",
        ] {
            assert_eq!(signing_domain(h), None, "{h:?}");
        }
    }
}
