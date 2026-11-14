//! What stands in for an image the reader does not show (blocked, or a
//! `cid:` part the message lacks). A small image is an icon, a social
//! button or a spacer: its alt text ("info icon") only clutters the
//! layout, so it leaves nothing (#161). A larger one, or one with no
//! declared size, shows its alt text or the caller's label, not
//! underlined: inside a link Qt would underline it, and a row of linked
//! labels reads as a stack of links.

use crate::images::Tag;
use crate::plain::escape_into;
use crate::sanitize::RenderOptions;

/// An image that declares a size and no side larger than this, in pixels,
/// shows no label. A banner or a logo has one small side only.
const SMALL_MAX: u32 = 48;

/// Writes the label for an image that is not shown, if it gets one.
/// `(width, height)` is the size it declares, in pixels.
pub(crate) fn write(
    tag: &Tag,
    (width, height): (Option<u32>, Option<u32>),
    options: &RenderOptions,
    out: &mut String,
) {
    let small = |side: Option<u32>| side.is_none_or(|px| px <= SMALL_MAX);
    if (width.is_some() || height.is_some()) && small(width) && small(height) {
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
