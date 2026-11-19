//! The declarations of a `style` attribute as (property, value) pairs, with
//! the `font` shorthand expanded, plus the verdict on styles that clearly
//! hide their element (#341).

/// `style` as lower-cased property and trimmed value pairs, in order. The
/// `font` shorthand becomes `font-style`, `font-weight`, `font-size` and
/// `font-family`; one that cannot be read gives nothing.
pub(crate) fn declarations(style: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for declaration in style.split(';') {
        let Some((prop, value)) = declaration.split_once(':') else {
            continue;
        };
        let prop = prop.trim().to_ascii_lowercase();
        let value = value.trim();
        if prop == "font" {
            out.extend(font_shorthand(value));
        } else {
            out.push((prop, value.to_owned()));
        }
    }
    out
}

/// `value` lower-cased, without `!important`.
fn plain(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    let lower = match lower.rsplit_once('!') {
        Some((head, tail)) if tail.trim() == "important" => head.to_owned(),
        _ => lower,
    };
    lower.trim().to_owned()
}

/// `font: [style] [weight] size[/line-height] family, ...`. Only a size
/// followed by a family counts; a system font (`caption`) or any word
/// before the size that is not a style, weight or `normal` gives nothing.
fn font_shorthand(value: &str) -> Vec<(String, String)> {
    let value = value.trim();
    let mut out = Vec::new();
    let mut rest = value;
    loop {
        let (token, after) = match rest.split_once(char::is_whitespace) {
            Some((t, a)) => (t, a.trim_start()),
            None => (rest, ""),
        };
        let lower = token.to_ascii_lowercase();
        if token.is_empty() {
            return Vec::new();
        }
        if is_size(&lower) {
            let size = token.split('/').next().unwrap_or(token);
            let mut family = after;
            // `15px / 24px`: the line height is a separate word or two.
            if let Some(tail) = family.strip_prefix('/') {
                family = tail.trim_start();
                family = family
                    .split_once(char::is_whitespace)
                    .map_or("", |(_, f)| f.trim_start());
            } else if token.ends_with('/') {
                family = family
                    .split_once(char::is_whitespace)
                    .map_or("", |(_, f)| f.trim_start());
            }
            let family = family.trim();
            if family.is_empty() {
                return Vec::new();
            }
            out.push(("font-size".to_owned(), size.to_owned()));
            out.push(("font-family".to_owned(), family.to_owned()));
            return out;
        }
        match lower.as_str() {
            "italic" | "oblique" => out.push(("font-style".to_owned(), lower)),
            "bold" | "bolder" | "lighter" | "100" | "200" | "300" | "400" | "500" | "600"
            | "700" | "800" | "900" => out.push(("font-weight".to_owned(), lower)),
            "normal" | "small-caps" => {}
            _ => return Vec::new(),
        }
        if after.is_empty() {
            return Vec::new();
        }
        rest = after;
    }
}

fn is_size(token: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "xx-small", "x-small", "small", "medium", "large", "x-large", "xx-large", "smaller",
        "larger",
    ];
    let word = token.split('/').next().unwrap_or(token);
    KEYWORDS.contains(&word)
        || (word.starts_with(|c: char| c.is_ascii_digit() || c == '.')
            && word.contains(|c: char| c.is_ascii_alphabetic() || c == '%'))
}

/// A font-family list with `sans-serif` added when it names a common sans
/// face and no generic family, so a face the device lacks falls back to a
/// sans, not to the reader's serif.
pub(crate) fn with_generic(families: &str) -> String {
    const GENERIC: &[&str] = &["serif", "sans-serif", "monospace", "cursive", "fantasy"];
    const SANS: &[&str] = &[
        "arial",
        "helvetica",
        "helvetica neue",
        "verdana",
        "tahoma",
        "trebuchet ms",
        "segoe ui",
        "geneva",
        "calibri",
        "roboto",
        "open sans",
        "lucida grande",
        "system-ui",
        "-apple-system",
    ];
    let names: Vec<String> = families
        .split(',')
        .map(|f| f.trim().trim_matches(['"', '\'']).to_ascii_lowercase())
        .collect();
    let has_generic = names.iter().any(|n| GENERIC.contains(&n.as_str()));
    if !has_generic && names.iter().any(|n| SANS.contains(&n.as_str())) {
        format!("{}, sans-serif", families.trim())
    } else {
        families.trim().to_owned()
    }
}

/// Whether the declarations clearly hide their element in a browser:
/// `opacity:0`, or a box collapsed to nothing and clipped (`max-height:0`,
/// `height:0` or `max-width:0` with `overflow:hidden`). A zero font size or
/// line height never hides on its own or in company: it is the newsletter
/// trick that removes the gap around an image or button, whose content
/// shows. `mso-hide` is Outlook only, so it hides nothing here. The last
/// declaration of a property wins.
pub(crate) fn collapsed(decls: &[(String, String)]) -> bool {
    let get = |name: &str| {
        decls
            .iter()
            .rev()
            .find(|(p, _)| p == name)
            .map(|(_, v)| plain(v))
    };
    let zero = |name: &str| {
        get(name).is_some_and(|v| {
            let end = v.find(|c: char| c.is_ascii_alphabetic() || c == '%');
            let number = &v[..end.unwrap_or(v.len())];
            !number.is_empty() && number.parse::<f64>() == Ok(0.0)
        })
    };
    let clipped = get("overflow").as_deref() == Some("hidden");
    let box_collapsed = zero("max-height") || zero("height") || zero("max-width");
    zero("opacity") || (box_collapsed && clipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decl(style: &str) -> Vec<(String, String)> {
        declarations(style)
    }

    fn pair(p: &str, v: &str) -> (String, String) {
        (p.to_owned(), v.to_owned())
    }

    #[test]
    fn important_is_ignored_when_judging_a_collapsed_box() {
        assert!(collapsed(&decl("opacity: 0 !important")));
        assert!(collapsed(&decl(
            "max-height:0px ! important;overflow:Hidden !IMPORTANT"
        )));
        assert!(!collapsed(&decl("opacity:0.1 !important")));
    }

    #[test]
    fn the_last_declaration_wins() {
        assert!(!collapsed(&decl("opacity:0;opacity:1")));
        assert!(collapsed(&decl("opacity:1;opacity:0")));
    }

    #[test]
    fn zero_font_or_line_height_never_collapses() {
        for style in [
            "font-size:0;line-height:0",
            "font-size:0;overflow:hidden",
            "font-size:0;max-height:0",
            "line-height:0",
        ] {
            assert!(!collapsed(&decl(style)), "{style}");
        }
    }

    #[test]
    fn a_system_font_keyword_gives_nothing() {
        assert!(decl("font: caption").is_empty());
        assert!(decl("font:menu").is_empty());
    }

    #[test]
    fn an_empty_or_sizeless_shorthand_gives_nothing() {
        assert!(decl("font:").is_empty());
        assert!(decl("font:   ").is_empty());
        assert!(decl("font:12px").is_empty());
        assert!(decl("font:italic bold").is_empty());
    }

    #[test]
    fn the_line_height_form_is_skipped() {
        let want = vec![
            pair("font-size", "12px"),
            pair("font-family", "Arial, sans-serif"),
        ];
        assert_eq!(decl("font:12px/18px Arial, sans-serif"), want);
        assert_eq!(decl("font:12px / 18px Arial, sans-serif"), want);
        assert_eq!(decl("font:12px /18px Arial, sans-serif"), want);
    }

    #[test]
    fn weight_and_style_come_before_the_size() {
        assert_eq!(
            decl("font:italic 700 1em Georgia, serif"),
            vec![
                pair("font-style", "italic"),
                pair("font-weight", "700"),
                pair("font-size", "1em"),
                pair("font-family", "Georgia, serif"),
            ]
        );
    }
}
