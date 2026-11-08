//! Keys for accounts and for files in the data folder: the creation time in
//! milliseconds, plus `-n` if that key is taken. Digits and `-` only, so a
//! key is always a safe file name.

pub(crate) fn fresh_key(now_ms: i64, taken: impl Fn(&str) -> bool) -> String {
    let base = now_ms.max(0).to_string();
    if !taken(&base) {
        return base;
    }
    (1_u32..)
        .map(|n| format!("{base}-{n}"))
        .find(|key| !taken(key))
        .unwrap_or(base)
}

/// Whether `key` has the shape `fresh_key` makes. Anything else (a path,
/// `..`, an empty string) is refused before it reaches the file system.
pub(crate) fn is_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_the_time_then_numbered() {
        assert_eq!(fresh_key(1234, |_| false), "1234");
        assert_eq!(fresh_key(1234, |key| key == "1234"), "1234-1");
        assert_eq!(
            fresh_key(1234, |key| key == "1234" || key == "1234-1"),
            "1234-2"
        );
        assert_eq!(fresh_key(-5, |_| false), "0");
    }

    #[test]
    fn only_key_shapes_pass() {
        assert!(is_key("1234"));
        assert!(is_key("1234-2"));
        for bad in ["", "..", "../x", "a1", "12/3", "1.json"] {
            assert!(!is_key(bad), "{bad}");
        }
    }
}
