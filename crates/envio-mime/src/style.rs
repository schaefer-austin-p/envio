//! The `style` attribute: which CSS properties survive, and which values.

use crate::cssdecl::{collapsed, declarations, with_generic};

/// CSS properties kept in `style`.
const STYLE_PROPS: &[&str] = &[
    "color",
    "background-color",
    "font-family",
    "font-size",
    "font-style",
    "font-weight",
    "text-decoration",
    "text-align",
    "vertical-align",
    "white-space",
    "margin",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
    "padding",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
];

/// Also kept on `img`, so the image pass can spot 1x1 pixels and size the
/// image; it turns them into attributes (Qt ignores CSS width on images).
const IMG_SIZE_PROPS: &[&str] = &["width", "height", "max-width"];

/// Substrings that make a CSS value unsafe, checked in lower case.
const BAD_CSS: &[&str] = &[
    "url(",
    "var(",
    "env(",
    "expression",
    "\\",
    "@",
    "/*",
    "<",
    ">",
    "{",
    "}",
    "javascript:",
    "vbscript:",
];

const DISPLAY_NONE: &str = "display:none";
const VISIBILITY_HIDDEN: &str = "visibility:hidden";

/// The one `display` or `visibility` value that survives filtering, as a
/// marker: the image pass sees it and removes the element with its content
/// (ammonia can only drop whole tags by name, not by style).
fn hiding(prop: &str, value: &str) -> Option<&'static str> {
    let value = value.to_ascii_lowercase();
    let value = value.trim_end_matches("important").trim_end();
    let value = value.strip_suffix('!').unwrap_or(value).trim_end();
    match (prop, value) {
        ("display", "none") => Some(DISPLAY_NONE),
        ("visibility", "hidden") => Some(VISIBILITY_HIDDEN),
        _ => None,
    }
}

/// Whether a filtered `style` value hides its element.
pub(crate) fn is_hidden(style: &str) -> bool {
    style
        .split(';')
        .any(|d| d == DISPLAY_NONE || d == VISIBILITY_HIDDEN)
}

/// Keeps only allowed properties with safe values; `None` when nothing is
/// left.
pub(crate) fn filter_style(element: &str, style: &str) -> Option<String> {
    let decls = declarations(style);
    let collapsed = collapsed(&decls).then(|| DISPLAY_NONE.to_owned());
    let kept: Vec<String> = decls
        .iter()
        .filter_map(|(prop, value)| {
            let (prop, value) = (prop.clone(), value.as_str());
            if let Some(marker) = hiding(&prop, value) {
                return Some(marker.to_owned());
            }
            let allowed = STYLE_PROPS.contains(&prop.as_str())
                || (element == "img" && IMG_SIZE_PROPS.contains(&prop.as_str()))
                || (matches!(element, "table" | "td" | "th" | "col") && prop == "width")
                || prop == "background";
            if !(allowed && safe_css_value(value) && sane_lengths(&prop, value)) {
                return None;
            }
            if prop.starts_with("margin") {
                return vertical_margins(&prop, value);
            }
            if prop == "background" {
                return shorthand_colour(value);
            }
            if prop == "font-family" {
                return Some(format!("{prop}:{}", with_generic(value)));
            }
            Some(format!("{prop}:{value}"))
        })
        .chain(collapsed)
        .collect();
    (!kept.is_empty()).then(|| kept.join(";"))
}

/// Qt 5's TextEdit paints a block's background from the left edge but
/// indents its text by the left margin, so a coloured box with side
/// margins shows its colour beside its text. Qt also reads `auto` (the
/// usual email centring) as a wide margin. Only the top and bottom margins
/// are kept, and `auto` is dropped.
fn vertical_margins(prop: &str, value: &str) -> Option<String> {
    // `!important` may be written `! important`; nothing after `!` is a length.
    let value = value
        .split('!')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let parts: Vec<&str> = value.split_whitespace().collect();
    let (top, bottom) = match (prop, parts.as_slice()) {
        ("margin", [all]) | ("margin", [all, _]) => (Some(*all), Some(*all)),
        ("margin", [top, _, bottom]) | ("margin", [top, _, bottom, _]) => {
            (Some(*top), Some(*bottom))
        }
        ("margin-top", [top]) => (Some(*top), None),
        ("margin-bottom", [bottom]) => (None, Some(*bottom)),
        _ => (None, None),
    };
    let kept: Vec<String> = [("margin-top", top), ("margin-bottom", bottom)]
        .into_iter()
        .filter_map(|(side, v)| v.filter(|v| *v != "auto").map(|v| format!("{side}:{v}")))
        .collect();
    (!kept.is_empty()).then(|| kept.join(";"))
}

/// The `background` shorthand reduced to `background-color` (Qt 5 rich
/// text has no shorthand), so a call-to-action link or cell keeps its
/// fill (#178). The first colour token wins and is written normalised
/// (`#rrggbb`, `transparent`, or as written lower-cased when it is half
/// transparent). A shorthand with no colour (`none`, a gradient, a
/// keyword-only value) gives `background-color:transparent`, so it resets
/// an earlier colour.
///
/// `None` when any token is not a plain keyword, length, colour or
/// gradient (`var(`, `env(`, escapes, comments and so on): nothing of that
/// value is kept. A value with `url(` never gets this far; the gate in
/// [`filter_style`] drops the whole value, colour included, so a value
/// with a URL is never partially trusted.
fn shorthand_colour(value: &str) -> Option<String> {
    let value = strip_important(value);
    let lower = value.to_ascii_lowercase();
    if lower.contains("var(") || lower.contains("env(") {
        return None;
    }
    let mut colour = None;
    for token in tokens(value) {
        let lower = token.to_ascii_lowercase();
        if lower == "!important" {
            continue;
        }
        if lower.trim_end_matches("!important") == "transparent" {
            colour.get_or_insert_with(|| "transparent".to_owned());
        } else if crate::colours::parse_colour(&lower).is_some() {
            colour.get_or_insert_with(|| match crate::colours::paint(&lower) {
                crate::colours::Paint::Opaque(hex) => hex,
                crate::colours::Paint::Clear => "transparent".to_owned(),
                crate::colours::Paint::Unknown => {
                    lower.trim_end_matches("!important").trim_end().to_owned()
                }
            });
        } else if !plain(&lower) {
            return None;
        }
    }
    Some(format!(
        "background-color:{}",
        colour.as_deref().unwrap_or("transparent")
    ))
}

/// `value` without a trailing `!important`, however it is spaced
/// (`! important`).
fn strip_important(value: &str) -> &str {
    match value.rsplit_once('!') {
        Some((head, tail)) if tail.trim().eq_ignore_ascii_case("important") => head.trim_end(),
        _ => value,
    }
}

/// `value` split at whitespace outside parentheses.
fn tokens(value: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut start, mut depth) = (None, 0usize);
    for (i, c) in value.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
        match (c.is_whitespace() && depth == 0, start) {
            (true, Some(s)) => {
                out.push(&value[s..i]);
                start = None;
            }
            (false, None) => start = Some(i),
            _ => {}
        }
    }
    out.extend(start.map(|s| &value[s..]));
    out
}

/// A non-colour shorthand token that is harmless: a keyword, length or
/// percentage (letters, digits and `-%.,/`), or a linear or radial
/// gradient whose arguments are made of the same plus `#`, spaces and
/// parentheses.
fn plain(token: &str) -> bool {
    let word = |c: char| c.is_ascii_alphanumeric() || "-%.,/".contains(c);
    match token.split_once('(') {
        None => token.chars().all(word),
        Some((name, args)) => {
            matches!(name, "linear-gradient" | "radial-gradient")
                && args.ends_with(')')
                && args.chars().all(|c| word(c) || "#() ".contains(c))
        }
    }
}

fn safe_css_value(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let balanced = |q: char| value.chars().filter(|&c| c == q).count() % 2 == 0;
    !value.is_empty()
        && !BAD_CSS.iter().any(|bad| lower.contains(bad))
        && balanced('"')
        && balanced('\'')
}

/// Longest length (in any unit) we keep in a style.
const MAX_LENGTH: f64 = 10_000.0;

/// For the properties that hold lengths, every number must parse and stay
/// at or below [`MAX_LENGTH`]; anything else (`calc(`, `1e999`) is dropped.
fn sane_lengths(prop: &str, value: &str) -> bool {
    let is_length = matches!(prop, "width" | "height" | "font-size")
        || prop.starts_with("margin")
        || prop.starts_with("padding");
    if !is_length {
        return true;
    }
    let value = value.to_ascii_lowercase();
    let value = value.replace("!important", "");
    if value.contains('(') {
        return false;
    }
    let numeric = |c: char| c.is_ascii_digit() || "+-.".contains(c);
    value.split_whitespace().all(|token| {
        if !token.starts_with(numeric) {
            return true;
        }
        let end = token.find(|c: char| !numeric(c)).unwrap_or(token.len());
        let unit = &token[end..];
        unit.chars().all(|c| c.is_ascii_alphabetic() || c == '%')
            && token[..end]
                .parse::<f64>()
                .is_ok_and(|n| n.is_finite() && n.abs() <= MAX_LENGTH)
    })
}
