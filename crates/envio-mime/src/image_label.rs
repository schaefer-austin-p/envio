//! What stands in for an image the reader does not show (blocked, or a
//! `cid:` part the message lacks). A small image is an icon, a social
//! button or a spacer: its alt text ("info icon") only clutters the
//! layout, so it leaves nothing (#161). A larger one, or one with no
//! declared size, shows its alt text or the caller's label, not
//! underlined: inside a link Qt would underline it, and a row of linked
//! labels reads as a stack of links.

use crate::images::{Tag, pixels, style_value};
use crate::plain::escape_into;
use crate::sanitize::RenderOptions;

/// An image that declares a side in pixels no larger than this, and no
/// other side larger or in another unit, shows no label. A banner or a
/// logo has one small side only.
const SMALL_MAX: u32 = 48;

/// A side as the label sees it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Undeclared,
    Small,
    /// Over [`SMALL_MAX`] pixels, or not a pixel length (`100%`, `auto`,
    /// junk): it may be as wide as the reader.
    Large,
}

/// The tag's `name` side: its attribute, else its `style` declaration.
fn side(tag: &Tag, name: &str) -> Side {
    let declared = tag
        .attr(name)
        .or_else(|| tag.attr("style").and_then(|s| style_value(s, name)));
    match declared.map(pixels) {
        None => Side::Undeclared,
        Some(Some(px)) if px <= SMALL_MAX => Side::Small,
        Some(_) => Side::Large,
    }
}

/// Writes the label for an image that is not shown, if it gets one.
pub(crate) fn write(tag: &Tag, options: &RenderOptions, out: &mut String) {
    let sides = [side(tag, "width"), side(tag, "height")];
    if sides.contains(&Side::Small) && !sides.contains(&Side::Large) {
        return;
    }
    let alt = tag.attr("alt").map(str::trim).unwrap_or_default();
    let label = if alt.is_empty() {
        options.blocked_label.as_str()
    } else {
        alt
    };
    if label.is_empty() {
        return;
    }
    out.push_str(r#"<span style="text-decoration:none">"#);
    escape_into(label, out);
    out.push_str("</span>");
}
