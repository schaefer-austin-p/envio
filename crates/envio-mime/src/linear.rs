//! Single-column layout tables become blocks. Qt resolves a `width="100%"`
//! table nested in a cell against the cell's minimum width, so the text
//! column of a layout-table notice shrinks to a few characters (#90). A
//! table that only stacks its content has no columns to lay out, so it
//! turns into `div`s, which take the full width.
//!
//! Works on the balanced, double-quoted markup `images::fit` leaves. There
//! is no tree: [`layout_tables`] pre-scans for a verdict per table, and
//! [`linearise`] rewrites with it, both in one forward pass.

use crate::images::{parse_tag, tag_len};
use crate::plain::escape_into;

/// What the pre-scan tracks for a table that is still open.
struct Frame {
    /// Index of its entry in the verdicts.
    index: usize,
    /// It has a `th`, a `caption`, a row without exactly one cell, or no row.
    data: bool,
    rows: usize,
    /// Cells so far in the open row; `None` between rows.
    cells: Option<usize>,
}

/// Whether each table is a single-column layout table, one entry per
/// `<table` start tag, in order. Such a table has no `th` and no `caption`,
/// and every one of its own rows has exactly one of its own cells (any
/// `colspan`). Nested tables are judged on their own.
pub(crate) fn layout_tables(html: &str) -> Vec<bool> {
    let mut verdicts = Vec::new();
    let mut open: Vec<Frame> = Vec::new();
    let mut pos = 0;
    while let Some((start, end)) = next_tag(html, pos) {
        pos = end;
        let (closing, name) = name_of(&html[start..end]);
        if name == "table" {
            if closing {
                if let Some(frame) = open.pop() {
                    verdicts[frame.index] = !frame.data && frame.rows > 0;
                }
            } else {
                open.push(Frame {
                    index: verdicts.len(),
                    data: false,
                    rows: 0,
                    cells: None,
                });
                verdicts.push(false);
            }
            continue;
        }
        let Some(frame) = open.last_mut() else {
            continue;
        };
        match (closing, name) {
            (false, "tr") => {
                frame.rows += 1;
                frame.cells = Some(0);
            }
            (true, "tr") => {
                frame.data |= frame.cells != Some(1);
                frame.cells = None;
            }
            (false, "td") => match frame.cells {
                Some(n) => {
                    frame.cells = Some(n + 1);
                    frame.data |= n > 0;
                }
                None => frame.data = true,
            },
            (false, "th" | "caption") => frame.data = true,
            _ => {}
        }
    }
    verdicts
}

/// `html` with every table [`layout_tables`] calls a layout table written as
/// `div`s: the table and its cells keep `align` and `style`, its own
/// `tbody`, `thead`, `tfoot`, `tr`, `colgroup` and `col` tags go, and the
/// content stays in order.
pub(crate) fn linearise(html: &str) -> String {
    let verdicts = layout_tables(html);
    if !verdicts.contains(&true) {
        return html.to_owned();
    }
    let mut out = String::with_capacity(html.len());
    // Whether each open table is being linearised; the top owns what is read.
    let mut open: Vec<bool> = Vec::new();
    let mut tables = 0;
    let mut pos = 0;
    while let Some((start, end)) = next_tag(html, pos) {
        out.push_str(&html[pos..start]);
        pos = end;
        let raw = &html[start..end];
        let (closing, name) = name_of(raw);
        let own = open.last().copied().unwrap_or(false);
        match (closing, name) {
            (false, "table") => {
                let linear = verdicts.get(tables).copied().unwrap_or(false);
                tables += 1;
                open.push(linear);
                if linear {
                    block(raw, &mut out);
                } else {
                    out.push_str(raw);
                }
            }
            (true, "table") => {
                if open.pop().unwrap_or(false) {
                    out.push_str("</div>");
                } else {
                    out.push_str(raw);
                }
            }
            (_, "tbody" | "thead" | "tfoot" | "tr" | "colgroup" | "col") if own => {}
            (false, "td" | "th") if own => block(raw, &mut out),
            (true, "td" | "th") if own => out.push_str("</div>"),
            _ => out.push_str(raw),
        }
    }
    out.push_str(&html[pos..]);
    out
}

/// Writes a `div` start tag with the `align` and `style` of the start tag `raw`.
fn block(raw: &str, out: &mut String) {
    out.push_str("<div");
    if let Some(tag) = parse_tag(raw) {
        for (name, value) in &tag.attrs {
            if name == "align" || name == "style" {
                out.push(' ');
                out.push_str(name);
                out.push_str("=\"");
                escape_into(value, out);
                out.push('"');
            }
        }
    }
    out.push('>');
}

/// The byte range of the first tag at or after `pos`. Every `<` starts a
/// tag: ammonia escapes `<` in text.
fn next_tag(html: &str, pos: usize) -> Option<(usize, usize)> {
    let start = pos + html[pos..].find('<')?;
    Some((start, start + tag_len(&html[start..])))
}

/// Whether `raw` is an end tag, and its name.
fn name_of(raw: &str) -> (bool, &str) {
    let inner = raw.strip_prefix('<').unwrap_or(raw);
    let (closing, inner) = match inner.strip_prefix('/') {
        Some(rest) => (true, rest),
        None => (false, inner),
    };
    let len = inner
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(inner.len());
    (closing, &inner[..len])
}

#[cfg(test)]
mod tests {
    use super::layout_tables;

    // Ammonia strips `caption`, so only a unit test reaches that rule.
    #[test]
    fn a_caption_makes_a_data_table() {
        let html = "<table><caption>c</caption><tr><td>a</td></tr></table>";
        assert_eq!(layout_tables(html), [false]);
    }

    #[test]
    fn one_verdict_per_table_in_start_order() {
        let html = "<table><tr><td><table><tr><td>a</td><td>b</td></tr></table></td></tr></table>\
                    <table><tr><td>c</td></tr></table>";
        assert_eq!(layout_tables(html), [true, false, true]);
    }
}
