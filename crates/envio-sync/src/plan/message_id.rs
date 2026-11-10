//! Which Message-IDs are safe to search for. Pure.

/// `<x>` with something inside: the only shape searched for on a server.
/// A substring search for `<>` or "" would match any message.
pub fn usable_message_id(id: &str) -> bool {
    id.trim()
        .strip_prefix('<')
        .and_then(|rest| rest.strip_suffix('>'))
        .is_some_and(|inner| !inner.is_empty())
}

/// The trimmed id to search for, when it is usable.
pub fn search_id(id: &str) -> Option<&str> {
    usable_message_id(id).then(|| id.trim())
}

#[cfg(test)]
mod tests {
    use super::{search_id, usable_message_id};

    #[test]
    fn search_id_trims_and_rejects_unusable() {
        assert_eq!(search_id(" <a@b> "), Some("<a@b>"));
        assert_eq!(search_id("<>"), None);
    }

    #[test]
    fn only_an_angle_bracketed_id_is_searched_for() {
        assert!(usable_message_id("<a@b>"));
        assert!(usable_message_id(" <a@b> "));
        for bad in ["", "  ", "<>", "a@b", "<a@b", "a@b>"] {
            assert!(!usable_message_id(bad), "{bad:?}");
        }
    }
}
