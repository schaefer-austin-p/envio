//! The account heading in the folders drawer.

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
    use super::account_heading;

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
