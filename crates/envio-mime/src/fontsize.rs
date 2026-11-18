//! Font sizes after ammonia: a heading wider than the phone reader wraps
//! mid-word (#177), and a size under 1px is a blank line (#176). Every size
//! the mail sets is resolved against the size it inherits and clamped to
//! 1 to 26 px, so nested `%` and `em` cannot compound past the limit (#191).
//!
//! Works on the balanced, double-quoted markup ammonia writes, in one
//! forward pass with a stack of open elements. A size that cannot be
//! parsed (`1e3px`, `40px 40px`, an unknown unit) is dropped, never kept.

use crate::images::{Tag, parse_tag, tag_len};

/// The largest font size kept, in px.
const MAX_PX: f64 = 26.0;
/// The smallest: Qt 5 ignores 0 and may truncate less than 1px to it.
const MIN_PX: f64 = 1.0;
/// The size at the root of the document.
const DEFAULT_PX: f64 = 16.0;

/// Elements without an end tag.
const VOID: &[&str] = &["br", "col", "hr", "img"];

/// Sizes of `<font size=1..=7>` in px.
const FONT_TAG_PX: [f64; 7] = [10.0, 13.0, 16.0, 18.0, 24.0, 32.0, 48.0];
/// The largest `<font size>` kept (24px).
const MAX_FONT_TAG: i32 = 5;

/// `html` with every font size it sets resolved and clamped.
pub(crate) fn fit(html: &str) -> String {
    if !html.contains("font") {
        return html.to_owned();
    }
    let mut out = String::with_capacity(html.len());
    // Each open element's name and its computed font size.
    let mut open: Vec<(String, f64)> = Vec::new();
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
        let inherited = open.last().map_or(DEFAULT_PX, |(_, px)| *px);
        let rewritten = tag.name == "font" || tag.attr("style").is_some();
        let px = resize(&mut tag, inherited);
        if rewritten {
            tag.write(&mut out);
        } else {
            out.push_str(raw);
        }
        if !VOID.contains(&tag.name.as_str()) {
            open.push((tag.name, px));
        }
    }
    out.push_str(rest);
    out
}

/// Rewrites the sizes `tag` sets and returns the size its content has.
fn resize(tag: &mut Tag, inherited: f64) -> f64 {
    let mut px = inherited;
    if tag.name == "font"
        && let Some(at) = tag.attrs.iter().position(|(n, _)| n == "size")
    {
        match font_tag_level(&tag.attrs[at].1) {
            Some(level) => {
                px = FONT_TAG_PX[level as usize - 1];
                tag.attrs[at].1 = level.to_string();
            }
            None => {
                tag.attrs.remove(at);
            }
        }
    }
    if let Some(at) = tag.attrs.iter().position(|(n, _)| n == "style") {
        let (style, size) = resize_style(&tag.attrs[at].1, inherited);
        px = size.unwrap_or(px);
        if style.is_empty() {
            tag.attrs.remove(at);
        } else {
            tag.attrs[at].1 = style;
        }
    }
    px
}

/// `style` with each `font-size` clamped (or dropped when unparseable), and
/// the size the last valid one gives; sizes are relative to `inherited`.
fn resize_style(style: &str, inherited: f64) -> (String, Option<f64>) {
    let mut size = None;
    let kept: Vec<String> = style
        .split(';')
        .filter_map(|declaration| match declaration.split_once(':') {
            Some((prop, value)) if prop.trim().eq_ignore_ascii_case("font-size") => {
                let px = computed_px(value, inherited)?;
                let fitted = px.clamp(MIN_PX, MAX_PX);
                size = Some(fitted);
                Some(if fitted == px {
                    declaration.to_owned()
                } else {
                    format!("font-size:{fitted}px")
                })
            }
            _ => Some(declaration.to_owned()),
        })
        .collect();
    (kept.join(";"), size)
}

/// The level (1 to [`MAX_FONT_TAG`]) a `<font size>` attribute gives: a
/// bare number is absolute, `+n` and `-n` are relative to 3. `None` when
/// it is anything else.
fn font_tag_level(value: &str) -> Option<i32> {
    let value = value.trim();
    let (sign, digits) = match value.as_bytes().first()? {
        b'+' => (1, &value[1..]),
        b'-' => (-1, &value[1..]),
        _ => (0, value),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: i32 = digits.parse().ok()?;
    let level = if sign == 0 { n } else { 3 + sign * n };
    Some(level.clamp(1, MAX_FONT_TAG))
}

/// The size in px a `font-size` value gives under `inherited`, before
/// clamping. `None` for anything that is not one length, percentage or
/// keyword, optionally followed by `!important`.
fn computed_px(value: &str, inherited: f64) -> Option<f64> {
    let (size, flag) = match value.split_once('!') {
        Some((size, flag)) => (size, flag.trim()),
        None => (value, "important"),
    };
    if !flag.eq_ignore_ascii_case("important") {
        return None;
    }
    let size = size.trim().to_ascii_lowercase();
    let size = size.strip_prefix('+').unwrap_or(&size);
    let px = keyword_px(size, inherited).or_else(|| length_px(size, inherited))?;
    px.is_finite().then_some(px)
}

fn keyword_px(word: &str, inherited: f64) -> Option<f64> {
    Some(match word {
        "xx-small" => 9.0,
        "x-small" => 10.0,
        "small" => 13.0,
        "medium" => 16.0,
        "large" => 18.0,
        "x-large" => 24.0,
        "xx-large" => 32.0,
        "xxx-large" => 48.0,
        "larger" => inherited * 1.2,
        "smaller" => inherited / 1.2,
        _ => return None,
    })
}

fn length_px(size: &str, inherited: f64) -> Option<f64> {
    let end = size
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(size.len());
    if !size[..end].contains(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let n: f64 = size[..end].parse().ok()?;
    match &size[end..] {
        "px" => Some(n),
        "pt" => Some(n * 4.0 / 3.0),
        "rem" => Some(n * DEFAULT_PX),
        "em" => Some(n * inherited),
        "%" => Some(n * inherited / 100.0),
        "" if n == 0.0 => Some(0.0),
        _ => None,
    }
}

/// Closes the innermost open element called `name`, with any left open
/// inside it; an end tag with no open element is ignored.
fn close(open: &mut Vec<(String, f64)>, name: &str) {
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
