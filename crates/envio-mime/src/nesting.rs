//! A cheap guard against deeply nested HTML. ammonia (html5ever) is
//! quadratic in the depth of its stack of open elements and Qt's layout is
//! super-linear, so mail nested deeper than [`MAX_DEPTH`] never reaches
//! them. The scan below keeps a small model of that stack.

/// Deepest element nesting we hand to ammonia and Qt.
pub(crate) const MAX_DEPTH: usize = 256;

/// Elements that have no end tag.
const VOID: &[&str] = &[
    "br", "img", "hr", "meta", "link", "input", "wbr", "col", "area", "base", "source", "track",
    "embed", "param", "keygen",
];

/// Elements whose end tag can be implied by the next start tag.
const IMPLIED_END: &[&str] = &[
    "p", "li", "dt", "dd", "td", "th", "tr", "option", "optgroup", "thead", "tbody", "tfoot", "rb",
    "rt", "rp",
];

/// Elements whose content is text, not tags.
const RAW_TEXT: &[&str] = &[
    "script",
    "style",
    "textarea",
    "title",
    "xmp",
    "iframe",
    "noembed",
    "noframes",
    "noscript",
    "plaintext",
];

/// How far back a start tag looks for an element it implicitly closes.
const IMPLIED_WINDOW: usize = 8;
const NAME_MAX: usize = 16;

/// Whether the approximate element nesting of `html` goes above
/// [`MAX_DEPTH`]. One linear pass over the bytes; truncated input ends the
/// scan (nothing here indexes without checking).
pub(crate) fn too_deep(html: &str) -> bool {
    let bytes = html.as_bytes();
    let mut stack: Vec<String> = Vec::new();
    let mut i = 0;
    while let Some(lt) = find(bytes, i, b"<") {
        let after = lt + 1;
        match bytes.get(after) {
            Some(b'!') if bytes[after..].starts_with(b"!--") => {
                match find(bytes, after + 3, b"-->") {
                    Some(end) => i = end + 3,
                    None => return false,
                }
            }
            Some(b'!' | b'?') => match find(bytes, after, b">") {
                Some(end) => i = end + 1,
                None => return false,
            },
            Some(b'/') => {
                let name = tag_name(bytes, after + 1);
                let Some(end) = find(bytes, after + 1, b">") else {
                    return false;
                };
                if !name.is_empty()
                    && let Some(open) = stack.iter().rposition(|n| *n == name)
                {
                    stack.truncate(open);
                }
                i = end + 1;
            }
            Some(b) if b.is_ascii_alphabetic() => {
                let name = tag_name(bytes, after);
                let Some(end) = tag_end(bytes, after) else {
                    return false;
                };
                i = end + 1;
                let self_closing = end > 0 && bytes.get(end - 1) == Some(&b'/');
                if VOID.contains(&name.as_str()) || self_closing {
                    continue;
                }
                if IMPLIED_END.contains(&name.as_str()) {
                    let from = stack.len().saturating_sub(IMPLIED_WINDOW);
                    if let Some(open) = stack[from..].iter().rposition(|n| *n == name) {
                        stack.truncate(from + open);
                    }
                }
                let raw = RAW_TEXT.contains(&name.as_str());
                stack.push(name.clone());
                if stack.len() > MAX_DEPTH {
                    return true;
                }
                if raw {
                    // Skip the content up to the end tag, which pops it.
                    let close = format!("</{name}");
                    match find_ci(bytes, i, close.as_bytes()) {
                        Some(at) => i = at,
                        None => return false,
                    }
                }
            }
            _ => i = after,
        }
    }
    false
}

/// Lower-case tag name at `start` (ASCII letters and digits); more than
/// [`NAME_MAX`] bytes counts as one shared name.
fn tag_name(bytes: &[u8], start: usize) -> String {
    let rest = bytes.get(start..).unwrap_or_default();
    let len = rest
        .iter()
        .position(|b| !b.is_ascii_alphanumeric())
        .unwrap_or(rest.len());
    if len > NAME_MAX {
        return "~long".to_owned();
    }
    String::from_utf8_lossy(&rest[..len]).to_ascii_lowercase()
}

/// Index of the `>` that ends a start tag, skipping `>` inside quoted
/// attribute values (a quote counts only right after `=`). `None` when the
/// tag is cut off.
fn tag_end(bytes: &[u8], from: usize) -> Option<usize> {
    let mut quote = None;
    let mut prev = b' ';
    for (offset, &b) in bytes.get(from..)?.iter().enumerate() {
        match quote {
            Some(q) => {
                if b == q {
                    quote = None;
                }
            }
            None if b == b'>' => return Some(from + offset),
            None if (b == b'"' || b == b'\'') && prev == b'=' => quote = Some(b),
            None => {}
        }
        // Spaces between `=` and the quote are allowed.
        if quote.is_none() && !b.is_ascii_whitespace() {
            prev = b;
        }
    }
    None
}

fn find(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    let hay = bytes.get(from..)?;
    hay.windows(needle.len())
        .position(|w| w == needle)
        .map(|p| from + p)
}

fn find_ci(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    let hay = bytes.get(from..)?;
    hay.windows(needle.len())
        .position(|w| w.eq_ignore_ascii_case(needle))
        .map(|p| from + p)
}
