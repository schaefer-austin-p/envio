//! The account heading in the folders drawer.

/// What an account is called on tabs and rows: its label, else the sender
/// name, else the address.
pub fn display_name(label: &str, name: &str, email: &str) -> String {
    [label, name, email]
        .into_iter()
        .map(str::trim)
        .find(|s| !s.is_empty())
        .unwrap_or_default()
        .to_owned()
}

/// "Name · address", or the address alone when the name is empty or is the
/// address (ignoring case and surrounding whitespace).
pub fn account_heading(name: &str, email: &str) -> String {
    let (name, email) = (name.trim(), email.trim());
    if name.is_empty() || name.to_lowercase() == email.to_lowercase() {
        email.to_owned()
    } else {
        format!("{name} · {email}")
    }
}

#[cfg(test)]
mod tests {
    use super::{account_heading, display_name};

    #[test]
    fn the_display_name_prefers_label_then_name_then_address() {
        assert_eq!(display_name("Work", "Ada", "a@x.org"), "Work");
        assert_eq!(display_name(" ", "Ada", "a@x.org"), "Ada");
        assert_eq!(display_name("", "", "a@x.org"), "a@x.org");
    }

    #[test]
    fn an_empty_name_shows_the_address_once() {
        assert_eq!(account_heading("", "grace@envio.test"), "grace@envio.test");
        assert_eq!(
            account_heading("  ", "grace@envio.test"),
            "grace@envio.test"
        );
    }

    #[test]
    fn the_address_as_name_shows_once_ignoring_case_and_whitespace() {
        assert_eq!(
            account_heading("grace@envio.test", "grace@envio.test"),
            "grace@envio.test"
        );
        assert_eq!(
            account_heading(" Grace@Envio.Test ", "grace@envio.test"),
            "grace@envio.test"
        );
    }

    #[test]
    fn a_different_name_shows_both() {
        assert_eq!(
            account_heading("Grace", "grace@envio.test"),
            "Grace · grace@envio.test"
        );
    }
}
