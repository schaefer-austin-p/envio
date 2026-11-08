//! A cheap guard against deeply nested HTML. ammonia (html5ever) is
//! quadratic in nesting depth and Qt's layout is super-linear, so mail
//! nested deeper than [`MAX_DEPTH`] never reaches them.

/// Deepest element nesting we hand to ammonia and Qt.
pub(crate) const MAX_DEPTH: usize = 256;

/// Elements that have no end tag.
const VOID: &[&str] = &[
    "br", "img", "hr", "meta", "link", "input", "wbr", "col", "area", "base", "source", "track",
    "embed",
];

/// Whether the approximate element nesting of `html` goes above
/// [`MAX_DEPTH`]. Start tags count up, end tags down; void elements,
/// `/>` tags and comments are skipped. One linear pass.
pub(crate) fn too_deep(html: &str) -> bool {
    let bytes = html.as_bytes();
    let mut depth = 0usize;
    let mut i = 0;
    while let Some(offset) = bytes[i..].iter().position(|&b| b == b'<') {
        i += offset;
        let rest = &bytes[i + 1..];
        match rest.first() {
            Some(b'!') if rest.starts_with(b"!--") => {
                i += 4 + find(&rest[3..], b"-->").map_or(rest.len(), |p| p + 3);
            }
            Some(b'/') => {
                depth = depth.saturating_sub(1);
                i += 2;
            }
            Some(b) if b.is_ascii_alphabetic() => {
                let name_len = rest
                    .iter()
                    .position(|c| !c.is_ascii_alphanumeric())
                    .unwrap_or(rest.len());
                let name = String::from_utf8_lossy(&rest[..name_len]).to_ascii_lowercase();
                let end = rest.iter().position(|&c| c == b'>');
                let self_closing = end.is_some_and(|e| e > 0 && rest[e - 1] == b'/');
                if !VOID.contains(&name.as_str()) && !self_closing {
                    depth += 1;
                    if depth > MAX_DEPTH {
                        return true;
                    }
                }
                i += 1 + end.unwrap_or(rest.len());
            }
            _ => i += 1,
        }
    }
    false
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}
