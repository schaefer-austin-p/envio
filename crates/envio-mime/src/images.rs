//! Second pass over ammonia's output: images (remote, `cid:`, trackers)
//! and over-wide tables. Ammonia's serializer always double-quotes
//! attribute values and escapes `<` in text, so every `<` starts a tag and
//! a quote-aware scan finds where it ends.

use crate::plain::escape_into;
use crate::sanitize::{RenderOptions, Rendered};

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
struct Tag {
    name: String,
    attrs: Vec<(String, String)>,
}

impl Tag {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    fn write(&self, out: &mut String) {
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

/// Applies the image and width rules to sanitized HTML.
pub(crate) fn fit(html: &str, options: &RenderOptions) -> Rendered {
    let mut out = Rendered {
        html: String::with_capacity(html.len()),
        ..Rendered::default()
    };
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        out.html.push_str(&rest[..start]);
        let len = tag_len(&rest[start..]);
        let raw = &rest[start..start + len];
        match parse_tag(raw) {
            Some(tag) if tag.name == "img" => image(&tag, options, &mut out),
            Some(mut tag) if matches!(tag.name.as_str(), "table" | "td" | "th") => {
                limit_width(&mut tag, options.max_image_width);
                tag.write(&mut out.html);
            }
            _ => out.html.push_str(raw),
        }
        rest = &rest[start + len..];
    }
    out.html.push_str(rest);
    out
}

/// Bytes up to and including the `>` that closes the tag at the start of
/// `s`, skipping `>` inside double-quoted values.
fn tag_len(s: &str) -> usize {
    let mut quoted = false;
    for (i, c) in s.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '>' if !quoted => return i + 1,
            _ => {}
        }
    }
    s.len()
}

/// Parses `<name a="v" ...>`; `None` for end tags and anything else.
fn parse_tag(raw: &str) -> Option<Tag> {
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

fn image(tag: &Tag, options: &RenderOptions, out: &mut Rendered) {
    let src = tag.attr("src").unwrap_or_default().trim();
    let width = size(tag, "width");
    let height = size(tag, "height");
    if is_tracker(src, width, height) {
        out.removed_trackers += 1;
        return;
    }
    let lower = src.to_ascii_lowercase();
    let url = if lower.starts_with("cid:") {
        match cid_url(&src[4..], options) {
            Some(url) => url.to_owned(),
            None => return,
        }
    } else if lower.starts_with("http:") || lower.starts_with("https:") {
        if !options.allow_remote_images {
            out.blocked_images += 1;
            let alt = tag.attr("alt").map(str::trim).unwrap_or_default();
            let label = if alt.is_empty() {
                &options.blocked_label
            } else {
                alt
            };
            escape_into(label, &mut out.html);
            return;
        }
        src.to_owned()
    } else {
        return;
    };
    let (width, height) = scale(width, height, options.max_image_width);
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
}

/// The loadable URL for a `cid:` reference, with or without angle brackets.
fn cid_url<'o>(cid: &str, options: &'o RenderOptions) -> Option<&'o str> {
    let id = crate::thread::normalize_id(cid)?;
    options.cid_urls.get(&id).map(String::as_str)
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

fn style_value<'s>(style: &'s str, name: &str) -> Option<&'s str> {
    style.split(';').find_map(|declaration| {
        let (prop, value) = declaration.split_once(':')?;
        prop.trim().eq_ignore_ascii_case(name).then_some(value)
    })
}

/// A length as whole pixels: `20`, `20px`, `1.0PX`, `0.5`, ` 20 px`,
/// `15pt` (pt x 4/3) and a trailing `!important`. Percentages, negatives
/// and the like are `None`; a number too large for `u32` is `u32::MAX`.
fn pixels(value: &str) -> Option<u32> {
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

/// Drops a pixel `width` wider than `max` from a table or cell.
fn limit_width(tag: &mut Tag, max: u32) {
    if max == 0 {
        return;
    }
    tag.attrs
        .retain(|(name, value)| name != "width" || pixels(value).is_none_or(|w| w <= max));
}
