//! Relative and unparseable font sizes are clamped like absolute ones
//! (#191): at most 26px, at least 1px, compounding included.

use envio_mime::{RenderOptions, sanitize_html};

fn clean(html: &str) -> String {
    sanitize_html(html, &RenderOptions::default()).html
}

fn sized(value: &str) -> String {
    clean(&format!(r#"<div style="font-size:{value}">t</div>"#))
}

#[test]
fn big_percentages_and_keywords_are_clamped() {
    for big in [
        "300%",
        "163%",
        "xx-large",
        "xxx-large",
        "XX-LARGE",
        "300% !important",
    ] {
        assert_eq!(
            sized(big),
            r#"<div style="font-size:26px">t</div>"#,
            "{big}"
        );
    }
    for ok in [
        "100%", "120%", "x-large", "medium", "smaller", "larger", "x-small",
    ] {
        let html = format!(r#"<div style="font-size:{ok}">t</div>"#);
        assert_eq!(clean(&html), html, "{ok}");
    }
}

#[test]
fn tiny_percentages_become_one_pixel() {
    for tiny in ["0%", "1%", "5%"] {
        assert_eq!(
            sized(tiny),
            r#"<div style="font-size:1px">t</div>"#,
            "{tiny}"
        );
    }
}

#[test]
fn nested_percentages_compound() {
    assert_eq!(
        clean(r#"<div style="font-size:300%"><p style="font-size:300%">t</p></div>"#),
        r#"<div style="font-size:26px"><p style="font-size:26px">t</p></div>"#
    );
    // 150% twice is 36px: each step alone is in range, together they are not.
    assert_eq!(
        clean(r#"<div style="font-size:150%"><span style="font-size:150%">t</span></div>"#),
        r#"<div style="font-size:150%"><span style="font-size:26px">t</span></div>"#
    );
    assert_eq!(
        clean(
            r#"<div style="font-size:1.5em"><div><span style="font-size:1.5em">t</span></div></div>"#
        ),
        r#"<div style="font-size:1.5em"><div><span style="font-size:26px">t</span></div></div>"#
    );
}

#[test]
fn a_shrunk_parent_shrinks_the_child() {
    // 10% of 16px is 1.6px; 1000% of that is 16px: in range, kept.
    let html = r#"<div style="font-size:10%"><span style="font-size:1000%">t</span></div>"#;
    assert_eq!(clean(html), html);
    assert_eq!(
        clean(r#"<div style="font-size:1%"><span style="font-size:50%">t</span></div>"#),
        r#"<div style="font-size:1px"><span style="font-size:1px">t</span></div>"#
    );
}

#[test]
fn siblings_do_not_inherit_from_each_other() {
    let html = r#"<div style="font-size:150%">a</div><div style="font-size:150%">b</div>"#;
    assert_eq!(clean(html), html);
}

#[test]
fn rem_ignores_the_parent() {
    let html = r#"<div style="font-size:20px"><span style="font-size:1.5rem">t</span></div>"#;
    assert_eq!(clean(html), html);
}

#[test]
fn values_that_cannot_be_parsed_are_dropped() {
    for bad in [
        "1e3px",
        "1e3%",
        "40px 40px",
        "inherit 300%",
        "calc(300%)",
        "huge",
        "300",
        "300 %",
        "10vw",
        "5ex",
    ] {
        assert_eq!(
            clean(&format!(r#"<div style="font-size:{bad}">t</div>"#)),
            "<div>t</div>",
            "{bad}"
        );
    }
}

#[test]
fn the_font_shorthand_is_dropped() {
    assert_eq!(
        clean(r#"<div style="font:300% serif;color:red">t</div>"#),
        r#"<div style="color:red">t</div>"#
    );
}

#[test]
fn font_size_attributes_are_clamped() {
    for (size, kept) in [
        ("7", "5"),
        ("6", "5"),
        ("+4", "5"),
        ("+7", "5"),
        ("-9", "1"),
        ("0", "1"),
        ("2", "2"),
        ("5", "5"),
        ("+1", "4"),
    ] {
        assert_eq!(
            clean(&format!(r#"<font size="{size}">t</font>"#)),
            format!(r#"<font size="{kept}">t</font>"#),
            "{size}"
        );
    }
    for bad in ["", "big", "1e1", "3 3"] {
        assert_eq!(
            clean(&format!(r#"<font size="{bad}">t</font>"#)),
            "<font>t</font>",
            "{bad}"
        );
    }
}

#[test]
fn a_font_tag_with_a_relative_style_compounds_too() {
    assert_eq!(
        clean(r#"<font style="font-size:200%"><font style="font-size:200%">t</font></font>"#),
        r#"<font style="font-size:26px"><font style="font-size:26px">t</font></font>"#
    );
}
