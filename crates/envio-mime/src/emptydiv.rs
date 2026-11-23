//! Empty `div`s go. A layout table's spacer cell (an empty `td` with a
//! `background`) becomes an empty `div` when the table is linearised; a
//! browser sizes it by its CSS height, but Qt rich text ignores heights and
//! gives the empty block one line of its background, which paints a dark
//! bar over the next block (#341). With nothing in it, nothing is lost.

use crate::images::tag_len;

/// `html` without any `div` that holds nothing but whitespace, however deep
/// (a `div` left empty by removing the empty ones in it goes too).
/// `&nbsp;` and other entities are content and stay.
pub(crate) fn drop_empty(html: &str) -> String {
    if !html.contains("</div>") {
        return html.to_owned();
    }
    let mut out = String::with_capacity(html.len());
    // Offsets in `out` of the open `div` start tags, innermost last; a start
    // tag that only whitespace has followed so far is the one a `</div>`
    // would empty.
    let mut open: Vec<usize> = Vec::new();
    let mut pos = 0;
    while let Some(i) = crate::work::find_lt(&html[pos..]) {
        let start = pos + i;
        out.push_str(&html[pos..start]);
        let len = tag_len(&html[start..]);
        let raw = &html[start..start + len];
        pos = start + len;
        let lower = raw.get(..5).map(str::to_ascii_lowercase);
        let named = |n: &str| {
            lower.as_deref().is_some_and(|l| l.starts_with(n))
                && raw[n.len()..].starts_with([' ', '>', '\t', '\n', '\r', '/'])
        };
        if named("<div") {
            open.push(out.len());
            out.push_str(raw);
        } else if named("</div") {
            match open.pop() {
                Some(from) if from != usize::MAX && only_gap(&out, from) => out.truncate(from),
                _ => out.push_str(raw),
            }
        } else {
            // Any other tag is content, so the open divs are not empty.
            out.push_str(raw);
            open.iter_mut().for_each(|o| *o = usize::MAX);
        }
    }
    out.push_str(&html[pos..]);
    out
}

/// Whether nothing but whitespace follows the start tag at `from` in `out`.
fn only_gap(out: &str, from: usize) -> bool {
    out[from + tag_len(&out[from..])..]
        .chars()
        .all(char::is_whitespace)
}
