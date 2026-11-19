//! Organisational domains and DMARC's relaxed alignment (RFC 7489 3.1.1),
//! over the Public Suffix List including its private section.

use std::net::IpAddr;

/// The organisational domain (public suffix plus one label), lowercased,
/// trailing dot removed; None for an IP literal (v4, or v6 with or without
/// brackets), a single label, an empty string or a public suffix itself.
/// An A-label and its U-label give the same answer.
pub fn org_domain(host: &str) -> Option<String> {
    let host = host.trim().trim_end_matches('.');
    if host.is_empty() {
        return None;
    }
    let bare = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    if bare.parse::<IpAddr>().is_ok() {
        return None;
    }
    let ascii = idna::domain_to_ascii(host).ok()?;
    if !ascii.contains('.') {
        return None;
    }
    let domain = psl::domain(ascii.as_bytes())?;
    std::str::from_utf8(domain.as_bytes())
        .ok()
        .map(str::to_owned)
}

/// DMARC relaxed alignment: both have the same org domain.
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
}
