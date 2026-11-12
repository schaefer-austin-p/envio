//! CSS widths on tables and cells become attributes Qt honours.

use std::path::Path;
use std::time::{Duration, Instant};

use envio_mime::{RenderOptions, Rendered, parse, render, sanitize_html};

fn options() -> RenderOptions {
    RenderOptions {
        blocked_label: "[image]".to_owned(),
        max_image_width: 360,
        ..RenderOptions::default()
    }
}

fn clean(html: &str) -> String {
    sanitize_html(html, &options()).html
}

fn corpus(name: &str) -> Rendered {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus")
        .join(name);
    render(&parse(&std::fs::read(path).unwrap()), &options())
}

/// The start tags of `name` in `html`.
fn tags<'h>(html: &'h str, name: &str) -> Vec<&'h str> {
    let open = format!("<{name}");
    html.match_indices(&open)
        .filter(|(i, _)| html[i + open.len()..].starts_with([' ', '>', '/']))
        .map(|(i, _)| &html[i..i + html[i..].find('>').unwrap() + 1])
        .collect()
}

fn attr<'t>(tag: &'t str, name: &str) -> Option<&'t str> {
    let key = format!(" {name}=\"");
    let start = tag.find(&key)? + key.len();
    Some(&tag[start..start + tag[start..].find('"')?])
}

#[test]
fn css_pixel_width_on_a_cell_becomes_the_width_attribute() {
    let html =
        clean(r#"<table><tr><td style="width:32px;color:red">x</td><td>y</td></tr></table>"#);
    let td = tags(&html, "td")[0];
    assert_eq!(attr(td, "width"), Some("32"), "{td}");
    assert!(!attr(td, "style").unwrap_or_default().contains("width"));
    assert_eq!(attr(td, "style"), Some("color:red"));
}

#[test]
fn css_percent_width_on_a_table_becomes_the_width_attribute() {
    let html = clean(r#"<table style="width: 100%"><tr><td>x</td><td>y</td></tr></table>"#);
    let table = tags(&html, "table")[0];
    assert_eq!(attr(table, "width"), Some("100%"), "{table}");
    assert_eq!(attr(table, "style"), None, "{table}");
}

#[test]
fn pixel_width_wider_than_the_screen_becomes_full_width() {
    let html = clean(r#"<table style="width: 480px"><tr><td>x</td><td>y</td></tr></table>"#);
    let table = tags(&html, "table")[0];
    assert_eq!(attr(table, "width"), Some("100%"), "{table}");
}

#[test]
fn width_attribute_wins_over_css() {
    let html =
        clean(r#"<table><tr><td width="20" style="width:40px">x</td><td>y</td></tr></table>"#);
    let td = tags(&html, "td")[0];
    assert_eq!(attr(td, "width"), Some("20"), "{td}");
    assert_eq!(attr(td, "style"), None, "the CSS width goes too: {td}");
}

#[test]
fn other_units_are_not_carried() {
    for width in ["10em", "auto", "150%"] {
        let html = clean(&format!(
            r#"<table><tr><td style="width:{width}">x</td><td>y</td></tr></table>"#
        ));
        let td = tags(&html, "td")[0];
        assert_eq!(attr(td, "width"), None, "{width}: {td}");
        assert_eq!(
            attr(td, "style"),
            None,
            "the CSS width goes too: {width}: {td}"
        );
    }
}

#[test]
fn a_width_attribute_the_message_set_is_kept_only_when_sane() {
    let cases = [
        ("20", Some("20")),
        ("20px", Some("20")),
        ("12pt", Some("16")),
        (" 20px!important", Some("20")),
        ("50%", Some("50%")),
        ("999", None),
        ("150%", None),
        ("0", None),
        ("-5", None),
        ("calc(100vw)", None),
    ];
    for (width, kept) in cases {
        let html = clean(&format!(
            r#"<table width="{width}"><colgroup><col width="{width}"></colgroup><tr><td width="{width}">x</td><td>y</td></tr></table>"#
        ));
        for tag in ["table", "col", "td"] {
            let got = tags(&html, tag)[0];
            // A percentage stays on tables and columns, never on cells.
            let kept = if tag == "td" && width.ends_with('%') {
                None
            } else {
                kept
            };
            assert_eq!(attr(got, "width"), kept, "{tag} {width}: {got}");
        }
    }
}

#[test]
fn percent_widths_on_cells_are_dropped() {
    // Qt resolves a percentage cell against the cell's minimum width, which
    // squeezes the text column beside a fixed one (#90).
    let html = clean(
        r#"<table><tr><td width="50%">a</td><td style="width:100%">b</td><th style="width:30%">c</th><th width="40%">d</th></tr></table>"#,
    );
    for tag in tags(&html, "td").into_iter().chain(tags(&html, "th")) {
        assert_eq!(attr(tag, "width"), None, "{tag}");
        assert_eq!(attr(tag, "style"), None, "{tag}");
    }
}

#[test]
fn pixel_widths_on_cells_are_kept() {
    let html = clean(
        r#"<table><tr><td width="16">a</td><td style="width:32px">b</td><th width="24">c</th></tr></table>"#,
    );
    assert_eq!(attr(tags(&html, "td")[0], "width"), Some("16"));
    assert_eq!(attr(tags(&html, "td")[1], "width"), Some("32"));
    assert_eq!(attr(tags(&html, "th")[0], "width"), Some("24"));
}

#[test]
fn percent_width_on_a_table_is_kept() {
    let html = clean(r#"<table width="80%"><tr><td>a</td><td>b</td></tr></table>"#);
    assert_eq!(attr(tags(&html, "table")[0], "width"), Some("80%"));
}

#[test]
fn col_keeps_its_width() {
    let html = clean(
        r#"<table><colgroup><col style="width:16px"></colgroup><tr><td>x</td><td>y</td></tr></table>"#,
    );
    let col = tags(&html, "col")[0];
    assert_eq!(attr(col, "width"), Some("16"), "{col}");
}

#[test]
fn receipt_like_keeps_its_spacer_and_card_widths() {
    let html = corpus("receipt-like.eml").html;
    let mut spacers = 0;
    for tag in tags(&html, "td").into_iter().chain(tags(&html, "table")) {
        if let Some(w) = attr(tag, "width").and_then(|w| w.parse::<u32>().ok()) {
            assert!(w <= 360, "{tag}");
        }
    }
    for td in tags(&html, "td") {
        if attr(td, "width").is_some_and(|w| matches!(w, "16" | "32")) {
            spacers += 1;
        }
    }
    assert!(spacers >= 10, "only {spacers} spacer cells kept a width");
    assert!(
        tags(&html, "table")
            .iter()
            .any(|t| attr(t, "width") == Some("100%")),
        "the card lost its full width"
    );
}

fn best_of_3(html: &str) -> Duration {
    (0..3)
        .map(|_| {
            let start = Instant::now();
            let _ = sanitize_html(html, &options());
            start.elapsed()
        })
        .min()
        .unwrap()
}

#[test]
fn rendering_scales_linearly() {
    let one = corpus("newsletter-like.eml").html;
    // Re-render the sanitised body repeated, as input HTML.
    let single = one.clone();
    let double = format!("{single}{single}");
    let t1 = best_of_3(&single);
    let t2 = best_of_3(&double);
    assert!(t1 < Duration::from_secs(2), "{t1:?}");
    assert!(
        t2 < 3 * t1.max(Duration::from_millis(1)),
        "{t1:?} vs {t2:?}"
    );
}
