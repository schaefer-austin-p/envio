//! Removes elements that CSS hides (`display:none`, `visibility:hidden`),
//! working on the balanced markup ammonia serializes.

use std::collections::HashMap;

use crate::images::{parse_tag, tag_len};
use crate::sanitize::TAGS;

/// Elements html5ever 0.40.1's serializer writes without an end tag (its
/// `serialize/mod.rs`, `ignore_children`).
const VOID: &[&str] = &[
    "area", "base", "basefont", "bgsound", "br", "col", "embed", "frame", "hr", "img", "input",
    "keygen", "link", "meta", "param", "source", "track", "wbr",
];

/// Where each element ends, found in one forward pass with a stack of open
/// tags: linear in the input, whatever it holds. A start tag with no
/// matching end tag has no entry, and its lookup is O(1), so nothing is
/// ever rescanned.
pub(crate) struct Ends(HashMap<usize, usize>);

impl Ends {
    pub(crate) fn new(html: &str) -> Self {
        let mut ends = HashMap::new();
        // Open elements: name and the offset of their start tag.
        let mut open: Vec<(String, usize)> = Vec::new();
        let mut pos = 0;
        while let Some(offset) = html[pos..].find('<') {
            let start = pos + offset;
            let len = tag_len(&html[start..]);
            let raw = &html[start..start + len];
            pos = start + len;
            if let Some(end) = raw.strip_prefix("</") {
                let name = end.trim_end_matches('>').trim().to_ascii_lowercase();
                if let Some((_, from)) = open.pop_if(|(top, _)| *top == name) {
                    ends.insert(from, pos);
                }
            } else if let Some(tag) = parse_tag(raw).filter(|t| !VOID.contains(&t.name.as_str())) {
                open.push((tag.name, start));
            }
        }
        Self(ends)
    }

    /// Offset just after the end tag of the element whose start tag is at
    /// `start`; `None` when it has none (void, or malformed).
    pub(crate) fn end_of(&self, start: usize) -> Option<usize> {
        self.0.get(&start).copied()
    }
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
