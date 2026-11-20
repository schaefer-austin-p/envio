//! Log masking: what may appear in a log line instead of mail content.

/// `alice@example.org` becomes `a…@example.org`; without an `@`, just `…`.
pub fn address(email: &str) -> String {
    match email.split_once('@') {
        Some((local, domain)) => {
            let first: String = local.chars().take(1).collect();
            format!("{first}…@{domain}")
        }
        None => "…".to_string(),
    }
}

/// A server's reply text, safe for a log. It fails closed: addresses are
/// masked, then any run of 24 or more token characters (letters, digits and
/// `._-+/=~`) is dropped wherever it sits, whatever quotes, braces, `%`,
/// backslashes or control characters surround it. Cut at 200 characters.
pub fn server_text(text: &str) -> String {
    const MIN_RUN: usize = 24;
    let masked = mask_addresses(text);
    let mut out = String::new();
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String| {
        if run.chars().count() >= MIN_RUN {
            out.push_str("<redacted>");
        } else {
            out.push_str(run);
        }
        run.clear();
    };
    for c in masked.chars() {
        if c.is_ascii_alphanumeric() || "._-+/=~".contains(c) {
            run.push(c);
        } else {
            flush(&mut run, &mut out);
            out.push(if c.is_control() { ' ' } else { c });
        }
    }
    flush(&mut run, &mut out);
    if out.chars().count() > 200 {
        let cut: String = out.chars().take(200).collect();
        format!("{cut}…")
    } else {
        out
    }
}

/// Every `local@domain` (the characters an address is made of on both sides
/// of an `@`) becomes `l…@domain`; a lone `@` stays.
fn mask_addresses(text: &str) -> String {
    let local = |c: char| c.is_ascii_alphanumeric() || "._%+-".contains(c);
    let domain = |c: char| c.is_ascii_alphanumeric() || ".-".contains(c);
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '@' {
            let start = out
                .char_indices()
                .rev()
                .take_while(|(_, c)| local(*c))
                .last()
                .map(|(at, _)| at);
            let mut end = i + 1;
            while end < chars.len() && domain(chars[end]) {
                end += 1;
            }
            if let Some(start) = start.filter(|_| end > i + 1) {
                let first: String = out[start..].chars().take(1).collect();
                out.truncate(start);
                out.push_str(&first);
                out.push('…');
                out.extend(&chars[i..end]);
                i = end;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// A subject becomes only its length.
pub fn subject(subject: &str) -> String {
    format!("<subject, {} chars>", subject.chars().count())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_keeps_only_its_first_letter_and_domain() {
        assert_eq!(address("alice@example.org"), "a…@example.org");
        assert_eq!(address("not-an-address"), "…");
    }

    #[test]
    fn server_text_masks_addresses_and_drops_token_like_runs() {
        let text = "AUTHENTICATE failed for alice@outlook.com token \
                    eyJ0eXAiOiJKV1QiLCJhbGciOiJSUzI1NiJ9.abcdef.ghijkl [AUTHENTICATIONFAILED]";
        let out = server_text(text);
        assert!(out.contains("a…@outlook.com"), "{out}");
        assert!(!out.contains("alice"), "{out}");
        assert!(!out.contains("eyJ"), "{out}");
        assert!(out.contains("AUTHENTICATIONFAILED"), "{out}");
        assert!(server_text(&"x ".repeat(300)).chars().count() <= 201);
    }

    const TOK: &str = "EwBwA8l6BAAUbDba3x2OMJElkF7gJ4z_VbCPEz0AAfj0fmHWqWbTB2xt";

    fn assert_clean(text: &str) {
        let out = server_text(text);
        assert!(!out.contains("EwBw"), "{text} -> {out}");
        assert!(!out.contains("fj0fm"), "{text} -> {out}");
    }

    #[test]
    fn server_text_redacts_a_token_wherever_it_sits_in_a_word() {
        assert_clean(&format!("token=\"{TOK}\""));
        assert_clean(&format!("{{\"access_token\":\"{TOK}\"}}"));
        assert_clean(&format!("{TOK}%3D%3D"));
        assert_clean(&format!("{TOK}\\\""));
        assert_clean(&format!("user=bob@x.com\x01auth=Bearer {TOK}\x01\x01"));
        assert_clean(&format!("{TOK}="));
        assert_clean(&format!("{TOK}=="));
        assert_clean(&format!("a'{TOK}'b,{TOK}:x"));
    }

    #[test]
    fn server_text_masks_an_address_beside_a_display_name() {
        let out = server_text("for Bob Smith <bob@outlook.com>, retry");
        assert!(out.contains("<b…@outlook.com>"), "{out}");
        assert!(!out.contains("bob@"), "{out}");
    }

    #[test]
    fn a_subject_becomes_its_length() {
        assert_eq!(subject("Lunch plans"), "<subject, 11 chars>");
    }
}
