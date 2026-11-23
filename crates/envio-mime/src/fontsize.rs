//! Font sizes after ammonia: a heading wider than the phone reader wraps
//! mid-word (#177), and a size under 1px is a blank line (#176). Every size
//! the mail sets is resolved against the size it inherits and clamped to
//! 1 to 26 px, so nested `%` and `em` cannot compound past the limit (#191).
//!
//! Qt 5 sizes some tags itself (#243): `h1` to `h5`, `big`, `small` and
//! `<font size>` get a fixed multiple of the document's default font, not of
//! the parent (QTextFormatPrivate::resolveFont, qtextformat.cpp), and the
//! `font-size` keywords `small` to `xx-large` do the same. That size is
//! inherited by every element inside and beats any `px` there. The pass
//! models it, and gives an `h1` (twice the default, over the cap) the
//! keyword `x-large` instead. `sub` and `sup` are painted at two thirds of
//! their size, rounded down (QTextEngine::font, qtextengine.cpp), so they
//! and everything in them keep at least 2px.
//!
//! Runs last, on the markup Qt finally reads, so its stack of open
//! elements matches Qt's (elements the earlier passes drop do not count).
//! Works on the balanced, double-quoted markup the earlier passes leave, in one
//! forward pass with a stack of open elements. A size that cannot be
//! parsed (`1e3px`, `40px 40px`, an unknown unit) is dropped, never kept.

use crate::hidden::VOID;
use crate::images::{Tag, parse_tag, tag_len};

/// The largest font size kept, in px.
const MAX_PX: f64 = 26.0;
/// The smallest: Qt 5 ignores 0 and may truncate less than 1px to it.
const MIN_PX: f64 = 1.0;
/// The size at the root of the document.
const DEFAULT_PX: f64 = 16.0;

/// Qt's multiples of the default font for a size adjustment of -2 to 4
/// (index = adjustment + 2; qtextformat.cpp `scaleFactors`).
const ADJUST_SCALE: [f64; 7] = [0.7, 0.8, 1.0, 1.2, 1.5, 2.0, 2.4];
/// The adjustment (and CSS keyword) given to an `h1`: 24px, in range.
const NEUTRAL_ADJUSTMENT: i32 = 2;
const NEUTRAL_KEYWORD: &str = "x-large";
/// The smallest size under `sub` and `sup`: painted at 2/3, rounded down.
const MIN_SCRIPT_PX: f64 = 2.0;
/// The largest `<font size>` kept (24px).
const MAX_FONT_TAG: i32 = 5;

/// An open element: its name, its computed font size, and whether it is
/// `sub`, `sup` or inside one.
struct Open {
    name: String,
    px: f64,
    scripted: bool,
}

/// The size in px Qt gives an element with this size adjustment.
fn adjusted_px(adjustment: i32) -> f64 {
    ADJUST_SCALE[(adjustment + 2).clamp(0, 6) as usize] * DEFAULT_PX
}

/// The size adjustment Qt gives `name` by itself.
fn tag_adjustment(name: &str) -> Option<i32> {
    Some(match name {
        "h1" => 3,
        "h2" => 2,
        "h3" | "big" => 1,
        "h4" => 0,
        "h5" | "small" => -1,
        _ => return None,
    })
}

/// `html` with every font size it sets resolved and clamped.
pub(crate) fn fit(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut open: Vec<Open> = Vec::new();
    let mut rest = html;
    while let Some(start) = crate::work::find_lt(rest) {
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
        // Worst case, as in the tests' model: a parent is painted at whole
        // px and a relative child applies to that, so 1.5px counts as 2px
        // (#259). Qt 5.15 itself ignores `%` and `em`.
        let inherited = open.last().map_or(DEFAULT_PX, |o| o.px.round());
        let scripted =
            matches!(tag.name.as_str(), "sub" | "sup") || open.last().is_some_and(|o| o.scripted);
        let before = tag.attrs.clone();
        let rewritten = tag.name == "font" || tag.attr("style").is_some();
        let px = resize(&mut tag, inherited, scripted);
        if rewritten || tag.attrs != before {
            tag.write(&mut out);
        } else {
            out.push_str(raw);
        }
        if !VOID.contains(&tag.name.as_str()) {
            open.push(Open {
                name: tag.name,
                px,
                scripted,
            });
        }
    }
    out.push_str(rest);
    out
}

/// Rewrites the sizes `tag` sets and returns the size its content has.
/// `scripted` is true for `sub`, `sup` and what is inside them.
fn resize(tag: &mut Tag, inherited: f64, scripted: bool) -> f64 {
    let min = if scripted { MIN_SCRIPT_PX } else { MIN_PX };
    let mut px = inherited;
    if tag.name == "font"
        && let Some(at) = tag.attrs.iter().position(|(n, _)| n == "size")
    {
        match font_tag_level(&tag.attrs[at].1) {
            Some(level) => {
                px = adjusted_px(level - 3);
                tag.attrs[at].1 = level.to_string();
            }
            None => {
                tag.attrs.remove(at);
            }
        }
    }
    let mut keyword = false;
    if let Some(at) = tag.attrs.iter().position(|(n, _)| n == "style") {
        let resized = resize_style(&tag.attrs[at].1, inherited, min);
        px = resized.size.unwrap_or(px);
        keyword = resized.keyword;
        if resized.style.is_empty() {
            tag.attrs.remove(at);
        } else {
            tag.attrs[at].1 = resized.style;
        }
    }
    // Qt's own size for the tag wins over any px or % here, unless the
    // style has a keyword (which Qt reads as an adjustment too).
    if let Some(adjustment) = tag_adjustment(&tag.name)
        && !keyword
    {
        px = adjusted_px(adjustment);
        if px > MAX_PX {
            set_style_size(tag, NEUTRAL_KEYWORD);
            px = adjusted_px(NEUTRAL_ADJUSTMENT);
        }
    }
    if px < min {
        set_style_size(tag, &format!("{min}px"));
        px = min;
    }
    px
}

/// Makes `value` the only `font-size` in the style of `tag`.
fn set_style_size(tag: &mut Tag, value: &str) {
    let mut declarations: Vec<String> = tag
        .attr("style")
        .unwrap_or_default()
        .split(';')
        .filter(|d| {
            !d.trim().is_empty()
                && d.split_once(':')
                    .is_none_or(|(prop, _)| !prop.trim().eq_ignore_ascii_case("font-size"))
        })
        .map(str::to_owned)
        .collect();
    declarations.push(format!("font-size:{value}"));
    let style = declarations.join(";");
    match tag.attrs.iter_mut().find(|(n, _)| n == "style") {
        Some(attr) => attr.1 = style,
        None => tag.attrs.push(("style".to_owned(), style)),
    }
}

/// A style after [`resize_style`].
struct Resized {
    style: String,
    /// The size the last valid `font-size` gives.
    size: Option<f64>,
    /// Whether a `font-size` keyword Qt reads as an adjustment is left in.
    keyword: bool,
}

/// `style` with each `font-size` clamped to at least `min` (or dropped when
/// unparseable); sizes are relative to `inherited`.
fn resize_style(style: &str, inherited: f64, min: f64) -> Resized {
    let mut size = None;
    let mut keyword = false;
    let kept: Vec<String> = style
        .split(';')
        .filter_map(|declaration| match declaration.split_once(':') {
            Some((prop, value)) if prop.trim().eq_ignore_ascii_case("font-size") => {
                let px = computed_px(value, inherited)?;
                let fitted = px.clamp(min, MAX_PX);
                size = Some(fitted);
                if fitted == px {
                    keyword |= is_adjusting_keyword(value);
                    Some(declaration.to_owned())
                } else {
                    Some(format!("font-size:{fitted}px"))
                }
            }
            _ => Some(declaration.to_owned()),
        })
        .collect();
    Resized {
        style: kept.join(";"),
        size,
        keyword,
    }
}

/// Whether Qt turns this `font-size` value into a size adjustment.
fn is_adjusting_keyword(value: &str) -> bool {
    let word = value.split('!').next().unwrap_or_default().trim();
    ["small", "medium", "large", "x-large", "xx-large"]
        .iter()
        .any(|k| word.eq_ignore_ascii_case(k))
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
    // More digits than an i32 holds is just a very large number.
    let n: i32 = digits.parse().unwrap_or(i32::MAX);
    let level = if sign == 0 {
        n
    } else {
        3i32.saturating_add(sign * n)
    };
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
fn close(open: &mut Vec<Open>, name: &str) {
    if let Some(at) = open.iter().rposition(|o| o.name.eq_ignore_ascii_case(name)) {
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
