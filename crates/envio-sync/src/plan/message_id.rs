//! Which Message-IDs are safe to search for. Pure.

/// `<x>` with something inside: the only shape searched for on a server.
/// A substring search for `<>` or "" would match any message.
pub fn usable_message_id(id: &str) -> bool {
    id.trim()
        .strip_prefix('<')
        .and_then(|rest| rest.strip_suffix('>'))
        .is_some_and(|inner| !inner.is_empty())
}

#[cfg(test)]
mod tests {
    use super::usable_message_id;

    #[test]
    fn only_an_angle_bracketed_id_is_searched_for() {
        assert!(usable_message_id("<a@b>"));
        assert!(usable_message_id(" <a@b> "));
        for bad in ["", "  ", "<>", "a@b", "<a@b", "a@b>"] {
            assert!(!usable_message_id(bad), "{bad:?}");
        }
    }
}
