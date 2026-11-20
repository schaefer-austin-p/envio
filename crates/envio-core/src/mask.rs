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

/// A server's reply text, safe for a log: addresses are masked, long
/// token-like runs (an access token, a SASL string) are dropped, and it is
/// cut at 200 characters.
pub fn server_text(text: &str) -> String {
    let words: Vec<String> = text
        .split_whitespace()
        .map(|word| {
            let core = word.trim_matches(|c: char| ",;:()[]<>\"'".contains(c));
            if core.contains('@') {
                word.replace(core, &address(core))
            } else if core.chars().count() >= 24
                && core
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._-+/=~".contains(c))
            {
                "<redacted>".to_string()
            } else {
                word.to_string()
            }
        })
        .collect();
    let joined = words.join(" ");
    if joined.chars().count() > 200 {
        let cut: String = joined.chars().take(200).collect();
        format!("{cut}…")
    } else {
        joined
    }
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

    #[test]
    fn a_subject_becomes_its_length() {
        assert_eq!(subject("Lunch plans"), "<subject, 11 chars>");
    }
}
