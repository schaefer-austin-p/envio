//! Whether the receiving server vouched for the From address, read from its
//! `Authentication-Results` header (RFC 8601). Envio checks no DKIM or SPF
//! itself; it only believes the verdict of the account's own mail provider.

/// Providers whose `Authentication-Results` authserv-id is on another domain
/// than the IMAP host: (registrable domain of the IMAP host, registrable
/// domains that stamp for it). The one place to extend.
const PROVIDER_STAMPS: &[(&str, &[&str])] = &[
    // imap.gmail.com stamps as mx.google.com.
    ("gmail.com", &["google.com"]),
    ("googlemail.com", &["google.com"]),
    // outlook.office365.com stamps as *.outlook.com / *.protection.outlook.com.
    ("office365.com", &["outlook.com"]),
    // imap.mail.me.com stamps as *.icloud.com or *.apple.com.
    ("me.com", &["icloud.com", "apple.com"]),
    // imap.mail.yahoo.com stamps as *.yahoo.com.
    ("yahoo.com", &["yahoo.com"]),
    // imap.fastmail.com stamps as mx*.messagingengine.com.
    ("fastmail.com", &["messagingengine.com"]),
];

/// Second-level labels that sit under a two-letter country code (co.uk,
/// com.au), so the registrable domain has three labels. Not a full public
/// suffix list.
const SECOND_LEVEL: &[&str] = &["co", "com", "org", "net", "gov", "edu", "ac", "ne", "or"];

/// The sender's domain was authenticated by the account's own server: the
/// topmost `Authentication-Results` header whose authserv-id belongs to the
/// provider of `imap_host` (lower ones, and any from other hosts, may come
/// from the sender) holds a `dmarc=pass`, or a `dkim=pass` or `spf=pass` for
/// a domain that matches the `from` address's domain. A message with no
/// such header is not verified.
pub fn sender_verified(raw: &[u8], from: &str, imap_host: &str) -> bool {
    let Some(domain) = from.trim().rsplit_once('@').map(|(_, d)| d.to_lowercase()) else {
        return false;
    };
    let Some(header) = results_of(raw, imap_host) else {
        return false;
    };
    split_outside_quotes(&header, |c| c == ';')
        .iter()
        .skip(1)
        .any(|clause| passes(clause, &domain))
}

/// The last two labels of `host`, or three when the second-to-last is a
/// well-known second-level label under a two-letter country code.
fn registrable(host: &str) -> String {
    let host = host.trim().trim_end_matches('.').to_lowercase();
    let labels: Vec<&str> = host.split('.').collect();
    let n = labels.len();
    let keep = if n >= 3 && labels[n - 1].len() == 2 && SECOND_LEVEL.contains(&labels[n - 2]) {
        3
    } else {
        2
    };
    labels[n.saturating_sub(keep)..].join(".")
}

/// Whether an authserv-id belongs to the provider of `imap_host`.
fn trusted_id(id: &str, imap_host: &str) -> bool {
    let id = id.trim().trim_matches('"');
    if id.is_empty() || imap_host.trim().is_empty() {
        return false;
    }
    let (id_reg, host_reg) = (registrable(id), registrable(imap_host));
    if id_reg == host_reg {
        return true;
    }
    PROVIDER_STAMPS
        .iter()
        .any(|(host, stamps)| *host == host_reg && stamps.contains(&id_reg.as_str()))
}

/// The comment-free value of the topmost `Authentication-Results` header
/// whose authserv-id is trusted for `imap_host`.
fn results_of(raw: &[u8], imap_host: &str) -> Option<String> {
    headers(raw).into_iter().find_map(|value| {
        let value = strip_comments(&value);
        let id = split_outside_quotes(&value, |c| c == ';')
            .first()
            .and_then(|first| first.split_whitespace().next().map(str::to_owned))?;
        trusted_id(&id, imap_host).then_some(value)
    })
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
    let aligned = |d: &str| {
        domain == d
            || domain
                .strip_suffix(d)
                .is_some_and(|head| head.ends_with('.'))
    };
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

    const HOST: &str = "imap.me.org";

    fn mail(results: &str) -> Vec<u8> {
        format!("{results}From: bob@corp.com\r\nSubject: x\r\n\r\nbody").into_bytes()
    }

    #[test]
    fn no_header_is_not_verified() {
        assert!(!sender_verified(&mail(""), "bob@corp.com", HOST));
    }

    #[test]
    fn dkim_pass_for_the_from_domain_verifies() {
        let m = mail("Authentication-Results: mx.me.org; dkim=pass header.d=corp.com\r\n");
        assert!(sender_verified(&m, "Bob@Corp.com", HOST));
    }

    #[test]
    fn dkim_pass_for_another_domain_does_not() {
        let m = mail("Authentication-Results: mx.me.org; dkim=pass header.d=evil.net\r\n");
        assert!(!sender_verified(&m, "bob@corp.com", HOST));
    }

    #[test]
    fn a_failed_check_does_not_verify() {
        let m =
            mail("Authentication-Results: mx.me.org; dkim=fail header.d=corp.com; spf=none\r\n");
        assert!(!sender_verified(&m, "bob@corp.com", HOST));
    }

    #[test]
    fn spf_and_dmarc_pass_verify_and_folded_lines_are_joined() {
        let m = mail(
            "Authentication-Results: mx.me.org;\r\n spf=pass smtp.mailfrom=bob@mail.corp.com;\r\n dmarc=fail header.from=corp.com\r\n",
        );
        assert!(!sender_verified(&m, "bob@corp.com", HOST));
        let m = mail("Authentication-Results: mx.me.org;\r\n dmarc=pass header.from=corp.com\r\n");
        assert!(sender_verified(&m, "bob@corp.com", HOST));
        let m =
            mail("Authentication-Results: mx.me.org;\r\n spf=pass smtp.mailfrom=bob@corp.com\r\n");
        assert!(sender_verified(&m, "bob@corp.com", HOST));
    }

    #[test]
    fn only_the_topmost_header_counts() {
        let m = mail(
            "Authentication-Results: mx.me.org; dkim=fail header.d=corp.com\r\nAuthentication-Results: evil.net; dkim=pass header.d=corp.com\r\n",
        );
        assert!(!sender_verified(&m, "bob@corp.com", HOST));
    }

    #[test]
    fn a_subdomain_sender_matches_a_parent_signing_domain_but_not_a_suffix_trick() {
        let m = mail("Authentication-Results: mx.me.org; dkim=pass header.d=corp.com\r\n");
        assert!(sender_verified(&m, "bob@mail.corp.com", HOST));
        assert!(!sender_verified(&m, "bob@evilcorp.com", HOST));
    }

    #[test]
    fn a_foreign_authserv_id_is_not_trusted_even_topmost() {
        let m = mail("Authentication-Results: evil.net; dkim=pass header.d=corp.com\r\n");
        assert!(!sender_verified(&m, "bob@corp.com", HOST));
        let m = mail(
            "Authentication-Results: evil.net; dkim=pass header.d=corp.com\r\nAuthentication-Results: mx.me.org; dkim=fail header.d=corp.com\r\n",
        );
        assert!(!sender_verified(&m, "bob@corp.com", HOST));
    }

    #[test]
    fn a_header_above_the_servers_matching_one_is_ignored() {
        let m = mail(
            "Authentication-Results: evil.net; dkim=pass header.d=corp.com\r\nAuthentication-Results: mx.me.org; dkim=none\r\n",
        );
        assert!(!sender_verified(&m, "bob@corp.com", HOST));
        let m = mail(
            "Authentication-Results: evil.net; dkim=fail header.d=corp.com\r\nAuthentication-Results: mx.me.org; dkim=pass header.d=corp.com\r\n",
        );
        assert!(sender_verified(&m, "bob@corp.com", HOST));
    }

    #[test]
    fn a_matching_authserv_id_is_trusted_by_registrable_domain() {
        let m = mail("Authentication-Results: mx.mail.me.org; dkim=pass header.d=corp.com\r\n");
        assert!(sender_verified(&m, "bob@corp.com", "imap.me.org"));
        assert!(!sender_verified(&m, "bob@corp.com", "imap.other.org"));
        assert!(!sender_verified(&m, "bob@corp.com", ""));
        let m = mail("Authentication-Results: mx.me.co.uk; dkim=pass header.d=corp.com\r\n");
        assert!(sender_verified(&m, "bob@corp.com", "imap.me.co.uk"));
        assert!(!sender_verified(&m, "bob@corp.com", "imap.you.co.uk"));
    }

    #[test]
    fn the_provider_map_covers_gmail() {
        let m = mail("Authentication-Results: mx.google.com; dkim=pass header.d=corp.com\r\n");
        assert!(sender_verified(&m, "bob@corp.com", "imap.gmail.com"));
        assert!(!sender_verified(&m, "bob@corp.com", "imap.me.org"));
        let m = mail(
            "Authentication-Results: x.protection.outlook.com; spf=pass smtp.mailfrom=corp.com\r\n",
        );
        assert!(sender_verified(&m, "bob@corp.com", "outlook.office365.com"));
    }

    #[test]
    fn the_provider_map_covers_fastmail() {
        let m = mail(
            "Authentication-Results: mx3.messagingengine.com; dkim=pass header.d=corp.com\r\n",
        );
        assert!(sender_verified(&m, "bob@corp.com", "imap.fastmail.com"));
        let m = mail("Authentication-Results: mx3.other.net; dkim=pass header.d=corp.com\r\n");
        assert!(!sender_verified(&m, "bob@corp.com", "imap.fastmail.com"));
    }

    #[test]
    fn a_header_without_an_authserv_id_does_not_verify() {
        // Some Exchange Online headers start straight at the results.
        let m = mail(
            "Authentication-Results: dkim=pass header.d=corp.com; spf=pass smtp.mailfrom=corp.com\r\n",
        );
        assert!(!sender_verified(
            &m,
            "bob@corp.com",
            "outlook.office365.com"
        ));
        let m = mail("Authentication-Results: ; dkim=pass header.d=corp.com\r\n");
        assert!(!sender_verified(
            &m,
            "bob@corp.com",
            "outlook.office365.com"
        ));
    }

    #[test]
    fn a_semicolon_in_a_comment_does_not_split_the_header() {
        let m = mail(
            "Authentication-Results: mx.me.org; dkim=pass (2048-bit key; unprotected) header.d=corp.com\r\n",
        );
        assert!(sender_verified(&m, "bob@corp.com", HOST));
    }

    #[test]
    fn a_comment_holding_a_forged_property_is_not_read() {
        let m = mail(
            "Authentication-Results: mx.me.org; spf=pass (domain of \"a smtp.mailfrom=x@corp.com b\"@evil.net designates 1.2.3.4 as permitted) smtp.mailfrom=evil.net\r\n",
        );
        assert!(!sender_verified(&m, "bob@corp.com", HOST));
        let m = mail(
            "Authentication-Results: mx.me.org; spf=pass (nested (smtp.mailfrom=x@corp.com) \\) smtp.mailfrom=x@corp.com) smtp.mailfrom=evil.net\r\n",
        );
        assert!(!sender_verified(&m, "bob@corp.com", HOST));
        // Standalone tokens: read as the first property if comments stayed.
        let m = mail(
            "Authentication-Results: mx.me.org; spf=pass ( smtp.mailfrom=corp.com ) smtp.mailfrom=evil.net\r\n",
        );
        assert!(!sender_verified(&m, "bob@corp.com", HOST));
        let m = mail(
            "Authentication-Results: mx.me.org; dkim=pass (header.d=corp.com ) header.d=evil.net\r\n",
        );
        assert!(!sender_verified(&m, "bob@corp.com", HOST));
    }

    #[test]
    fn a_quoted_local_part_holding_a_property_is_not_read() {
        let m = mail(
            "Authentication-Results: mx.me.org; spf=pass smtp.mailfrom=\"x@corp.com b\"@evil.net\r\n",
        );
        assert!(!sender_verified(&m, "bob@corp.com", HOST));
        // A quoted local part holding a `;` must not start a new clause.
        let m = mail(
            "Authentication-Results: mx.me.org; spf=pass smtp.mailfrom=\"a; dkim=pass header.d=corp.com b\"@evil.net\r\n",
        );
        assert!(!sender_verified(&m, "bob@corp.com", HOST));
        let m =
            mail("Authentication-Results: mx.me.org; spf=pass smtp.mailfrom=\"bob\"@corp.com\r\n");
        assert!(sender_verified(&m, "bob@corp.com", HOST));
    }
}
