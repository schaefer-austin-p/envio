//! Whether the receiving server vouched for the From address, read from its
//! `Authentication-Results` header (RFC 8601). Envio checks no DKIM or SPF
//! itself; it only believes the server's verdict.

/// The sender's domain was authenticated by the server: the topmost
/// `Authentication-Results` header (the one our own server prepended; lower
/// ones may come from the sender) holds a `dmarc=pass`, or a `dkim=pass` or
/// `spf=pass` for a domain that matches the `from` address's domain.
/// A message with no such header is not verified.
pub fn sender_verified(raw: &[u8], from: &str) -> bool {
    let Some(domain) = from.trim().rsplit_once('@').map(|(_, d)| d.to_lowercase()) else {
        return false;
    };
    let Some(header) = topmost_results(raw) else {
        return false;
    };
    header
        .split(';')
        .skip(1)
        .any(|clause| passes(clause, &domain))
}

/// The unfolded value of the first `Authentication-Results` header.
fn topmost_results(raw: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(raw);
    let mut value: Option<String> = None;
    for line in text.lines() {
        if line.is_empty() {
            break;
        }
        if line.starts_with([' ', '\t']) {
            if let Some(v) = value.as_mut() {
                v.push(' ');
                v.push_str(line.trim());
            }
            continue;
        }
        if value.is_some() {
            break;
        }
        if let Some((name, rest)) = line.split_once(':')
            && name.trim().eq_ignore_ascii_case("authentication-results")
        {
            value = Some(rest.trim().to_owned());
        }
    }
    value
}

/// One `method=pass prop=value ...` clause vouches for `domain`.
fn passes(clause: &str, domain: &str) -> bool {
    let mut words = clause.split_whitespace();
    let Some((method, result)) = words.next().and_then(|w| w.split_once('=')) else {
        return false;
    };
    if !result.eq_ignore_ascii_case("pass") {
        return false;
    }
    let method = method.to_lowercase();
    let props: Vec<(String, String)> = words
        .filter_map(|w| w.split_once('='))
        .map(|(k, v)| (k.to_lowercase(), v.trim_matches(['"', ';']).to_lowercase()))
        .collect();
    let prop = |name: &str| {
        props
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.rsplit('@').next().unwrap_or(v))
    };
    let aligned = |d: &str| {
        domain == d
            || domain
                .strip_suffix(d)
                .is_some_and(|head| head.ends_with('.'))
    };
    match method.as_str() {
        "dmarc" => prop("header.from").is_none_or(aligned),
        "dkim" => prop("header.d")
            .or_else(|| prop("header.i"))
            .is_some_and(aligned),
        "spf" => prop("smtp.mailfrom").is_some_and(aligned),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::sender_verified;

    fn mail(results: &str) -> Vec<u8> {
        format!("{results}From: bob@corp.com\r\nSubject: x\r\n\r\nbody").into_bytes()
    }

    #[test]
    fn no_header_is_not_verified() {
        assert!(!sender_verified(&mail(""), "bob@corp.com"));
    }

    #[test]
    fn dkim_pass_for_the_from_domain_verifies() {
        let m = mail("Authentication-Results: mx.me.org; dkim=pass header.d=corp.com\r\n");
        assert!(sender_verified(&m, "Bob@Corp.com"));
    }

    #[test]
    fn dkim_pass_for_another_domain_does_not() {
        let m = mail("Authentication-Results: mx.me.org; dkim=pass header.d=evil.net\r\n");
        assert!(!sender_verified(&m, "bob@corp.com"));
    }

    #[test]
    fn a_failed_check_does_not_verify() {
        let m =
            mail("Authentication-Results: mx.me.org; dkim=fail header.d=corp.com; spf=none\r\n");
        assert!(!sender_verified(&m, "bob@corp.com"));
    }

    #[test]
    fn spf_and_dmarc_pass_verify_and_folded_lines_are_joined() {
        let m = mail(
            "Authentication-Results: mx.me.org;\r\n spf=pass smtp.mailfrom=bob@mail.corp.com;\r\n dmarc=fail header.from=corp.com\r\n",
        );
        assert!(!sender_verified(&m, "bob@corp.com"));
        let m = mail("Authentication-Results: mx.me.org;\r\n dmarc=pass header.from=corp.com\r\n");
        assert!(sender_verified(&m, "bob@corp.com"));
        let m =
            mail("Authentication-Results: mx.me.org;\r\n spf=pass smtp.mailfrom=bob@corp.com\r\n");
        assert!(sender_verified(&m, "bob@corp.com"));
    }

    #[test]
    fn only_the_topmost_header_counts() {
        let m = mail(
            "Authentication-Results: mx.me.org; dkim=fail header.d=corp.com\r\nAuthentication-Results: forged; dkim=pass header.d=corp.com\r\n",
        );
        assert!(!sender_verified(&m, "bob@corp.com"));
    }

    #[test]
    fn a_subdomain_sender_matches_a_parent_signing_domain_but_not_a_suffix_trick() {
        let m = mail("Authentication-Results: mx; dkim=pass header.d=corp.com\r\n");
        assert!(sender_verified(&m, "bob@mail.corp.com"));
        assert!(!sender_verified(&m, "bob@evilcorp.com"));
    }
}
