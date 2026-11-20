//! Keywords a server sets by itself on every message. They say nothing the
//! user chose, and the server sets them again on the replacement, so they do
//! not stop an Edit from replacing its original (#324).

/// Stalwart marks every stored message with one of these two by what it
/// finds in the body. IMAP compares keywords without regard to case.
const SERVER_MADE: [&str; 2] = ["$HasAttachment", "$HasNoAttachment"];

/// `true` when `keywords` holds nothing but server-made ones (or nothing).
pub(crate) fn only_server_made(keywords: &[String]) -> bool {
    keywords
        .iter()
        .all(|k| SERVER_MADE.iter().any(|m| m.eq_ignore_ascii_case(k)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn nothing_and_the_servers_own_are_harmless() {
        assert!(only_server_made(&[]));
        assert!(only_server_made(&words(&["$hasnoattachment"])));
        assert!(only_server_made(&words(&["$HasAttachment"])));
    }

    #[test]
    fn any_other_keyword_is_not() {
        assert!(!only_server_made(&words(&["$Forwarded"])));
        assert!(!only_server_made(&words(&["$hasnoattachment", "$Label1"])));
    }
}
