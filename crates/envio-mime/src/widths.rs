//! CSS `width` on tables, cells and columns, moved to the `width`
//! attribute. Qt's rich text ignores CSS widths on them, so without this
//! a layout table's spacer cells and text columns all get the same share
//! and the text collapses to a one-character strip.

use crate::images::{Tag, pixels};

/// Moves a CSS `width` from the tag's `style` to its `width` attribute.
///
/// Only `Npx` (N > 0) and `N%` (0 < N <= 100) are carried; any other
/// unit stays out. A pixel width above `max` (when `max > 0`) becomes
/// `100%`, so a 600 px card fills a 360 px screen. An existing `width`
/// attribute wins. The CSS declaration is removed from the style either way.
pub(crate) fn carry(tag: &mut Tag, max: u32) {
    let Some(style) = tag.attr("style") else {
        return;
    };
    let mut width = None;
    let kept: Vec<&str> = style
        .split(';')
        .filter(|declaration| match declaration.split_once(':') {
            Some((prop, value)) if prop.trim().eq_ignore_ascii_case("width") => {
                width = Some(value.trim().to_owned());
                false
            }
            _ => true,
        })
        .collect();
    let Some(width) = width else {
        return;
    };
    let kept = kept.join(";");
    tag.attrs.retain(|(name, _)| name != "style");
    if !kept.is_empty() {
        tag.attrs.push(("style".to_owned(), kept));
    }
    if tag.attr("width").is_some() {
        return;
    }
    if let Some(value) = attribute_value(&width, max) {
        tag.attrs.push(("width".to_owned(), value));
    }
}

/// A `width` attribute the message set, as it may stay: a percentage in
/// (0, 100], or whole pixels above 0 and (when `max > 0`) at most `max`,
/// written as bare digits. `None` drops it, so the column sizes itself.
pub(crate) fn sane_attribute(value: &str, max: u32) -> Option<String> {
    let value = value.trim();
    if value.ends_with('%') {
        return attribute_value(value, max);
    }
    pixels(value)
        .filter(|&w| w > 0 && (max == 0 || w <= max))
        .map(|w| w.to_string())
}

/// The `width` attribute for a CSS width value, `None` when not carried.
fn attribute_value(css: &str, max: u32) -> Option<String> {
    let css = css.to_ascii_lowercase();
    let css = css.strip_suffix("!important").unwrap_or(&css).trim();
    if let Some(percent) = css.strip_suffix('%') {
        let percent = percent.trim();
        if percent.is_empty() || !percent.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return None;
        }
        let n = percent.parse::<f64>().ok()?;
        return (n > 0.0 && n <= 100.0).then(|| format!("{percent}%"));
    }
    if !css.ends_with("px") {
        return None;
    }
    match pixels(css)? {
        0 => None,
        w if max > 0 && w > max => Some("100%".to_owned()),
        w => Some(w.to_string()),
    }
}
