//! Qt 5's rich text paints a block container's `background-color` only
//! behind the first block inside it; the blocks after it are unpainted, so
//! light text in them lands on the white page (#161). Linearised cells
//! (`linear::linearise`) have exactly this shape. This pass gives each
//! block inside a coloured container the container's colour, unless the
//! block has a background of its own.
//!
//! Works on the balanced, double-quoted markup the earlier passes leave,
//! in one forward pass with a stack of open elements. The colour written
//! is always `#rrggbb` from [`crate::colours::paint`], never the input
//! text.

use crate::colours::{Paint, paint};
use crate::images::{Tag, parse_tag, tag_len};

/// Blocks Qt lays out on their own, which take the colour and pass it on.
const BLOCKS: &[&str] = &[
    "address",
    "blockquote",
    "center",
    "dd",
    "div",
    "dl",
    "dt",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "li",
    "ol",
    "p",
    "pre",
    "td",
    "th",
    "ul",
];

/// Pass a background of their own on to their cells but take none.
const ROWS: &[&str] = &["table", "tr"];

/// Elements without an end tag.
const VOID: &[&str] = &["br", "col", "hr", "img"];

/// An element's own background.
enum Own {
    /// None declared.
    None,
    /// A `background-color` in `style` that paints nothing: the colour
    /// pushed down replaces it.
    Clear,
    /// Any other declaration, or a `bgcolor`: left as it is.
    Set(Paint),
}

/// `html` with each block inside a container with an opaque background
/// given that background, nearest container first. Inline elements pass
/// it through; a block's own background, opaque or not, is never
/// overridden (only a transparent `background-color` is replaced), and a
/// background that is not opaque passes nothing on.
pub(crate) fn fill(html: &str) -> String {
    if !html.contains("background-color") && !html.contains("bgcolor") {
        return html.to_owned();
    }
    let mut out = String::with_capacity(html.len());
    // Each open element's name and the colour its content inherits.
    let mut open: Vec<(String, Option<String>)> = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let len = tag_len(&rest[start..]);
        let raw = &rest[start..start + len];
        rest = &rest[start + len..];
        if let Some(end) = raw.strip_prefix("</") {
            close(&mut open, name_of(end));
            out.push_str(raw);
            continue;
        }
        let Some(mut tag) = parse_tag(raw) else {
            out.push_str(raw);
            continue;
        };
        let name = tag.name.as_str();
        if VOID.contains(&name) {
            out.push_str(raw);
            continue;
        }
        let inherited = open.last().and_then(|(_, bg)| bg.clone());
        let block = BLOCKS.contains(&name);
        let own = if block || ROWS.contains(&name) {
            own(&tag)
        } else {
            Own::None
        };
        let bg = match &own {
            Own::Set(Paint::Opaque(colour)) => Some(colour.clone()),
            Own::Set(_) => None,
            Own::None | Own::Clear => inherited,
        };
        match &bg {
            Some(colour) if block && !matches!(own, Own::Set(_)) => {
                give(&mut tag, colour);
                tag.write(&mut out);
            }
            _ => out.push_str(raw),
        }
        open.push((tag.name, bg));
    }
    out.push_str(rest);
    out
}

/// The tag's own background: the last `background-color` in its `style`
/// (Qt applies CSS after attributes), else its `bgcolor`.
fn own(tag: &Tag) -> Own {
    let declared = tag
        .attr("style")
        .and_then(|style| style.split(';').rev().find_map(background_value).map(paint));
    match declared {
        Some(Paint::Clear) => Own::Clear,
        Some(paint) => Own::Set(paint),
        None => tag
            .attr("bgcolor")
            .map_or(Own::None, |value| Own::Set(paint(value))),
    }
}

/// The value of a `background-color` declaration.
fn background_value(declaration: &str) -> Option<&str> {
    let (prop, value) = declaration.split_once(':')?;
    prop.trim()
        .eq_ignore_ascii_case("background-color")
        .then_some(value)
}

/// Sets `colour` as the tag's background, first in its `style` so the rest
/// keeps its place, replacing any `background-color` it had.
fn give(tag: &mut Tag, colour: &str) {
    let mut style = format!("background-color:{colour}");
    let index = tag.attrs.iter().position(|(n, _)| n == "style");
    if let Some(i) = index {
        for declaration in tag.attrs[i].1.split(';') {
            if background_value(declaration).is_none() && !declaration.trim().is_empty() {
                style.push(';');
                style.push_str(declaration);
            }
        }
        tag.attrs[i].1 = style;
    } else {
        tag.attrs.push(("style".to_owned(), style));
    }
}

/// Closes the innermost open element called `name`, with any left open
/// inside it; an end tag with no open element is ignored.
fn close(open: &mut Vec<(String, Option<String>)>, name: &str) {
    if let Some(at) = open.iter().rposition(|(n, _)| n.eq_ignore_ascii_case(name)) {
        open.truncate(at);
    }
}

/// The element name at the start of `s` (after `</`).
fn name_of(s: &str) -> &str {
    let len = s
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(s.len());
    &s[..len]
}
