//! The font-size pass sees the tree Qt finally sees (#191, review 995),
//! survives huge `<font size>` values, and never lets a size through that is
//! above the limit.

use envio_mime::{RenderOptions, sanitize_html};
use proptest::prelude::*;

fn clean(html: &str) -> String {
    sanitize_html(html, &RenderOptions::default()).html
}

#[test]
fn huge_font_tag_sizes_do_not_overflow() {
    for (size, kept) in [
        ("+2147483647", "5"),
        ("-2147483647", "1"),
        ("+2147483648", "5"),
        ("-2147483648", "1"),
        ("+99999999999999999999", "5"),
        ("2147483647", "5"),
        ("99999999999999999999", "5"),
    ] {
        let want = if kept.is_empty() {
            "<font>t</font>".to_owned()
        } else {
            format!(r#"<font size="{kept}">t</font>"#)
        };
        assert_eq!(
            clean(&format!(r#"<font size="{size}">t</font>"#)),
            want,
            "{size}"
        );
    }
}

#[test]
fn a_void_element_does_not_swallow_the_stack() {
    assert_eq!(
        clean(r#"<p><wbr style="font-size:1px"><span style="font-size:1500%">t</span></p>"#),
        r#"<p><span style="font-size:26px">t</span></p>"#
    );
}

#[test]
fn an_unwrapped_tag_does_not_count() {
    // `section` is not a Qt tag: it is removed, so its size never applies.
    assert_eq!(
        clean(r#"<section style="font-size:1px"><span style="font-size:1500%">t</span></section>"#),
        r#"<span style="font-size:26px">t</span>"#
    );
}

#[test]
fn dropped_table_parts_do_not_count() {
    let out = clean(
        r#"<table><tbody style="font-size:1px"><tr style="font-size:1px"><td><span style="font-size:1500%">t</span></td></tr></tbody></table>"#,
    );
    assert!(!out.contains("1500%"), "{out}");
    assert!(out.contains("font-size:26px"), "{out}");
}

#[test]
fn several_declarations_use_the_last_valid_one() {
    assert_eq!(
        clean(
            r#"<div style="font-size:200%;color:red;font-size:300%"><span style="font-size:200%">t</span></div>"#
        ),
        r#"<div style="font-size:26px;color:red;font-size:26px"><span style="font-size:26px">t</span></div>"#
    );
}

#[test]
fn relative_sizes_under_headings_stay_in_range() {
    for tag in [
        "h1", "h2", "h3", "h4", "h5", "h6", "big", "small", "sub", "sup",
    ] {
        let out = clean(&format!(
            r#"<{tag} style="font-size:150%"><span style="font-size:1500%">t</span></{tag}>"#
        ));
        assert!(sizes_of(&out).iter().all(|s| *s <= 26.0), "{tag}: {out}");
    }
}

/// Every `font-size:` px value and `size=` level in `html`.
fn sizes_of(html: &str) -> Vec<f64> {
    let mut sizes = Vec::new();
    for part in html.split("font-size:").skip(1) {
        let value: String = part
            .chars()
            .take_while(|c| !matches!(c, ';' | '"'))
            .collect();
        // Relative sizes are checked by `settled`: a second pass keeps them.
        match value.strip_suffix("px").map(str::parse::<f64>) {
            Some(Ok(n)) => sizes.push(n),
            Some(Err(_)) => sizes.push(f64::INFINITY),
            None if value.ends_with("large") && value.starts_with("xx") => {
                sizes.push(f64::INFINITY)
            }
            None => sizes.push(0.0),
        }
    }
    for part in html.split(" size=\"").skip(1) {
        let value: String = part.chars().take_while(|c| *c != '"').collect();
        sizes.push(value.parse::<f64>().map_or(f64::INFINITY, |n| {
            if n <= 5.0 { 0.0 } else { f64::INFINITY }
        }));
    }
    sizes
}

fn element() -> impl Strategy<Value = (&'static str, &'static str, &'static str)> {
    let tag = prop::sample::select(vec![
        "div", "span", "p", "font", "h1", "h3", "big", "small", "section", "table", "tbody", "tr",
        "td", "center", "b", "i",
    ]);
    let size = prop::sample::select(vec![
        "",
        "font-size:300%",
        "font-size:150%",
        "font-size:1500%",
        "font-size:5%",
        "font-size:2em",
        "font-size:xx-large",
        "font-size:larger",
        "font-size:1px",
        "font-size:smaller",
        "font-size:12pt",
        "font-size:1e3px",
        "font-size:99999rem",
    ]);
    let attr = prop::sample::select(vec!["", "7", "+9", "-5", "3", "+2147483647", "x"]);
    (tag, size, attr)
}

fn nested() -> impl Strategy<Value = String> {
    prop::collection::vec(element(), 1..8).prop_map(|els| {
        let mut open = String::new();
        let mut close = String::new();
        for (tag, size, attr) in els {
            let style = if size.is_empty() {
                String::new()
            } else {
                format!(r#" style="{size}""#)
            };
            let size_attr = if tag == "font" && !attr.is_empty() {
                format!(r#" size="{attr}""#)
            } else {
                String::new()
            };
            open.push_str(&format!("<{tag}{style}{size_attr}>"));
            close = format!("</{tag}>{close}");
        }
        format!("{open}t{close}")
    })
}

proptest! {
    #[test]
    fn no_size_survives_above_the_limit(html in nested()) {
        let out = clean(&html);
        for size in sizes_of(&out) {
            prop_assert!(size <= 26.0, "{out}");
        }
        prop_assert_eq!(clean(&out), out);
    }
}
