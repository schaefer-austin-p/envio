//! Does sanitised mail set its own colours? Such mail is drawn on a light
//! "paper" card, as other mail apps do, because its dark text assumes a
//! white page. Reads tags only, never text.

mod names;

use crate::images::{parse_tag, tag_len};

/// Below this WCAG relative luminance, black text has the better contrast.
const DARK: f64 = 0.18;

/// True when any tag sets a visible background (`style` `background-color`,
/// a `bgcolor` attribute) or a dark text colour (`style` `color`, `font
/// color`). A colour that cannot be read is no evidence, so it does not
/// count; nor do `transparent`, `inherit`, `currentcolor`, an empty value or
/// a fully transparent colour as a background; nor does light text without
/// a background.
pub fn sets_colours(html: &str) -> bool {
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        let len = tag_len(&rest[start..]);
        let raw = &rest[start..start + len];
        rest = &rest[start + len..];
        let Some(tag) = parse_tag(raw) else { continue };
        if tag.attr("bgcolor").is_some_and(is_visible_attr) {
            return true;
        }
        if tag.name == "font" && tag.attr("color").is_some_and(is_dark) {
            return true;
        }
        if let Some(style) = tag.attr("style") {
            for (name, value) in declarations(style) {
                match name.as_str() {
                    // `linearise` moves a table's `bgcolor`, bare hex
                    // included, into `background-color`.
                    "background-color" if is_visible_attr(value) => return true,
                    "color" if is_dark(value) => return true,
                    _ => {}
                }
            }
        }
    }
    false
}

fn declarations(style: &str) -> impl Iterator<Item = (String, &str)> {
    style.split(';').filter_map(|d| {
        let (name, value) = d.split_once(':')?;
        Some((name.trim().to_ascii_lowercase(), value))
    })
}

fn is_dark(value: &str) -> bool {
    parse_colour(value).is_some_and(|(r, g, b)| luminance(r, g, b) < DARK)
}

/// A background value that parses to a colour that is not fully transparent.
fn is_visible(value: &str) -> bool {
    parse_rgba(value).is_some_and(|(_, _, _, a)| a > 0)
}

/// A `bgcolor` value, as an attribute or as the `background-color` that
/// `linearise` turns it into: HTML also accepts bare hex (`ffffff`) there.
fn is_visible_attr(value: &str) -> bool {
    let v = value.trim();
    let bare_hex = matches!(v.len(), 3 | 6) && v.bytes().all(|b| b.is_ascii_hexdigit());
    if bare_hex {
        is_visible(&format!("#{v}"))
    } else {
        is_visible(v)
    }
}

fn luminance(r: u8, g: u8, b: u8) -> f64 {
    let linear = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

/// Reads a CSS colour: `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`,
/// `rgb()`/`rgba()`, `hsl()`/`hsla()` and the CSS names. Alpha is ignored.
/// A trailing `!important` is ignored.
pub fn parse_colour(value: &str) -> Option<(u8, u8, u8)> {
    parse_rgba(value).map(|(r, g, b, _)| (r, g, b))
}

/// Like [`parse_colour`], with alpha (255 when none is given).
fn parse_rgba(value: &str) -> Option<(u8, u8, u8, u8)> {
    let value = value.trim();
    let value = match value.rfind('!') {
        Some(at) if value[at + 1..].trim().eq_ignore_ascii_case("important") => &value[..at],
        _ => value,
    };
    let value = value.trim().to_ascii_lowercase();
    if let Some(hex) = value.strip_prefix('#') {
        return hex_colour(hex);
    }
    if let Some((func, args)) = value.split_once('(') {
        let args = args.strip_suffix(')')?;
        let parts: Vec<&str> = args
            .split(|c: char| c == ',' || c == '/' || c.is_whitespace())
            .filter(|p| !p.is_empty())
            .collect();
        if parts.len() < 3 {
            return None;
        }
        let alpha = match parts.get(3) {
            Some(a) => alpha(a)?,
            None => 255,
        };
        let (r, g, b) = match func.trim() {
            "rgb" | "rgba" => (channel(parts[0])?, channel(parts[1])?, channel(parts[2])?),
            "hsl" | "hsla" => hsl(parts[0], parts[1], parts[2])?,
            _ => return None,
        };
        return Some((r, g, b, alpha));
    }
    let (r, g, b) = names::lookup(&value)?;
    Some((r, g, b, 255))
}

fn hex_colour(hex: &str) -> Option<(u8, u8, u8, u8)> {
    if !hex.is_ascii() {
        return None;
    }
    let digit = |s: &str| u8::from_str_radix(s, 16).ok();
    let short = |i: usize| digit(&hex[i..=i]).map(|v| v * 17);
    let long = |i: usize| digit(&hex[i..i + 2]);
    match hex.len() {
        3 => Some((short(0)?, short(1)?, short(2)?, 255)),
        4 => Some((short(0)?, short(1)?, short(2)?, short(3)?)),
        6 => Some((long(0)?, long(2)?, long(4)?, 255)),
        8 => Some((long(0)?, long(2)?, long(4)?, long(6)?)),
        _ => None,
    }
}

fn alpha(part: &str) -> Option<u8> {
    let v = match part.strip_suffix('%') {
        Some(p) => p.parse::<f64>().ok()? / 100.0,
        None => part.parse::<f64>().ok()?,
    };
    Some((v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn channel(part: &str) -> Option<u8> {
    let v = match part.strip_suffix('%') {
        Some(p) => p.parse::<f64>().ok()? / 100.0 * 255.0,
        None => part.parse::<f64>().ok()?,
    };
    Some(v.clamp(0.0, 255.0).round() as u8)
}

fn hsl(h: &str, s: &str, l: &str) -> Option<(u8, u8, u8)> {
    let h = h.strip_suffix("deg").unwrap_or(h).parse::<f64>().ok()?;
    let s = s.strip_suffix('%')?.parse::<f64>().ok()?.clamp(0.0, 100.0) / 100.0;
    let l = l.strip_suffix('%')?.parse::<f64>().ok()?.clamp(0.0, 100.0) / 100.0;
    let h = h.rem_euclid(360.0);
    let a = s * l.min(1.0 - l);
    let f = |n: f64| {
        let k = (n + h / 30.0) % 12.0;
        let v = l - a * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0);
        (v * 255.0).round() as u8
    };
    Some((f(0.0), f(8.0), f(4.0)))
}
