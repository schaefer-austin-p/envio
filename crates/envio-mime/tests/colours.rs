//! Mail that sets its own colours is flagged, so the reader draws it on a
//! light card.

use std::collections::HashMap;
use std::path::Path;

use envio_mime::{RenderOptions, parse, parse_colour, render, sanitize_html, sets_colours};

fn options() -> RenderOptions {
    RenderOptions {
        allow_remote_images: false,
        cid_urls: HashMap::new(),
        blocked_label: "[image]".to_owned(),
        max_image_width: 600,
    }
}

fn own(html: &str) -> bool {
    sanitize_html(html, &options()).own_colours
}

fn corpus(name: &str) -> bool {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus")
        .join(name);
    render(&parse(&std::fs::read(path).unwrap()), &options()).own_colours
}

#[test]
fn receipt_like_sets_colours() {
    assert!(corpus("receipt-like.eml"));
}

#[test]
fn notice_like_sets_colours() {
    assert!(corpus("notice-like.eml"));
}

#[test]
fn a_font_color_attribute_counts() {
    assert!(own(r##"<p><font color="#1a1f36">hi</font></p>"##));
}

#[test]
fn a_bgcolor_counts() {
    assert!(own(
        r##"<table><tr><td bgcolor="#ffffff">hi</td></tr></table>"##
    ));
}

#[test]
fn a_style_background_color_counts() {
    assert!(own(r##"<div style="background-color:#fafafa">hi</div>"##));
}

#[test]
fn html_without_colours_does_not() {
    assert!(!own("<p>Hello <b>there</b></p>"));
}

#[test]
fn text_that_mentions_color_is_not_a_colour() {
    assert!(!own("<p>color: red</p>"));
}

#[test]
fn light_text_without_a_background_does_not() {
    assert!(!own(r##"<p style="color:#eeeeee">hi</p>"##));
}

#[test]
fn dark_text_without_a_background_counts() {
    assert!(own(r##"<p style="color:#1a1f36">hi</p>"##));
}

#[test]
fn light_named_and_functional_text_does_not() {
    for html in [
        r#"<font color="white">hi</font>"#,
        r#"<p style="color:WhiteSmoke">hi</p>"#,
        r#"<p style="color:rgb(238,238,238)">hi</p>"#,
        r#"<p style="color:hsl(0,0%,90%)">hi</p>"#,
    ] {
        assert!(!own(html), "{html}");
    }
}

#[test]
fn an_unknown_colour_without_a_background_does_not() {
    assert!(!own(r#"<p style="color:papayawhip">hi</p>"#));
}

#[test]
fn parse_colour_reads_each_form() {
    assert_eq!(parse_colour("#fff"), Some((255, 255, 255)));
    assert_eq!(parse_colour("#1a1f36"), Some((0x1a, 0x1f, 0x36)));
    assert_eq!(parse_colour("rgb(10, 20, 30)"), Some((10, 20, 30)));
    assert_eq!(parse_colour("rgba(10,20,30,0.5)"), Some((10, 20, 30)));
    assert_eq!(parse_colour("rgb(100%, 0%, 50%)"), Some((255, 0, 128)));
    assert_eq!(parse_colour("hsl(0, 100%, 50%)"), Some((255, 0, 0)));
    assert_eq!(parse_colour("hsla(120, 100%, 25%, 1)"), Some((0, 128, 0)));
    assert_eq!(parse_colour("NAVY"), Some((0, 0, 128)));
    assert_eq!(parse_colour("WhiteSmoke"), Some((245, 245, 245)));
    assert_eq!(parse_colour("#1a1f36 !important"), Some((0x1a, 0x1f, 0x36)));
    assert_eq!(parse_colour("papayawhip"), Some((255, 239, 213)));
    assert_eq!(parse_colour("notacolour"), None);
    assert_eq!(parse_colour("#12"), None);
}

#[test]
fn dark_css_names_parse() {
    assert_eq!(parse_colour("dimgray"), Some((105, 105, 105)));
    assert_eq!(parse_colour("darkslategray"), Some((47, 79, 79)));
    assert_eq!(parse_colour("darkblue"), Some((0, 0, 139)));
    assert_eq!(parse_colour("DarkRed"), Some((139, 0, 0)));
    assert_eq!(parse_colour("midnightblue"), Some((25, 25, 112)));
    assert_eq!(parse_colour("rebeccapurple"), Some((102, 51, 153)));
    assert_eq!(parse_colour("lightgoldenrodyellow"), Some((250, 250, 210)));
    assert!(sets_colours(r#"<p style="color:dimgray">x</p>"#));
    assert!(sets_colours(r#"<font color="midnightblue">x</font>"#));
}

#[test]
fn four_and_eight_digit_hex_ignore_alpha() {
    assert_eq!(parse_colour("#f00a"), Some((255, 0, 0)));
    assert_eq!(parse_colour("#1a1f36cc"), Some((0x1a, 0x1f, 0x36)));
    assert_eq!(parse_colour("#12345"), None);
    assert!(sets_colours(r##"<p style="color:#000a">x</p>"##));
}

#[test]
fn a_background_that_is_no_colour_is_no_evidence() {
    let none = [
        "transparent",
        "inherit",
        "initial",
        "currentcolor",
        "none",
        "",
        "garbage",
        "#fff0",
        "rgba(0,0,0,0)",
    ];
    for bg in none {
        let style = format!(r#"<p style="color:#ddd; background-color:{bg}">x</p>"#);
        assert!(!sets_colours(&style), "{bg}");
    }
    for bg in ["", "transparent", "garbage", "inherit"] {
        let attr = format!(r#"<table bgcolor="{bg}"><tr><td>x</td></tr></table>"#);
        assert!(!sets_colours(&attr), "bgcolor {bg:?}");
    }
    assert!(sets_colours(r#"<p style="background-color:white">x</p>"#));
    assert!(sets_colours(r#"<td bgcolor="navy">x</td>"#));
}

#[test]
fn the_dark_threshold_is_wcag_luminance() {
    assert!(sets_colours(r##"<p style="color:#666666">x</p>"##));
    assert!(!sets_colours(r#"<p style="color:gray">x</p>"#));
    assert!(!sets_colours(r##"<p style="color:#777777">x</p>"##));
}

#[test]
fn plain_text_mail_does_not() {
    let raw =
        b"From: a@example.org\r\nSubject: s\r\nContent-Type: text/plain\r\n\r\ncolor: red\r\n";
    assert!(!render(&parse(raw), &options()).own_colours);
}

#[test]
fn too_deep_mail_does_not() {
    let html = format!(
        "{}x{}",
        r##"<div style="background-color:#fff">"##.repeat(500),
        "</div>".repeat(500)
    );
    assert!(!own(&html));
}

#[test]
fn bare_hex_bgcolor_sets_colours() {
    assert!(sets_colours(r#"<td bgcolor="ffffff">x</td>"#));
    assert!(sets_colours(r#"<td bgcolor="fff">x</td>"#));
    assert!(!sets_colours(r#"<td bgcolor="ffff">x</td>"#));
    assert!(!sets_colours(r#"<p style="background:ffffff">x</p>"#));
}

#[test]
fn a_bare_hex_table_bgcolor_sets_colours_through_the_sanitiser() {
    // The real path: linearise turns the cell's bgcolor into a style first.
    assert!(own(
        r#"<table><tr><td bgcolor="ffffff">hi</td></tr></table>"#
    ));
    assert!(own(r#"<table><tr bgcolor="fff"><td>hi</td></tr></table>"#));
}
