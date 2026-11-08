//! A cheap guard against deeply nested HTML. ammonia (html5ever) is
//! quadratic in the depth of its stack of open elements and Qt's layout is
//! super-linear, so mail nested deeper than [`MAX_DEPTH`] never reaches
//! them.
//!
//! The scan keeps a model of that stack that must never count *less* than
//! the parser: where html5ever's rules are subtle (adoption agency, scope
//! boundaries, foreign content) the model keeps the element open. Counting
//! too much only sends very odd mail to the plain-text fallback.

/// Deepest element nesting we hand to ammonia and Qt.
pub(crate) const MAX_DEPTH: usize = 256;

/// Whatever the depth, this many start tags is too many.
const MAX_START_TAGS: usize = 50_000;

/// Elements that have no end tag.
const VOID: &[&str] = &[
    "br", "img", "hr", "meta", "link", "input", "wbr", "col", "area", "base", "source", "track",
    "embed", "param", "keygen",
];

/// Elements whose end tag can be implied.
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

/// Elements where `/>` really closes the element (foreign content).
const FOREIGN: &[&str] = &["svg", "math"];

const NAME_MAX: usize = 16;

/// How a start tag ended.
enum TagEnd {
    /// Index of the closing `>`.
    At(usize),
    /// A quoted attribute value never closes.
    OpenQuote,
    /// No `>` before the end of the input.
    Cut,
}

/// Whether the approximate element nesting of `html` goes above
/// [`MAX_DEPTH`] (or it has too many start tags). One linear pass over the
/// bytes, nothing indexes without checking. Constructs that never end are
/// skipped up to the next `>`, so the scan keeps counting wherever the
/// parser might keep building elements.
pub(crate) fn too_deep(html: &str) -> bool {
    let bytes = html.as_bytes();
    let mut stack: Vec<String> = Vec::new();
    let mut starts = 0usize;
    // Raw-text names whose end tag was not found: do not search again.
    let mut no_close: Vec<String> = Vec::new();
    let mut i = 0;
    while let Some(lt) = find(bytes, i, b"<") {
        let after = lt + 1;
        // Resume point for a construct that never ends.
        let resume = |from: usize| find(bytes, from, b">").map(|gt| gt + 1);
        match bytes.get(after) {
            Some(b'!') if bytes[after..].starts_with(b"!--") => {
                match find(bytes, after + 3, b"-->")
                    .map(|end| end + 3)
                    .or_else(|| resume(lt))
                {
                    Some(next) => i = next,
                    None => return false,
                }
            }
            Some(b'!' | b'?') => match resume(after) {
                Some(next) => i = next,
                None => return false,
            },
            Some(b'/') => {
                let name = tag_name(bytes, after + 1);
                let Some(next) = resume(after + 1) else {
                    return false;
                };
                if !name.is_empty() {
                    pop_through(&mut stack, &name);
                }
                i = next;
            }
            Some(b) if b.is_ascii_alphabetic() => {
                let name = tag_name(bytes, after);
                let end = match tag_end(bytes, after + name.len().min(NAME_MAX)) {
                    TagEnd::At(end) => end,
                    TagEnd::OpenQuote => match resume(lt) {
                        Some(gt) => gt - 1,
                        None => return false,
                    },
                    TagEnd::Cut => return false,
                };
                i = end + 1;
                let slash = end > 0 && bytes.get(end - 1) == Some(&b'/');
                if VOID.contains(&name.as_str()) || (slash && FOREIGN.contains(&name.as_str())) {
                    continue;
                }
                starts += 1;
                if starts > MAX_START_TAGS {
                    return true;
                }
                if IMPLIED_END.contains(&name.as_str()) {
                    pop_through(&mut stack, &name);
                }
                stack.push(name.clone());
                if stack.len() > MAX_DEPTH {
                    return true;
                }
                if RAW_TEXT.contains(&name.as_str()) && !no_close.contains(&name) {
                    // Skip the content up to the end tag, which pops it.
                    match find_ci(bytes, i, format!("</{name}").as_bytes()) {
                        Some(at) => i = at,
                        None => no_close.push(name),
                    }
                }
            }
            _ => i = after,
        }
    }
    false
}

/// Closes the nearest open `name`, but only through elements whose end tag
/// can be implied; anything else in the way keeps it open (the parser may
/// nest deeper there, so we never close less than it does).
fn pop_through(stack: &mut Vec<String>, name: &str) {
    for (index, open) in stack.iter().enumerate().rev() {
        if open == name {
            stack.truncate(index);
            return;
        }
        if !IMPLIED_END.contains(&open.as_str()) {
            return;
        }
    }
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

/// Walks the attributes of a start tag the way the tokenizer does: an `=`
/// at the start of an attribute name belongs to the name, and a quote opens
/// a value only after `name =`.
fn tag_end(bytes: &[u8], from: usize) -> TagEnd {
    let ws = |b: u8| b.is_ascii_whitespace();
    let mut i = from;
    loop {
        while bytes.get(i).is_some_and(|&b| ws(b) || b == b'/') {
            i += 1;
        }
        match bytes.get(i) {
            None => return TagEnd::Cut,
            Some(b'>') => return TagEnd::At(i),
            Some(_) => {}
        }
        // Attribute name: the first byte is always part of it.
        i += 1;
        while bytes
            .get(i)
            .is_some_and(|&b| !ws(b) && !matches!(b, b'/' | b'=' | b'>'))
        {
            i += 1;
        }
        let mut j = i;
        while bytes.get(j).is_some_and(|&b| ws(b)) {
            j += 1;
        }
        if bytes.get(j) != Some(&b'=') {
            continue;
        }
        j += 1;
        while bytes.get(j).is_some_and(|&b| ws(b)) {
            j += 1;
        }
        match bytes.get(j) {
            None => return TagEnd::Cut,
            Some(&q) if q == b'"' || q == b'\'' => match find(bytes, j + 1, &[q]) {
                Some(close) => i = close + 1,
                None => return TagEnd::OpenQuote,
            },
            Some(_) => {
                i = j;
                while bytes.get(i).is_some_and(|&b| !ws(b) && b != b'>') {
                    i += 1;
                }
            }
        }
    }
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
