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
        if piece.trim_matches(is_fws).is_empty() {
            // Only the end of the list may be empty (`a=1;`).
            if i + 1 == pieces.len() {
                break;
            }
            return None;
        }
        let (name, val) = piece.split_once('=')?;
        let name = name.trim_matches(is_fws);
        let mut chars = name.chars();
        let first_ok = chars.next().is_some_and(|c| c.is_ascii_alphabetic());
        if !first_ok || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return None;
        }
        if tags.iter().any(|(n, _)| n == name) {
            return None;
        }
        let val = if matches!(name, "b" | "bh" | "p") {
            val.chars().filter(|c| !is_fws(*c)).collect()
        } else {
            val.trim_matches(is_fws).to_owned()
        };
        tags.push((name.to_owned(), val));
    }
    Some(tags)
}

/// Standard base64 with padding, as DKIM uses for `b=`, `bh=` and `p=`.
pub(super) fn base64_bytes(value: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::STANDARD.decode(value).ok()
}

/// RFC 5234 WSP and CRLF, the only whitespace a tag-list may fold with.
pub(super) fn is_fws(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

/// A colon-separated list (`h=`, `q=`, and the key record's `t=`, `s=`, `h=`),
/// each entry trimmed.
pub(super) fn colon_list(value: &str) -> impl Iterator<Item = &str> {
    value.split(':').map(|e| e.trim_matches(is_fws))
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

    #[test]
    fn only_rfc_fws_is_whitespace() {
        // NBSP, EM SPACE, VT and FF are not FWS.
        for ws in ["\u{a0}", "\u{2003}", "\x0b", "\x0c"] {
            assert_eq!(tag_list(&format!("a=1;{ws}b=2")), None, "{ws:?}");
            assert_eq!(tag_list(&format!("a{ws}=1")), None, "{ws:?}");
            let tags = tag_list(&format!("a={ws}1")).unwrap();
            assert_eq!(get(&tags, "a"), Some(format!("{ws}1").as_str()), "{ws:?}");
            let tags = tag_list(&format!("b=QU{ws}JD")).unwrap();
            assert_eq!(
                get(&tags, "b"),
                Some(format!("QU{ws}JD").as_str()),
                "{ws:?}"
            );
            assert!(colon_list(&format!("{ws}x")).any(|e| e == format!("{ws}x")));
        }
        assert_eq!(colon_list(" \t\r\nx \t").next(), Some("x"));
    }

    #[test]
    fn the_size_cap_is_exact() {
        let at = format!("a={}", "x".repeat(MAX_TAG_LIST - 2));
        assert_eq!(at.len(), 16384);
        assert!(tag_list(&at).is_some());
        let over = format!("a={}", "x".repeat(MAX_TAG_LIST - 1));
        assert_eq!(over.len(), 16385);
        assert_eq!(tag_list(&over), None);
    }

    #[test]
    fn the_worst_case_input_is_fast() {
        // Many distinct tags: the duplicate check is the quadratic part.
        let mut s = String::new();
        let mut i = 0;
        while s.len() + 8 <= MAX_TAG_LIST {
            s.push_str(&format!("t{i}=;"));
            i += 1;
        }
        let started = std::time::Instant::now();
        let tags = tag_list(&s).unwrap();
        assert!(tags.len() > 2000);
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }

    proptest! {
        #[test]
        fn tag_list_never_panics(s in any::<String>()) {
            let _ = tag_list(&s);
        }
    }
}
