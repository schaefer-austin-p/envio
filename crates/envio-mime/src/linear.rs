//! Layout tables become blocks. Qt resolves a `width="100%"` table nested
//! in a cell against the cell's minimum width, so the text column of a
//! layout-table notice shrinks to a few characters (#90). A table that only
//! stacks its content has no columns to lay out, so it turns into `div`s,
//! which take the full width.
//!
//! A newsletter built as a 600 px grid keeps its columns side by side at
//! phone width and squeezes their text (#160). Its cells are stacked the
//! same way, as responsive mail does on a small screen: a table whose cells
//! hold pictures, nested tables or blocks, or that calls itself
//! `role="presentation"`, is layout. A table of short text runs, such as a
//! receipt, keeps its columns, and so does a row whose extra columns are
//! narrow (an icon or avatar beside the text).
//!
//! Works on the balanced, double-quoted markup `images::fit` leaves. There
//! is no tree: [`layout_tables`] pre-scans for a verdict per table, and
//! [`linearise`] rewrites with it, both in one forward pass.

use std::borrow::Cow;

use crate::colours::is_visible_attr;
use crate::images::{Placed, parse_tag, pixels, tag_len};
use crate::plain::escape_into;

/// The most pixels a row's side columns may take, together, for the row to
/// keep its columns: what is left of a 360 px phone screen stays readable.
const SIDE_MAX: u32 = 120;

/// Content that makes a table cell layout rather than a run of text.
const BLOCKS: &[&str] = &[
    "blockquote",
    "center",
    "div",
    "dl",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "ol",
    "p",
    "pre",
    "ul",
];

/// What the pre-scan tracks for a table that is still open.
#[derive(Default)]
struct Frame {
    /// Index of its entry in the verdicts.
    index: usize,
    /// It has a `th` or a `caption`: a data table, whatever else it holds.
    data: bool,
    rows: usize,
    /// Cells so far in the open row; `None` between rows.
    cells: Option<usize>,
    /// A row without exactly one cell, or a cell outside a row.
    columns: bool,
    /// `role="presentation"`, or its own content has a picture, a nested
    /// table or a block.
    layout: bool,
    /// Its own content has a picture, a nested table or a block (unlike
    /// `layout`, `role="presentation"` does not set it).
    rich: bool,
    /// A cell has padding and a background or border: a button (#111).
    boxed: bool,
    /// Its `cellpadding` is a non-zero number.
    cellpad: bool,
    /// A row with more than one cell has wide side columns: more than one
    /// cell without a pixel width, or the cells beside the widest one
    /// over [`SIDE_MAX`] together.
    wide: bool,
    /// The open row's cells without a pixel width.
    free: usize,
    /// The open row's pixel widths: their sum and the largest.
    fixed: u32,
    widest: u32,
    /// The open cell, if any.
    cell: Option<Cell>,
}

/// What the pre-scan tracks for a cell that is still open.
#[derive(Default)]
struct Cell {
    /// Its `width` in pixels.
    width: Option<u32>,
    /// The widths of the pictures in it, summed.
    pictures: Option<u32>,
    /// It holds text, a picture without a width, a block or a nested table.
    open_width: bool,
}

impl Cell {
    /// Its pixel width: its own, else that of the pictures it holds alone
    /// (an icon cell often leaves the width to its image).
    fn pixels(&self) -> Option<u32> {
        self.width.or(self.pictures.filter(|_| !self.open_width))
    }
}

impl Frame {
    /// A picture, a block or a nested table in its own content; a picture
    /// has `Some(width)`, the width it declares.
    fn content(&mut self, picture: Option<Option<u32>>) {
        self.layout = true;
        self.rich = true;
        if let Some(cell) = &mut self.cell {
            match picture {
                Some(Some(px)) => {
                    cell.pictures = Some(cell.pictures.unwrap_or(0).saturating_add(px));
                }
                _ => cell.open_width = true,
            }
        }
    }

    /// Text in its own content (not an image's label): the open cell's
    /// width is the text's, not its pictures'.
    fn text(&mut self, text: &str) {
        if let Some(cell) = &mut self.cell
            && !text.replace("&nbsp;", " ").trim().is_empty()
        {
            cell.open_width = true;
        }
    }

    fn start_cell(&mut self, raw: &str) {
        match self.cells {
            Some(n) => self.cells = Some(n + 1),
            None => self.columns = true,
        }
        if self.cell.is_some() {
            self.end_cell();
        }
        self.boxed |= boxed_cell(raw, self.cellpad);
        self.cell = Some(Cell {
            width: cell_width(raw),
            ..Cell::default()
        });
    }

    fn end_cell(&mut self) {
        match self.cell.take().and_then(|cell| cell.pixels()) {
            Some(px) => {
                self.fixed = self.fixed.saturating_add(px);
                self.widest = self.widest.max(px);
            }
            None => self.free += 1,
        }
    }

    fn end_row(&mut self) {
        if self.cell.is_some() {
            self.end_cell();
        }
        if self.cells != Some(1) {
            self.columns = true;
            let sides = match self.free {
                0 => self.fixed.saturating_sub(self.widest),
                1 => self.fixed,
                _ => u32::MAX,
            };
            self.wide |= self.cells.unwrap_or(0) > 1 && sides > SIDE_MAX;
        }
        self.cells = None;
    }

    /// Single column, or columns that only lay out pictures and blocks too
    /// wide for a phone.
    fn linear(&self) -> bool {
        // Qt ignores a div's padding, so a table of one row with one boxed
        // text cell (a button) stays a table: it honours cell padding.
        let button = self.boxed && !self.rich && self.rows == 1 && !self.columns;
        !button && !self.data && self.rows > 0 && (!self.columns || (self.layout && self.wide))
    }
}

/// Whether each table is a layout table to write as blocks, one entry per
/// `<table` start tag, in order. Such a table has no `th` and no `caption`,
/// and either every one of its own rows has exactly one of its own cells
/// (any `colspan`), or it is layout and some row has wide side columns
/// (see [`Frame`]). `images` are those `images::fit` wrote, in order
/// (blocked ones are only text by now). Nested tables are judged on their
/// own.
pub(crate) fn layout_tables(html: &str, images: &[Placed]) -> Vec<bool> {
    let mut verdicts = Vec::new();
    let mut open: Vec<Frame> = Vec::new();
    let mut images = images.iter().copied().peekable();
    let mut pos = 0;
    while let Some((start, end)) = next_tag(html, pos) {
        // The text before this tag, around the images' labels.
        let mut from = pos;
        while let Some(image) = images.next_if(|image| image.at <= start) {
            if let Some(frame) = open.last_mut() {
                frame.text(&html[from..image.at.max(from)]);
                frame.content(Some(image.width));
            }
            from = from.max(image.end);
        }
        if let Some(frame) = open.last_mut() {
            frame.text(&html[from..start.max(from)]);
        }
        pos = end;
        let raw = &html[start..end];
        let (closing, name) = name_of(raw);
        if name == "table" {
            if closing {
                if let Some(frame) = open.pop() {
                    verdicts[frame.index] = frame.linear();
                }
            } else {
                if let Some(outer) = open.last_mut() {
                    outer.content(None);
                }
                open.push(Frame {
                    index: verdicts.len(),
                    layout: presentation(raw),
                    cellpad: cellpadding(raw),
                    ..Frame::default()
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
                frame.free = 0;
                frame.fixed = 0;
                frame.widest = 0;
            }
            (true, "tr") => frame.end_row(),
            (false, "td") => frame.start_cell(raw),
            (true, "td") if frame.cell.is_some() => frame.end_cell(),
            (false, "th" | "caption") => frame.data = true,
            (false, name) if BLOCKS.contains(&name) => frame.content(None),
            _ => {}
        }
    }
    verdicts
}

/// Whether the `table` start tag `raw` has `role="presentation"` or
/// `role="none"`.
fn presentation(raw: &str) -> bool {
    parse_tag(raw).is_some_and(|tag| {
        tag.attr("role").is_some_and(|role| {
            let role = role.trim();
            role.eq_ignore_ascii_case("presentation") || role.eq_ignore_ascii_case("none")
        })
    })
}

/// Whether the `table` start tag `raw` has a non-zero `cellpadding`.
fn cellpadding(raw: &str) -> bool {
    parse_tag(raw)
        .and_then(|tag| tag.attr("cellpadding").map(str::to_owned))
        .is_some_and(|v| {
            v.trim()
                .trim_end_matches('%')
                .parse::<f64>()
                .is_ok_and(|n| n > 0.0)
        })
}

/// Whether the cell start tag `raw` has real padding (a non-zero length in
/// `padding` or a `padding-*`, or the table's `cellpadding`) and a background (a `bgcolor` that
/// [`bgcolor`] accepts, or a `background-color` in its `style`, that is not
/// transparent): a button.
/// `filter_style` drops `border`, so it never counts.
fn boxed_cell(raw: &str, cellpad: bool) -> bool {
    let Some(tag) = parse_tag(raw) else {
        return false;
    };
    let style = tag.attr("style").unwrap_or("").to_ascii_lowercase();
    let mut padded = cellpad;
    let mut background = bgcolor(&tag.attrs).is_some_and(|c| is_visible_attr(&c));
    for declaration in style.split(';') {
        let Some((prop, value)) = declaration.split_once(':') else {
            continue;
        };
        let (prop, value) = (prop.trim(), value.trim());
        if prop == "background-color" && is_visible_attr(value) {
            background = true;
        }
        if prop.starts_with("padding") {
            padded |= value
                .split(|c: char| !(c.is_ascii_digit() || c == '.'))
                .any(|n| n.parse::<f64>().is_ok_and(|n| n > 0.0));
        }
    }
    padded && background
}

/// The cell's `width` in pixels; `None` for a percentage or none.
fn cell_width(raw: &str) -> Option<u32> {
    parse_tag(raw)?.attr("width").and_then(pixels)
}

/// `html` with every table [`layout_tables`] calls a layout table written as
/// `div`s: the table and its cells keep `align` and `style`, its own
/// `tbody`, `thead`, `tfoot`, `tr`, `colgroup` and `col` tags go, and the
/// content stays in order.
pub(crate) fn linearise(html: &str, images: &[Placed]) -> String {
    let verdicts = layout_tables(html, images);
    if !verdicts.contains(&true) {
        return html.to_owned();
    }
    let mut out = String::with_capacity(html.len());
    // Whether each open table is being linearised; the top owns what is read.
    let mut open: Vec<bool> = Vec::new();
    let mut tables = 0;
    // The valid `bgcolor` of the open linearised row, for cells without one.
    let mut row_bg: Option<String> = None;
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
                    block(raw, None, true, &mut out);
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
            (false, "tr") if own => {
                row_bg = parse_tag(raw).and_then(|t| bgcolor(&t.attrs).map(Cow::into_owned));
            }
            (true, "tr") if own => row_bg = None,
            (_, "tbody" | "thead" | "tfoot" | "colgroup" | "col") if own => {}
            (false, "td" | "th") if own => block(raw, row_bg.as_deref(), false, &mut out),
            (true, "td" | "th") if own => out.push_str("</div>"),
            _ => out.push_str(raw),
        }
    }
    out.push_str(&html[pos..]);
    out
}

/// Writes a `div` start tag with the `align` and `style` of the start tag
/// `raw`; a `table`'s `align` places the table box, not its text, so it is
/// dropped. Its `bgcolor` (else `row_bg`, the row's) leads the style as
/// `background-color`, so the tag's own style still wins.
fn block(raw: &str, row_bg: Option<&str>, table: bool, out: &mut String) {
    out.push_str("<div");
    if let Some(tag) = parse_tag(raw) {
        let own_bg = bgcolor(&tag.attrs);
        let bg = own_bg.as_deref().or(row_bg);
        let mut styled = false;
        for (name, value) in &tag.attrs {
            if name == "align" && !table {
                attr("align", None, value, out);
            } else if name == "style" && !styled {
                styled = true;
                attr("style", bg, value, out);
            }
        }
        if !styled && let Some(bg) = bg {
            attr("style", Some(bg), "", out);
        }
    }
    out.push('>');
}

/// Writes ` name="[background-color:bg;]value"`, escaped.
fn attr(name: &str, bg: Option<&str>, value: &str, out: &mut String) {
    out.push(' ');
    out.push_str(name);
    out.push_str("=\"");
    if let Some(bg) = bg {
        out.push_str("background-color:");
        escape_into(bg, out);
        out.push(';');
    }
    escape_into(value, out);
    out.push('"');
}

/// The `bgcolor` value if it is a colour: `#` and 3 or 6 hex digits, bare
/// hex (written with its `#`), or ASCII letters only (a name; none uses
/// only a-f). Anything else could smuggle CSS.
fn bgcolor(attrs: &[(String, String)]) -> Option<Cow<'_, str>> {
    let value = attrs.iter().find(|(n, _)| n == "bgcolor")?.1.as_str();
    let hex = |h: &str| matches!(h.len(), 3 | 6) && h.bytes().all(|b| b.is_ascii_hexdigit());
    match value.strip_prefix('#') {
        Some(h) => hex(h).then_some(Cow::Borrowed(value)),
        None if hex(value) => Some(Cow::Owned(format!("#{value}"))),
        None => (!value.is_empty() && value.bytes().all(|b| b.is_ascii_alphabetic()))
            .then_some(Cow::Borrowed(value)),
    }
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
        assert_eq!(layout_tables(html, &[]), [false]);
    }

    #[test]
    fn one_verdict_per_table_in_start_order() {
        let html = "<table><tr><td><table><tr><td>a</td><td>b</td></tr></table></td></tr></table>\
                    <table><tr><td>c</td></tr></table>";
        assert_eq!(layout_tables(html, &[]), [true, false, true]);
    }
}
