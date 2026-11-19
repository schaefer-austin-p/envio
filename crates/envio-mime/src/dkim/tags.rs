//! The DKIM tag-list syntax (RFC 6376 3.2), shared by the signature header
//! and the key record.

use base64::Engine as _;

/// A tag list longer than this is not a DKIM header or key record (a 4096-bit
/// RSA key record is about 800 bytes).
const MAX_TAG_LIST: usize = 16 * 1024;

/// RFC 6376 3.2 tag-list: `tag=value` pairs split on `;`, whitespace
/// (FWS) around tags and values removed, inside base64 values removed.
/// None on a duplicate tag or a malformed pair.
pub(crate) fn tag_list(value: &str) -> Option<Vec<(String, String)>> {
    if value.len() > MAX_TAG_LIST {
        return None;
    }
    let mut tags: Vec<(String, String)> = Vec::new();
    let pieces: Vec<&str> = value.split(';').collect();
    for (i, piece) in pieces.iter().enumerate() {
        if piece.trim().is_empty() {
            // Only the end of the list may be empty (`a=1;`).
            if i + 1 == pieces.len() {
                break;
            }
            return None;
        }
        let (name, val) = piece.split_once('=')?;
        let name = name.trim();
        let mut chars = name.chars();
        let first_ok = chars.next().is_some_and(|c| c.is_ascii_alphabetic());
        if !first_ok || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return None;
        }
        if tags.iter().any(|(n, _)| n == name) {
            return None;
        }
        let val = if matches!(name, "b" | "bh" | "p") {
            val.chars().filter(|c| !c.is_whitespace()).collect()
        } else {
            val.trim().to_owned()
        };
        tags.push((name.to_owned(), val));
    }
    Some(tags)
}

/// Standard base64 with padding, as DKIM uses for `b=`, `bh=` and `p=`.
pub(super) fn base64_bytes(value: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::STANDARD.decode(value).ok()
}

/// A colon-separated list (`h=`, `q=`, and the key record's `t=`, `s=`, `h=`),
/// each entry trimmed.
pub(super) fn colon_list(value: &str) -> impl Iterator<Item = &str> {
    value.split(':').map(str::trim)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn get<'a>(tags: &'a [(String, String)], name: &str) -> Option<&'a str> {
        tags.iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    #[test]
    fn folded_values_with_tabs_and_crlf_are_unfolded() {
        let tags = tag_list(" v=1;\r\n\ta=rsa-sha256 ;\r\n d = example.com\t;").unwrap();
        assert_eq!(get(&tags, "v"), Some("1"));
        assert_eq!(get(&tags, "a"), Some("rsa-sha256"));
        assert_eq!(get(&tags, "d"), Some("example.com"));
        assert_eq!(tags.len(), 3);
    }

    #[test]
    fn base64_values_lose_inner_whitespace() {
        let tags = tag_list("b=QUJD\r\n REVG\tR0hJ; bh=AA AA; p=MI GfMA").unwrap();
        assert_eq!(get(&tags, "b"), Some("QUJDREVGR0hJ"));
        assert_eq!(get(&tags, "bh"), Some("AAAA"));
        assert_eq!(get(&tags, "p"), Some("MIGfMA"));
    }

    #[test]
    fn a_duplicate_tag_is_none() {
        assert_eq!(tag_list("a=1; b=2; a=3"), None);
    }

    #[test]
    fn a_trailing_semicolon_is_fine_and_an_empty_value_is_kept() {
        let tags = tag_list("v=1; b=;").unwrap();
        assert_eq!(get(&tags, "b"), Some(""));
        assert_eq!(tags.len(), 2);
        assert_eq!(tag_list("").unwrap().len(), 0);
    }

    #[test]
    fn malformed_pairs_are_none() {
        for bad in ["v", "v=1;;a=2", "=1", "a b=1", "v=1; ;a=2", "1a=2"] {
            assert_eq!(tag_list(bad), None, "{bad:?}");
        }
        assert_eq!(tag_list(&"a=1;".repeat(10_000)), None, "size cap");
    }

    proptest! {
        #[test]
        fn tag_list_never_panics(s in any::<String>()) {
            let _ = tag_list(&s);
        }
    }
}
