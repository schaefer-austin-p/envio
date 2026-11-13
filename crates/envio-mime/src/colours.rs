//! Does sanitised mail set its own colours? Such mail is drawn on a light
//! "paper" card, as other mail apps do, because its dark text assumes a
//! white page. Reads tags only, never text.

use crate::images::{parse_tag, tag_len};

/// Below this WCAG relative luminance, black text has the better contrast.
const DARK: f64 = 0.18;

/// True when any tag sets a background (`style` `background-color`, a
/// `bgcolor` attribute) or a dark text colour (`style` `color`, `font
/// color`). A colour that cannot be read is no evidence, so it does not
/// count; nor does light text without a background.
pub fn sets_colours(html: &str) -> bool {
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        let len = tag_len(&rest[start..]);
        let raw = &rest[start..start + len];
        rest = &rest[start + len..];
        let Some(tag) = parse_tag(raw) else { continue };
        if tag.attr("bgcolor").is_some() {
            return true;
        }
        if tag.name == "font" && tag.attr("color").is_some_and(is_dark) {
            return true;
        }
        if let Some(style) = tag.attr("style") {
            for (name, value) in declarations(style) {
                match name.as_str() {
                    "background-color" => return true,
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

/// Reads a CSS colour: `#rgb`, `#rrggbb`, `rgb()`/`rgba()`, `hsl()`/`hsla()`
/// and common names. A trailing `!important` is ignored.
pub fn parse_colour(value: &str) -> Option<(u8, u8, u8)> {
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
        return match func.trim() {
            "rgb" | "rgba" => Some((channel(parts[0])?, channel(parts[1])?, channel(parts[2])?)),
            "hsl" | "hsla" => hsl(parts[0], parts[1], parts[2]),
            _ => None,
        };
    }
    NAMES
        .iter()
        .find(|(name, _)| *name == value)
        .map(|&(_, rgb)| ((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8))
}

fn hex_colour(hex: &str) -> Option<(u8, u8, u8)> {
    if !hex.is_ascii() {
        return None;
    }
    let digit = |s: &str| u8::from_str_radix(s, 16).ok();
    match hex.len() {
        3 => {
            let d = |i: usize| digit(&hex[i..=i]).map(|v| v * 17);
            Some((d(0)?, d(1)?, d(2)?))
        }
        6 => Some((digit(&hex[0..2])?, digit(&hex[2..4])?, digit(&hex[4..6])?)),
        _ => None,
    }
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

/// The CSS basic colours and a few common light and warm names.
const NAMES: &[(&str, u32)] = &[
    ("black", 0x000000),
    ("silver", 0xC0C0C0),
    ("gray", 0x808080),
    ("grey", 0x808080),
    ("white", 0xFFFFFF),
    ("maroon", 0x800000),
    ("red", 0xFF0000),
    ("purple", 0x800080),
    ("fuchsia", 0xFF00FF),
    ("green", 0x008000),
    ("lime", 0x00FF00),
    ("olive", 0x808000),
    ("yellow", 0xFFFF00),
    ("navy", 0x000080),
    ("blue", 0x0000FF),
    ("teal", 0x008080),
    ("aqua", 0x00FFFF),
    ("orange", 0xFFA500),
    ("lightgray", 0xD3D3D3),
    ("lightgrey", 0xD3D3D3),
    ("whitesmoke", 0xF5F5F5),
    ("gainsboro", 0xDCDCDC),
    ("ivory", 0xFFFFF0),
    ("beige", 0xF5F5DC),
    ("snow", 0xFFFAFA),
    ("linen", 0xFAF0E6),
];
