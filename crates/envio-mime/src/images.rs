//! Second pass over ammonia's output: images (remote, `cid:`, trackers)
//! and over-wide tables. Ammonia's serializer always double-quotes
//! attribute values and escapes `<` in text, so every `<` starts a tag and
//! a quote-aware scan finds where it ends.

use std::collections::{HashMap, HashSet};

use crate::hidden::{Ends, is_unwrapped};
use crate::image_label;
use crate::natural::{Unsized, draws_wider};
use crate::plain::escape_into;
use crate::sanitize::{RenderOptions, Rendered};
use crate::style::is_hidden;
use crate::widths;

/// Hosts that serve tracking pixels; subdomains match too.
const TRACKER_HOSTS: &[&str] = &[
    "google-analytics.com",
    "doubleclick.net",
    "list-manage.com",
    "mandrillapp.com",
    "sendgrid.net",
    "mailtrack.io",
    "mailfoogae.appspot.com",
    "pixel.wp.com",
    "bat.bing.com",
    "t.yesware.com",
    "r.superhuman.com",
    "bananatag.com",
    "getnotify.com",
    "mailstat.us",
];

/// An image this small (width or height, in pixels) is a tracking pixel.
const PIXEL_MAX: u32 = 2;

/// A start tag: lower-case name and unescaped attribute values.
pub(crate) struct Tag {
    pub(crate) name: String,
    pub(crate) attrs: Vec<(String, String)>,
}

impl Tag {
    pub(crate) fn attr(&self, name: &str) -> Option<&str> {
        crate::work::tick(self.attrs.len());
        self.attrs
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    pub(crate) fn write(&self, out: &mut String) {
        crate::work::tick(1 + self.attrs.len());
        out.push('<');
        out.push_str(&self.name);
        for (name, value) in &self.attrs {
            out.push(' ');
            out.push_str(name);
            out.push_str("=\"");
            escape_into(value, out);
            out.push('"');
        }
        out.push('>');
    }
}

/// Most distinct remote images one message loads, and most images waiting
/// for a reported size (#215). Past it an image shows its label: the
/// reader makes an item and a network load per URL, so an unbounded list
/// would stall or exhaust the phone.
pub(crate) const MAX_REMOTE_IMAGES: usize = 200;

/// Most (room, height) variants of one URL kept for a reported size. Per
/// URL, so repeats of one URL cannot use up the slots of the others; the
/// total stays under `MAX_REMOTE_IMAGES * MAX_VARIANTS`.
const MAX_VARIANTS: usize = 8;

/// The remote images [`fit`] has listed, for a constant-time de-dup.
struct Seen<'a> {
    options: &'a RenderOptions,
    urls: HashSet<String>,
    unsized_images: HashMap<String, HashSet<(u32, Option<u32>)>>,
}

impl Seen<'_> {
    /// Whether `url` at this room and height is a new entry within its
    /// URL's [`MAX_VARIANTS`]. Only listed URLs get here, so at most
    /// `MAX_REMOTE_IMAGES` keys exist.
    fn wants_size(&mut self, url: &str, room: u32, height: Option<u32>) -> bool {
        let variants = self.unsized_images.entry(url.to_owned()).or_default();
        variants.len() < MAX_VARIANTS && variants.insert((room, height))
    }
}

/// An image in [`fit`]'s result, shown or as its label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Placed {
    /// Byte range of the `img` tag or the label.
    pub(crate) at: usize,
    pub(crate) end: usize,
    /// The width the message gave it, in pixels, before scaling.
    pub(crate) width: Option<u32>,
}

/// Applies the image and width rules to sanitized HTML. Pushes each image
/// it writes, shown or as its label, to `images` in order.
pub(crate) fn fit(html: &str, options: &RenderOptions, images: &mut Vec<Placed>) -> Rendered {
    let mut out = Rendered {
        html: String::with_capacity(html.len()),
        ..Rendered::default()
    };
    let mut rest = html;
    let mut ends: Option<Ends> = None;
    let mut seen = Seen {
        options,
        urls: HashSet::new(),
        unsized_images: HashMap::new(),
    };
    // The pixel width of each open table cell, so a percentage image
    // width is a share of its cell rather than of the whole reader.
    let mut cells: Vec<Option<u32>> = Vec::new();
    while let Some(start) = crate::work::find_lt(rest) {
        out.html.push_str(&rest[..start]);
        let len = tag_len(&rest[start..]);
        let raw = &rest[start..start + len];
        let after = start + len;
        match parse_tag(raw) {
            Some(tag) if tag.attr("style").is_some_and(is_hidden) => {
                let at = html.len() - rest.len() + start;
                let ends = ends.get_or_insert_with(|| Ends::new(html));
                rest = match ends.end_of(at) {
                    Some(end) => &html[end..],
                    None => &rest[after..],
                };
                continue;
            }
            _ if is_unwrapped(raw) => {}
            Some(tag) if tag.name == "img" => {
                let at = out.html.len();
                crate::work::tick(cells.len());
                let cell = cells.iter().rev().find_map(|w| *w);
                if let Some(width) = image(&tag, cell, &mut seen, &mut out) {
                    let end = out.html.len();
                    images.push(Placed { at, end, width });
                }
            }
            Some(mut tag) if matches!(tag.name.as_str(), "table" | "td" | "th" | "col") => {
                widths::carry(&mut tag, options.max_image_width);
                limit_width(&mut tag, options.max_image_width);
                if matches!(tag.name.as_str(), "td" | "th") {
                    cells.push(tag.attr("width").and_then(pixels));
                }
                tag.write(&mut out.html);
            }
            _ => {
                if closes_cell(raw) {
                    cells.pop();
                }
                out.html.push_str(raw);
            }
        }
        rest = &rest[start + len..];
    }
    out.html.push_str(rest);
    out
}

/// Bytes up to and including the `>` that closes the tag at the start of
/// `s`, skipping `>` inside double-quoted values.
pub(crate) fn tag_len(s: &str) -> usize {
    let mut quoted = false;
    for (i, c) in s.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '>' if !quoted => {
                crate::work::tick(i + 1);
                return i + 1;
            }
            _ => {}
        }
    }
    crate::work::tick(s.len());
    s.len()
}

/// Parses `<name a="v" ...>`; `None` for end tags and anything else.
pub(crate) fn parse_tag(raw: &str) -> Option<Tag> {
    crate::work::tick(raw.len());
    let inner = raw.strip_prefix('<')?.strip_suffix('>')?;
    let inner = inner.strip_suffix('/').unwrap_or(inner);
    if !inner.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    let name_end = inner.find(char::is_whitespace).unwrap_or(inner.len());
    let mut tag = Tag {
        name: inner[..name_end].to_ascii_lowercase(),
        attrs: Vec::new(),
    };
    let mut rest = inner[name_end..].trim_start();
    while !rest.is_empty() {
        let name_len = rest
            .find(|c: char| c == '=' || c.is_whitespace())
            .unwrap_or(rest.len());
        let name = rest[..name_len].to_ascii_lowercase();
        rest = &rest[name_len..];
        let value = match rest.strip_prefix("=\"") {
            Some(after) => {
                let end = after.find('"')?;
                rest = &after[end + 1..];
                unescape(&after[..end])
            }
            None => String::new(),
        };
        tag.attrs.push((name, value));
        rest = rest.trim_start();
    }
    Some(tag)
}

/// Undoes html5ever's attribute escaping (`&amp;` last, so `&amp;lt;`
/// stays `&lt;`).
fn unescape(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", "\u{a0}")
        .replace("&amp;", "&")
}

/// Writes the image, or its label when it isn't loaded. `Some` with the
/// declared width when it stands for a picture; `None` for a tracker or an
/// unusable `src`.
fn image(
    tag: &Tag,
    cell: Option<u32>,
    seen: &mut Seen<'_>,
    out: &mut Rendered,
) -> Option<Option<u32>> {
    let options = seen.options;
    let src = tag.attr("src").unwrap_or_default().trim();
    let width = size(tag, "width");
    let height = size(tag, "height");
    if is_tracker(src, width, height) {
        out.removed_trackers += 1;
        return None;
    }
    let lower = src.to_ascii_lowercase();
    let is_cid = lower.starts_with("cid:");
    let natural;
    let url = if is_cid {
        match cid_url(&src[4..], options) {
            Some((url, size)) => {
                natural = size;
                url.to_owned()
            }
            None => {
                // Not in the message: shown like a blocked image, but not
                // counted, since loading remote images won't bring it back.
                image_label::write(tag, options, &mut out.html);
                return Some(width);
            }
        }
    } else if lower.starts_with("http:") || lower.starts_with("https:") {
        // The same rule blocked or not, so the blocked count is what
        // "Load images" brings back: each distinct URL once, up to the cap.
        if !seen.urls.contains(src) {
            if seen.urls.len() >= MAX_REMOTE_IMAGES {
                // Over the cap: the label, not counted as blocked (the
                // "Load images" choice would not bring it back).
                image_label::write(tag, options, &mut out.html);
                return Some(width);
            }
            seen.urls.insert(src.to_owned());
            if options.allow_remote_images {
                out.remote_images.push(src.to_owned());
            } else {
                out.blocked_images += 1;
            }
        }
        if !options.allow_remote_images {
            image_label::write(tag, options, &mut out.html);
            return Some(width);
        }
        natural = options.image_sizes.get(src).copied();
        src.to_owned()
    } else {
        return None;
    };
    let declared = width;
    let room = match (options.max_image_width, cell) {
        (0, _) => 0,
        (max, Some(cell)) => cell.min(max),
        (max, None) => max,
    };
    let (mut width, mut height) = match (width, share_of(tag, room)) {
        // Qt's rich text has no percentage or max-width for an image and
        // would draw it at the file's size; the height in another unit
        // would distort it, so only the width is set.
        (None, Some(share)) => (Some(share), None),
        _ => scale(width, height, options.max_image_width),
    };
    // Likewise an image with no width whose natural size is known (a cid
    // image's from its bytes, a remote one's as the reader reports it):
    // draw it at the room if it would come out wider. The height goes, to
    // keep the proportion.
    if width.is_none()
        && let Some(size) = natural
        && draws_wider(size, height, room)
    {
        width = Some(room);
        height = None;
    }
    // A remote image still without a width: a size the reader reports
    // later may cap it.
    if width.is_none() && room > 0 && !is_cid && seen.wants_size(&url, room, height) {
        out.unsized_images.push(Unsized {
            url: url.clone(),
            room,
            height,
        });
    }
    let mut attrs = vec![("src".to_owned(), url)];
    if let Some(alt) = tag.attr("alt") {
        attrs.push(("alt".to_owned(), alt.to_owned()));
    }
    if let Some(w) = width {
        attrs.push(("width".to_owned(), w.to_string()));
    }
    if let Some(h) = height {
        attrs.push(("height".to_owned(), h.to_string()));
    }
    Tag {
        name: "img".to_owned(),
        attrs,
    }
    .write(&mut out.html);
    Some(declared)
}

/// Whether `raw` is a `</td>` or `</th>` end tag (not `</thead>`).
fn closes_cell(raw: &str) -> bool {
    ["</td", "</th"].iter().any(|end| {
        raw.strip_prefix(end)
            .and_then(|rest| rest.chars().next())
            .is_some_and(|c| c == '>' || c.is_ascii_whitespace())
    })
}

/// A percentage width (`width="100%"`, `width:50%`) as pixels of `max`
/// (the reader, or the image's cell when narrower),
/// capped by a pixel `max-width`; `None` without one or when `max` is 0.
fn share_of(tag: &Tag, max: u32) -> Option<u32> {
    let style = tag.attr("style");
    let percent = tag.attr("width").and_then(percent).or_else(|| {
        style
            .and_then(|s| style_value(s, "width"))
            .and_then(percent)
    })?;
    if max == 0 {
        return None;
    }
    let cap = style
        .and_then(|s| style_value(s, "max-width"))
        .and_then(pixels)
        .filter(|&w| w > PIXEL_MAX)
        .map_or(max, |w| w.min(max));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let px = (f64::from(max) * percent / 100.0).round() as u32;
    Some(px.clamp(1, cap))
}

/// A percentage in (0, 100], with an optional trailing `!important`.
fn percent(value: &str) -> Option<f64> {
    let value = value.trim().to_ascii_lowercase();
    let value = value.split('!').next().unwrap_or_default().trim();
    let number = value.strip_suffix('%')?.trim();
    if number.is_empty() || !number.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    let n = number.parse::<f64>().ok()?;
    (n > 0.0).then_some(n.min(100.0))
}

/// The loadable URL for a `cid:` reference and the image's natural size,
/// when known.
fn cid_url<'o>(cid: &str, options: &'o RenderOptions) -> Option<(&'o str, Option<(u32, u32)>)> {
    let id = crate::thread::normalize_id(cid)?;
    let url = options.cid_urls.get(&id)?;
    Some((url, options.cid_sizes.get(&id).copied()))
}

fn is_tracker(src: &str, width: Option<u32>, height: Option<u32>) -> bool {
    if width.is_some_and(|w| w <= PIXEL_MAX) || height.is_some_and(|h| h <= PIXEL_MAX) {
        return true;
    }
    let Some(host) = ammonia::Url::parse(src).ok().and_then(|url| {
        url.host_str()
            .map(|h| h.trim_end_matches('.').to_ascii_lowercase())
    }) else {
        return false;
    };
    TRACKER_HOSTS.iter().any(|t| {
        host == *t
            || host
                .strip_suffix(t)
                .is_some_and(|prefix| prefix.ends_with('.'))
    })
}

/// A pixel size from the attribute, else from `style` (`width:20px`).
fn size(tag: &Tag, name: &str) -> Option<u32> {
    tag.attr(name).and_then(pixels).or_else(|| {
        tag.attr("style")
            .and_then(|s| style_value(s, name))
            .and_then(pixels)
    })
}

pub(crate) fn style_value<'s>(style: &'s str, name: &str) -> Option<&'s str> {
    crate::work::tick(style.len());
    style.split(';').find_map(|declaration| {
        let (prop, value) = declaration.split_once(':')?;
        prop.trim().eq_ignore_ascii_case(name).then_some(value)
    })
}

/// A length as whole pixels: `20`, `20px`, `1.0PX`, `0.5`, ` 20 px`,
/// `15pt` (pt x 4/3) and a trailing `!important`. Percentages, negatives
/// and the like are `None`; a number too large for `u32` is `u32::MAX`.
pub(crate) fn pixels(value: &str) -> Option<u32> {
    let value = value.trim().to_ascii_lowercase();
    let value = value.strip_suffix("!important").unwrap_or(&value).trim();
    let (number, factor) = match value.strip_suffix("pt") {
        Some(number) => (number, 4.0 / 3.0),
        None => (value.strip_suffix("px").unwrap_or(value), 1.0),
    };
    let number = number.trim();
    if number.is_empty() || !number.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    let px = (number.parse::<f64>().ok()? * factor).round();
    // Saturating on purpose: huge values count as "too wide".
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some(px.min(f64::from(u32::MAX)) as u32)
}

/// Scales a too-wide image down to `max`, keeping its aspect ratio when
/// the height is known.
fn scale(width: Option<u32>, height: Option<u32>, max: u32) -> (Option<u32>, Option<u32>) {
    match width {
        Some(w) if max > 0 && w > max => {
            let h = height.map(|h| {
                let scaled = u64::from(h) * u64::from(max) / u64::from(w);
                u32::try_from(scaled).unwrap_or(u32::MAX).max(1)
            });
            (Some(max), h)
        }
        _ => (width, height),
    }
}

/// Drops a `width` the message set on a table, cell or column unless it
/// is a sane percentage or pixels within `max`.
fn limit_width(tag: &mut Tag, max: u32) {
    tag.attrs = std::mem::take(&mut tag.attrs)
        .into_iter()
        .filter_map(|(name, value)| match name.as_str() {
            "width" => widths::sane_attribute(&tag.name, &value, max).map(|w| (name, w)),
            _ => Some((name, value)),
        })
        .collect();
}
