//! Whether the receiving server vouched for the From address, read from its
//! `Authentication-Results` header (RFC 8601). This is the provider fast
//! path, one of two ways a sender is verified; the other is DKIM checked in
//! the app (`dkim`). The fast path believes the header only for a short
//! table of providers, matched by exact IMAP host (plan #285, decision 4);
//! every other host, self-hosted, IP-address and Microsoft ones included,
//! verifies by DKIM or not at all.

use crate::align::aligned;
use crate::from_field::single_from_domain;

/// Providers the fast path trusts: (exact IMAP host, authserv-id suffixes
/// that stamp for it). Each entry needs a header captured from that
/// provider. The one place to extend; Microsoft is deliberately absent.
const PROVIDERS: &[(&str, &[&str])] = &[
    ("imap.gmail.com", &["mx.google.com"]),
    ("imap.fastmail.com", &["messagingengine.com"]),
    ("imap.mail.me.com", &["icloud.com", "apple.com"]),
    ("imap.mail.yahoo.com", &["yahoo.com"]),
];

/// The sender's domain was authenticated by the account's own provider: the
/// message has exactly one `From:` mailbox whose domain is the one of
/// `from`, `imap_host` is in [`PROVIDERS`], and the topmost
/// `Authentication-Results` header, with an authserv-id of that provider,
/// holds a `dmarc=pass`, or a `dkim=pass` or `spf=pass` for a domain that
/// aligns with it (relaxed, under the public suffix list). Headers below the
/// topmost may come from the sender and are never read, so a topmost header
/// with an untrusted or missing id leaves the message unverified.
pub fn sender_verified(raw: &[u8], from: &str, imap_host: &str) -> bool {
    let Some(domain) = from.trim().rsplit_once('@').map(|(_, d)| d.to_lowercase()) else {
        return false;
    };
    if single_from_domain(raw).as_deref() != Some(domain.as_str()) {
        return false;
    }
    let Some(header) = results_of(raw, imap_host) else {
        return false;
    };
    split_outside_quotes(&header, |c| c == ';')
        .iter()
        .skip(1)
        .any(|clause| passes(clause, &domain))
}

/// Whether an authserv-id is one of the ids that stamp for `imap_host`: it
/// equals a listed id or ends with `.` plus it. Hosts compare
/// case-insensitively and without a trailing dot.
fn trusted_id(id: &str, imap_host: &str) -> bool {
    let id = id
        .trim()
        .trim_matches('"')
        .trim_end_matches('.')
        .to_lowercase();
    let host = imap_host.trim().trim_end_matches('.');
    if id.is_empty() || host.is_empty() {
        return false;
    }
    PROVIDERS
        .iter()
        .filter(|(h, _)| h.eq_ignore_ascii_case(host))
        .flat_map(|(_, ids)| ids.iter())
        .any(|listed| {
            id == *listed
                || id
                    .strip_suffix(listed)
                    .is_some_and(|head| head.ends_with('.'))
        })
}

/// The comment-free value of the topmost `Authentication-Results` header,
/// when its authserv-id is trusted for `imap_host`. Fail closed: if the
/// topmost header has an untrusted or missing id (Exchange Online and
/// Outlook.com write none), there is no result, and the headers below it
/// are never read, since the sender may have written them.
fn results_of(raw: &[u8], imap_host: &str) -> Option<String> {
    let value = strip_comments(&headers(raw).into_iter().next()?);
    let id = split_outside_quotes(&value, |c| c == ';')
        .first()
        .and_then(|first| first.split_whitespace().next().map(str::to_owned))?;
    trusted_id(&id, imap_host).then_some(value)
}

/// The unfolded values of every `Authentication-Results` header, topmost
/// first.
fn headers(raw: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(raw);
    let mut values: Vec<String> = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        if line.is_empty() {
            break;
        }
        if line.starts_with([' ', '\t']) {
            if inside && let Some(v) = values.last_mut() {
                v.push(' ');
                v.push_str(line.trim());
            }
            continue;
        }
        inside = false;
        if let Some((name, rest)) = line.split_once(':')
            && name.trim().eq_ignore_ascii_case("authentication-results")
        {
            values.push(rest.trim().to_owned());
            inside = true;
        }
    }
    values
}

/// Replaces each RFC 5322 comment (nested, with backslash escapes) outside
/// a quoted string by one space.
fn strip_comments(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let (mut depth, mut quoted, mut escaped) = (0usize, false, false);
    for c in value.chars() {
        if escaped {
            escaped = false;
            if depth == 0 {
                out.push(c);
            }
            continue;
        }
        match c {
            '\\' => {
                escaped = true;
                if depth == 0 {
                    out.push(c);
                }
            }
            '"' if depth == 0 => {
                quoted = !quoted;
                out.push(c);
            }
            '(' if !quoted => {
                if depth == 0 {
                    out.push(' ');
                }
                depth += 1;
            }
            ')' if !quoted && depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// Splits `text` where `sep` matches outside a quoted string; empty pieces
/// are dropped from whitespace splits only by the caller.
fn split_outside_quotes(text: &str, sep: impl Fn(char) -> bool) -> Vec<String> {
    let mut pieces = vec![String::new()];
    let (mut quoted, mut escaped) = (false, false);
    for c in text.chars() {
        if escaped {
            escaped = false;
        } else if c == '\\' && quoted {
            escaped = true;
        } else if c == '"' {
            quoted = !quoted;
        } else if !quoted && sep(c) {
            pieces.push(String::new());
            continue;
        }
        if let Some(last) = pieces.last_mut() {
            last.push(c);
        }
    }
    pieces
}

/// The domain of a property value such as `bob@corp.com` or
/// `"a b"@corp.com` or `"bob@corp.com"`; `None` when a quoted part is not
/// followed by `@domain`.
fn value_domain(value: &str) -> Option<String> {
    let value = value.trim_end_matches(';');
    if let Some(rest) = value.strip_prefix('"') {
        let mut escaped = false;
        for (i, c) in rest.char_indices() {
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => {
                    let (inner, tail) = (&rest[..i], &rest[i + 1..]);
                    if tail.is_empty() && !inner.contains(char::is_whitespace) {
                        return Some(inner.rsplit('@').next()?.to_lowercase());
                    }
                    return Some(tail.strip_prefix('@')?.to_lowercase());
                }
                _ => {}
            }
        }
        return None;
    }
    Some(value.rsplit('@').next().unwrap_or(value).to_lowercase())
}

/// One `method=pass prop=value ...` clause vouches for `domain`.
fn passes(clause: &str, domain: &str) -> bool {
    let words: Vec<String> = split_outside_quotes(clause, char::is_whitespace)
        .into_iter()
        .filter(|w| !w.is_empty())
        .collect();
    let mut words = words.iter();
    let Some((method, result)) = words.next().and_then(|w| w.split_once('=')) else {
        return false;
    };
    if !result.eq_ignore_ascii_case("pass") {
        return false;
    }
    let method = method.to_lowercase();
    let props: Vec<(String, Option<String>)> = words
        .filter_map(|w| w.split_once('='))
        .map(|(k, v)| (k.to_lowercase(), value_domain(v)))
        .collect();
    let prop = |name: &str| {
        props
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_deref())
    };
    let aligned = |d: &str| aligned(d, domain);
    match method.as_str() {
        "dmarc" => prop("header.from").is_none_or(|d| d.is_some_and(aligned)),
        "dkim" => prop("header.d")
            .or_else(|| prop("header.i"))
            .is_some_and(|d| d.is_some_and(aligned)),
        "spf" => prop("smtp.mailfrom").is_some_and(|d| d.is_some_and(aligned)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::sender_verified;

    const HOST: &str = "imap.gmail.com";

    fn mail(results: &str) -> Vec<u8> {
        mail_from(results, "bob@example.com")
    }

    fn mail_from(results: &str, from: &str) -> Vec<u8> {
        format!("{results}From: {from}\r\nSubject: x\r\n\r\nbody").into_bytes()
    }

    #[test]
    fn no_header_is_not_verified() {
        assert!(!sender_verified(&mail(""), "bob@example.com", HOST));
    }

    #[test]
    fn dkim_pass_for_the_from_domain_verifies() {
        let m = mail("Authentication-Results: mx.google.com; dkim=pass header.d=example.com\r\n");
        assert!(sender_verified(&m, "Bob@Example.com", HOST));
    }

    #[test]
    fn dkim_pass_for_another_domain_does_not() {
        let m = mail("Authentication-Results: mx.google.com; dkim=pass header.d=example.net\r\n");
        assert!(!sender_verified(&m, "bob@example.com", HOST));
    }

    #[test]
    fn a_failed_check_does_not_verify() {
        let m = mail(
            "Authentication-Results: mx.google.com; dkim=fail header.d=example.com; spf=none\r\n",
        );
        assert!(!sender_verified(&m, "bob@example.com", HOST));
    }

    #[test]
    fn spf_and_dmarc_pass_verify_and_folded_lines_are_joined() {
        let m = mail(
            "Authentication-Results: mx.google.com;\r\n spf=pass smtp.mailfrom=bob@mail.example.net;\r\n dmarc=fail header.from=example.com\r\n",
        );
        assert!(!sender_verified(&m, "bob@example.com", HOST));
        let m = mail(
            "Authentication-Results: mx.google.com;\r\n dmarc=pass header.from=example.com\r\n",
        );
        assert!(sender_verified(&m, "bob@example.com", HOST));
        let m = mail(
            "Authentication-Results: mx.google.com;\r\n spf=pass smtp.mailfrom=bob@example.com\r\n",
        );
        assert!(sender_verified(&m, "bob@example.com", HOST));
    }

    #[test]
    fn dmarc_pass_for_another_header_from_does_not_verify() {
        let m =
            mail("Authentication-Results: mx.google.com; dmarc=pass header.from=example.net\r\n");
        assert!(!sender_verified(&m, "bob@example.com", HOST));
    }

    #[test]
    fn dmarc_pass_without_header_from_verifies() {
        // DMARC passes only for the From domain, so a pass with no property
        // to compare still speaks for the sender (RFC 7489 section 3.1).
        let m = mail("Authentication-Results: mx.google.com; dmarc=pass\r\n");
        assert!(sender_verified(&m, "bob@example.com", HOST));
    }

    #[test]
    fn only_the_topmost_header_counts() {
        let m = mail(
            "Authentication-Results: mx.google.com; dkim=fail header.d=example.com\r\nAuthentication-Results: example.net; dkim=pass header.d=example.com\r\n",
        );
        assert!(!sender_verified(&m, "bob@example.com", HOST));
    }

    #[test]
    fn a_foreign_authserv_id_is_not_trusted_even_topmost() {
        let m = mail("Authentication-Results: example.net; dkim=pass header.d=example.com\r\n");
        assert!(!sender_verified(&m, "bob@example.com", HOST));
        let m = mail(
            "Authentication-Results: example.net; dkim=pass header.d=example.com\r\nAuthentication-Results: mx.google.com; dkim=fail header.d=example.com\r\n",
        );
        assert!(!sender_verified(&m, "bob@example.com", HOST));
    }

    #[test]
    fn an_untrusted_topmost_header_hides_the_ones_below_it() {
        let m = mail(
            "Authentication-Results: example.net; dkim=pass header.d=example.com\r\nAuthentication-Results: mx.google.com; dkim=none\r\n",
        );
        assert!(!sender_verified(&m, "bob@example.com", HOST));
        let m = mail(
            "Authentication-Results: example.net; dkim=fail header.d=example.com\r\nAuthentication-Results: mx.google.com; dkim=pass header.d=example.com\r\n",
        );
        assert!(!sender_verified(&m, "bob@example.com", HOST));
    }

    #[test]
    fn an_id_less_exchange_header_on_top_hides_a_forged_trusted_one_below() {
        let m = mail(
            "Authentication-Results: spf=pass (sender IP is 1.2.3.4) smtp.mailfrom=example.com; dkim=pass header.d=example.com\r\nAuthentication-Results: mx.google.com; dkim=pass header.d=example.com\r\n",
        );
        assert!(!sender_verified(&m, "bob@example.com", HOST));
    }

    #[test]
    fn a_trusted_failing_header_on_top_is_not_overridden_by_a_forged_pass_below() {
        let m = mail(
            "Authentication-Results: mx.google.com; dkim=fail header.d=example.com\r\nAuthentication-Results: mx.google.com; dkim=pass header.d=example.com\r\n",
        );
        assert!(!sender_verified(&m, "bob@example.com", HOST));
    }

    #[test]
    fn a_header_without_an_authserv_id_does_not_verify() {
        // Some Exchange Online headers start straight at the results.
        let m = mail(
            "Authentication-Results: dkim=pass header.d=example.com; spf=pass smtp.mailfrom=example.com\r\n",
        );
        assert!(!sender_verified(
            &m,
            "bob@example.com",
            "outlook.office365.com"
        ));
        let m = mail("Authentication-Results: ; dkim=pass header.d=example.com\r\n");
        assert!(!sender_verified(
            &m,
            "bob@example.com",
            "outlook.office365.com"
        ));
    }

    #[test]
    fn a_semicolon_in_a_comment_does_not_split_the_header() {
        let m = mail(
            "Authentication-Results: mx.google.com; dkim=pass (2048-bit key; unprotected) header.d=example.com\r\n",
        );
        assert!(sender_verified(&m, "bob@example.com", HOST));
    }

    #[test]
    fn a_comment_holding_a_forged_property_is_not_read() {
        let m = mail(
            "Authentication-Results: mx.google.com; spf=pass (domain of \"a smtp.mailfrom=x@example.com b\"@example.net designates 1.2.3.4 as permitted) smtp.mailfrom=example.net\r\n",
        );
        assert!(!sender_verified(&m, "bob@example.com", HOST));
        let m = mail(
            "Authentication-Results: mx.google.com; spf=pass (nested (smtp.mailfrom=x@example.com) \\) smtp.mailfrom=x@example.com) smtp.mailfrom=example.net\r\n",
        );
        assert!(!sender_verified(&m, "bob@example.com", HOST));
        // Standalone tokens: read as the first property if comments stayed.
        let m = mail(
            "Authentication-Results: mx.google.com; spf=pass ( smtp.mailfrom=example.com ) smtp.mailfrom=example.net\r\n",
        );
        assert!(!sender_verified(&m, "bob@example.com", HOST));
        let m = mail(
            "Authentication-Results: mx.google.com; dkim=pass (header.d=example.com ) header.d=example.net\r\n",
        );
        assert!(!sender_verified(&m, "bob@example.com", HOST));
    }

    #[test]
    fn a_quoted_local_part_holding_a_property_is_not_read() {
        let m = mail(
            "Authentication-Results: mx.google.com; spf=pass smtp.mailfrom=\"x@example.com b\"@example.net\r\n",
        );
        assert!(!sender_verified(&m, "bob@example.com", HOST));
        // A quoted local part holding a `;` must not start a new clause.
        let m = mail(
            "Authentication-Results: mx.google.com; spf=pass smtp.mailfrom=\"a; dkim=pass header.d=example.com b\"@example.net\r\n",
        );
        assert!(!sender_verified(&m, "bob@example.com", HOST));
        let m = mail(
            "Authentication-Results: mx.google.com; spf=pass smtp.mailfrom=\"bob\"@example.com\r\n",
        );
        assert!(sender_verified(&m, "bob@example.com", HOST));
    }

    #[test]
    fn dmarc_header_from_alignment_is_relaxed_both_ways() {
        let m = mail_from(
            "Authentication-Results: mx.google.com; dmarc=pass header.from=sub.example.com\r\n",
            "bob@sub.example.com",
        );
        assert!(sender_verified(&m, "bob@sub.example.com", HOST));
        // A parent header domain covers a subdomain sender.
        let m = mail_from(
            "Authentication-Results: mx.google.com; dmarc=pass header.from=example.com\r\n",
            "bob@sub.example.com",
        );
        assert!(sender_verified(&m, "bob@sub.example.com", HOST));
        // Relaxed alignment (decision 5): a subdomain header domain covers
        // the parent sender too, they share an organisational domain.
        let m = mail(
            "Authentication-Results: mx.google.com; dmarc=pass header.from=sub.example.com\r\n",
        );
        assert!(sender_verified(&m, "bob@example.com", HOST));
        let m =
            mail("Authentication-Results: mx.google.com; dmarc=pass header.from=example.net\r\n");
        assert!(!sender_verified(&m, "bob@example.com", HOST));
    }

    #[test]
    fn a_sibling_subdomain_aligns_but_a_suffix_trick_does_not() {
        let m = mail_from(
            "Authentication-Results: mx.google.com; dkim=pass header.d=mail.example.com\r\n",
            "bob@news.example.com",
        );
        assert!(sender_verified(&m, "bob@news.example.com", HOST));
        let m = mail_from(
            "Authentication-Results: mx.google.com; dkim=pass header.d=example.com\r\n",
            "bob@evilexample.com",
        );
        assert!(!sender_verified(&m, "bob@evilexample.com", HOST));
    }

    #[test]
    fn an_ip_host_never_takes_the_fast_path() {
        // Comment 8528: the old rule took the last two labels of the host.
        for id in ["mx.0.1", "mx.10.0.0.1", "10.0.0.1"] {
            let m = mail(&format!(
                "Authentication-Results: {id}; dkim=pass header.d=example.com\r\n"
            ));
            assert!(!sender_verified(&m, "bob@example.com", "10.0.0.1"), "{id}");
        }
    }

    #[test]
    fn a_forged_header_on_a_server_that_stamps_nothing_is_not_verified() {
        // Verified under #278 by "same registrable domain as the host".
        let m = mail("Authentication-Results: mx.example.org; dkim=pass header.d=example.com\r\n");
        assert!(!sender_verified(&m, "bob@example.com", "imap.example.org"));
        assert!(!sender_verified(&m, "bob@example.com", "mx.example.org"));
    }

    #[test]
    fn alignment_is_relaxed_under_the_public_suffix_list() {
        let m =
            mail("Authentication-Results: mx.google.com; dkim=pass header.d=mail.example.com\r\n");
        assert!(sender_verified(&m, "bob@example.com", HOST));
        // duckdns.org is a public suffix: two tenants do not share a domain.
        let m = mail_from(
            "Authentication-Results: mx.google.com; dkim=pass header.d=evil.duckdns.org\r\n",
            "bob@victim.duckdns.org",
        );
        assert!(!sender_verified(&m, "bob@victim.duckdns.org", HOST));
    }

    #[test]
    fn microsoft_hosts_have_no_fast_path() {
        for host in ["outlook.office365.com", "imap-mail.outlook.com"] {
            for results in [
                "Authentication-Results: spf=pass smtp.mailfrom=example.com; dkim=pass header.d=example.com\r\n",
                "Authentication-Results: x.protection.outlook.com; dkim=pass header.d=example.com\r\n",
                "Authentication-Results: mx.outlook.com; spf=pass smtp.mailfrom=example.com\r\n",
            ] {
                assert!(!sender_verified(&mail(results), "bob@example.com", host));
            }
        }
    }

    #[test]
    fn two_from_fields_or_two_mailboxes_are_never_verified() {
        let ar = "Authentication-Results: mx.google.com; dkim=pass header.d=example.com\r\n";
        let two =
            format!("{ar}From: bob@example.com\r\nFrom: eve@example.net\r\nSubject: x\r\n\r\nb");
        assert!(!sender_verified(two.as_bytes(), "bob@example.com", HOST));
        assert!(!sender_verified(two.as_bytes(), "eve@example.net", HOST));
        let m = mail_from(ar, "bob@example.com, eve@example.net");
        assert!(!sender_verified(&m, "bob@example.com", HOST));
        // The From shown must be the one parsed.
        let m = mail_from(ar, "eve@example.net");
        assert!(!sender_verified(&m, "bob@example.com", HOST));
        // No From at all.
        let none = format!("{ar}Subject: x\r\n\r\nb");
        assert!(!sender_verified(none.as_bytes(), "bob@example.com", HOST));
    }

    #[test]
    fn an_id_must_equal_a_listed_id_or_end_with_a_dot_and_it() {
        let on = |id: &str, host: &str| {
            let m = mail(&format!(
                "Authentication-Results: {id}; dkim=pass header.d=example.com\r\n"
            ));
            sender_verified(&m, "bob@example.com", host)
        };
        assert!(on("mx.google.com", "imap.gmail.com"));
        assert!(on("a.mx.google.com", "imap.gmail.com"));
        assert!(!on("mx.google.com.evil.example", "imap.gmail.com"));
        assert!(!on("evilmx.google.com", "imap.gmail.com"));
        assert!(!on("google.com", "imap.gmail.com"));
        assert!(!on("mx.example.net", "imap.gmail.com"));
    }

    #[test]
    fn the_host_matches_exactly_ignoring_case_and_a_trailing_dot() {
        let m = mail("Authentication-Results: mx.google.com; dkim=pass header.d=example.com\r\n");
        for host in [
            "imap.gmail.com",
            "IMAP.Gmail.COM",
            "imap.gmail.com.",
            " imap.gmail.com ",
        ] {
            assert!(sender_verified(&m, "bob@example.com", host), "{host}");
        }
        for host in [
            "",
            "gmail.com",
            "imap.googlemail.com",
            "evil.imap.gmail.com",
            "imap.gmail.com.evil.example",
            "imap.me.org",
        ] {
            assert!(!sender_verified(&m, "bob@example.com", host), "{host}");
        }
    }

    #[test]
    fn each_table_provider_verifies_only_its_own_ids() {
        let cases = [
            ("imap.gmail.com", "mx.google.com", "mx.example.net"),
            (
                "imap.fastmail.com",
                "mx3.messagingengine.com",
                "mx3.other.example.net",
            ),
            ("imap.mail.me.com", "mx.icloud.com", "mx.example.net"),
            ("imap.mail.me.com", "relay.apple.com", "mx.example.net"),
            ("imap.mail.yahoo.com", "mta.yahoo.com", "mx.example.net"),
        ];
        for (host, id, other) in cases {
            let good = mail(&format!(
                "Authentication-Results: {id}; dkim=pass header.d=example.com\r\n"
            ));
            let bad = mail(&format!(
                "Authentication-Results: {other}; dkim=pass header.d=example.com\r\n"
            ));
            assert!(sender_verified(&good, "bob@example.com", host), "{host}");
            assert!(!sender_verified(&bad, "bob@example.com", host), "{host}");
        }
        // An id of one provider does not work on another's host.
        let m = mail("Authentication-Results: mx.google.com; dkim=pass header.d=example.com\r\n");
        assert!(!sender_verified(&m, "bob@example.com", "imap.fastmail.com"));
    }
}
