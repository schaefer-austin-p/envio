//! The `style` attribute: which CSS properties survive, and which values.

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
const IMG_SIZE_PROPS: &[&str] = &["width", "height"];

/// Substrings that make a CSS value unsafe, checked in lower case.
const BAD_CSS: &[&str] = &[
    "url(",
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
    let kept: Vec<String> = style
        .split(';')
        .filter_map(|declaration| {
            let (prop, value) = declaration.split_once(':')?;
            let prop = prop.trim().to_ascii_lowercase();
            let value = value.trim();
            if let Some(marker) = hiding(&prop, value) {
                return Some(marker.to_owned());
            }
            let allowed = STYLE_PROPS.contains(&prop.as_str())
                || (element == "img" && IMG_SIZE_PROPS.contains(&prop.as_str()));
            (allowed && safe_css_value(value) && sane_lengths(&prop, value))
                .then(|| format!("{prop}:{value}"))
        })
        .collect();
    (!kept.is_empty()).then(|| kept.join(";"))
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
