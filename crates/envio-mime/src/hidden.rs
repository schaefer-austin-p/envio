//! Removes elements that CSS hides (`display:none`, `visibility:hidden`),
//! working on the balanced markup ammonia serializes.

use crate::images::{parse_tag, tag_len};
use crate::sanitize::TAGS;

/// Elements that have no end tag.
const VOID: &[&str] = &["br", "hr", "img"];

/// Bytes to skip after the start tag of a hidden `name` element: its
/// content and its end tag. Nested elements of the same name are counted;
/// without a matching end tag the rest of the input goes.
pub(crate) fn skip_len(after_start_tag: &str, name: &str) -> usize {
    if VOID.contains(&name) {
        return 0;
    }
    let mut depth = 1usize;
    let mut pos = 0;
    while let Some(offset) = after_start_tag[pos..].find('<') {
        let start = pos + offset;
        let len = tag_len(&after_start_tag[start..]);
        let raw = &after_start_tag[start..start + len];
        pos = start + len;
        if let Some(end) = raw.strip_prefix("</") {
            if end.trim_end_matches('>').trim().eq_ignore_ascii_case(name) {
                depth -= 1;
                if depth == 0 {
                    return pos;
                }
            }
        } else if parse_tag(raw).is_some_and(|tag| tag.name == name) {
            depth += 1;
        }
    }
    after_start_tag.len()
}

/// Whether `raw` (a start or end tag) is of a name ammonia only let through
/// so the style check could see it: the tag goes, its content stays.
pub(crate) fn is_unwrapped(raw: &str) -> bool {
    let name = raw.trim_start_matches('<').trim_start_matches('/');
    let end = name
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .unwrap_or(name.len());
    end > 0 && !TAGS.contains(&&name[..end])
}
