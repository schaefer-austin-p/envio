//! The one-line preview shown in the message list.

/// Longest preview, in characters.
pub const PREVIEW_CHARS: usize = 200;

/// The first [`PREVIEW_CHARS`] characters of `text`, whitespace collapsed,
/// `>` quoted lines dropped, and nothing from the signature line (`-- `) on.
pub fn preview(text: &str) -> String {
    let mut out = String::new();
    let mut count = 0;
    for line in text.lines() {
        if is_signature_line(line) {
            break;
        }
        if line.trim_start().starts_with('>') {
            continue;
        }
        for word in line.split_whitespace() {
            if !push_word(&mut out, &mut count, word) {
                return out;
            }
        }
    }
    out
}

/// The RFC 3676 signature separator, also when a client trimmed its space.
pub(crate) fn is_signature_line(line: &str) -> bool {
    line == "-- " || line.trim_end() == "--"
}

/// Appends `word` (with a separating space) up to the limit; false once full.
fn push_word(out: &mut String, count: &mut usize, word: &str) -> bool {
    if !out.is_empty() {
        if *count >= PREVIEW_CHARS {
            return false;
        }
        out.push(' ');
        *count += 1;
    }
    for c in word.chars() {
        if *count >= PREVIEW_CHARS {
            return false;
        }
        out.push(c);
        *count += 1;
    }
    true
}
