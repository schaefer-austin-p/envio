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
    fn a_subject_becomes_its_length() {
        assert_eq!(subject("Lunch plans"), "<subject, 11 chars>");
    }
}
