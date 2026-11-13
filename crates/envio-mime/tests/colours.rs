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
    assert_eq!(parse_colour("papayawhip"), None);
    assert_eq!(parse_colour("#12"), None);
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
