//! Sanitizer rules one at a time (images: tests/images.rs).

use envio_mime::{RenderOptions, sanitize_html};

fn clean(html: &str) -> String {
    let options = RenderOptions::default();
    sanitize_html(html, &options).html
}

#[test]
fn keeps_qt_formatting() {
    let html = r#"<p align="center"><b>bold</b> <i>it</i> <u>u</u> <font color="red" face="Sans" size="3">f</font></p><ul type="disc"><li>one</li></ul><hr width="50%">"#;
    assert_eq!(clean(html), html);
}

#[test]
fn drops_unknown_tags_but_keeps_their_text() {
    assert_eq!(clean("<article><section>text</section></article>"), "text");
}

#[test]
fn drops_script_style_and_their_content() {
    assert_eq!(
        clean("<p>a</p><script>x()</script><style>p{}</style><p>b</p>"),
        "<p>a</p><p>b</p>"
    );
}

#[test]
fn drops_class_id_and_events() {
    assert_eq!(
        clean(r#"<p class="x" id="y" onclick="z()">t</p>"#),
        "<p>t</p>"
    );
}

#[test]
fn keeps_exact_http_https_mailto_links() {
    for href in [
        "https://example.com/a?b=1&amp;c=2",
        "http://example.com/",
        "mailto:bob@example.org",
    ] {
        let html = format!(r#"<a href="{href}">x</a>"#);
        assert_eq!(clean(&html), html);
    }
}

#[test]
fn drops_other_link_schemes_but_keeps_text() {
    for href in [
        "javascript:alert(1)",
        "  javascript:alert(1)",
        "JAVASCRIPT:alert(1)",
        "vbscript:x",
        "data:text/html,x",
        "file:///etc/passwd",
        "cid:part1",
        "relative/path",
        "//example.com/x",
    ] {
        let html = format!(r#"<a href="{href}">x</a>"#);
        assert_eq!(clean(&html), "<a>x</a>", "{href}");
    }
}

#[test]
fn filters_style_properties() {
    assert_eq!(
        clean(r#"<p style="color: red; position: fixed; font-weight:bold">t</p>"#),
        r#"<p style="color:red;font-weight:bold">t</p>"#
    );
    assert_eq!(
        clean(r#"<p style="background-color:url(x);color:expression(1)">t</p>"#),
        "<p>t</p>"
    );
    assert_eq!(
        clean(r#"<p style="font-family:'Open Sans">t</p>"#),
        "<p>t</p>"
    );
}

#[test]
fn width_in_style_only_counts_on_images() {
    assert_eq!(clean(r#"<div style="width:900px">t</div>"#), "<div>t</div>");
}

#[test]
fn escapes_text_and_attribute_values() {
    assert_eq!(clean("<p>1 &lt; 2 &amp; 3</p>"), "<p>1 &lt; 2 &amp; 3</p>");
    assert_eq!(clean("a < b"), "a &lt; b");
}

#[test]
fn absurd_nesting_becomes_escaped_plain_text() {
    let n = 10_000;
    let html = format!(
        "{}hello <b>&amp; world{}",
        "<div>".repeat(n),
        "</div>".repeat(n)
    );
    let out = clean(&html);
    assert!(out.contains("hello"), "{}", &out[..out.len().min(200)]);
    assert!(out.contains("world"));
    assert!(!out.contains("<div>"));
    assert!(!out.contains("<b>"));
    let evil = format!("{}<script>x</script>&lt;b&gt;", "<div>".repeat(n));
    assert!(!clean(&evil).contains("<script"));
}

#[test]
fn ordinary_nesting_still_goes_through_ammonia() {
    let n = 50;
    let html = format!(
        "{}<b>cell</b>{}",
        "<table><tr><td>".repeat(n),
        "</td><td>x</td></tr></table>".repeat(n)
    );
    let out = clean(&html);
    // Each outer table lays out a nested one beside text, so it stacks into
    // blocks; the innermost holds only short text and stays a table.
    assert_eq!(out.matches("<table>").count(), 1, "{out}");
    assert_eq!(out.matches("<td>").count(), 2, "{out}");
    assert!(out.contains("<b>cell</b>"));
    assert!(!out.contains("white-space"));
}

#[test]
fn css_braces_are_refused() {
    assert_eq!(
        clean(r#"<p style="color:red}p{color:blue">t</p>"#),
        "<p>t</p>"
    );
}

#[test]
fn links_with_control_characters_or_spaced_schemes_lose_the_href() {
    for href in [
        "https://example.com/&#10;x",
        "https://exa&#9;mple.com",
        "ht tps://example.com",
        "https ://example.com",
        "mailto:a@b.com&#13;&#10;Bcc:x@y.org",
    ] {
        let html = format!(r#"<a href="{href}">x</a>"#);
        assert_eq!(clean(&html), "<a>x</a>", "{href}");
    }
}

#[test]
fn rawtext_tags_are_dropped_with_their_content() {
    for tag in ["xmp", "noembed", "noframes", "plaintext"] {
        let out = clean(&format!("<p>a</p><{tag}><b>secret</b></{tag}><p>b</p>"));
        assert!(!out.contains("secret"), "{tag}: {out}");
        assert!(out.contains("<p>a</p>"), "{tag}: {out}");
    }
}

#[test]
fn absurd_or_unparseable_lengths_are_dropped() {
    assert_eq!(
        clean(r#"<p style="margin:99999px;color:red">t</p>"#),
        r#"<p style="color:red">t</p>"#
    );
    for decl in [
        "font-size:1e999px",
        "padding:5px 20000px",
        "margin-left:calc(1px + 2px)",
        "font-size:--1px",
        "height:10001px",
    ] {
        assert_eq!(
            clean(&format!(r#"<p style="{decl}">t</p>"#)),
            "<p>t</p>",
            "{decl}"
        );
    }
    let ok = r#"<p style="margin-top:0;padding:10px 2em;font-size:12PT !important">t</p>"#;
    assert_eq!(clean(ok), ok);
}

/// Qt 5's TextEdit draws a box's background from the left edge but its
/// text after the left margin, and reads `auto` as a wide margin; only the
/// top and bottom margins survive.
#[test]
fn only_vertical_margins_are_kept() {
    for (style, kept) in [
        ("margin:0 auto", "margin-top:0;margin-bottom:0"),
        (
            "margin:0px auto !important",
            "margin-top:0px;margin-bottom:0px",
        ),
        ("margin:4px", "margin-top:4px;margin-bottom:4px"),
        ("margin:0 ! important", "margin-top:0;margin-bottom:0"),
        ("margin-top:5px ! important", "margin-top:5px"),
        ("margin:1px 20px 3px", "margin-top:1px;margin-bottom:3px"),
        (
            "margin:1px 20px 3px 40px",
            "margin-top:1px;margin-bottom:3px",
        ),
        ("margin:auto 20px 3px", "margin-bottom:3px"),
        ("margin-top:5px", "margin-top:5px"),
        ("margin-bottom:6px", "margin-bottom:6px"),
    ] {
        assert_eq!(
            clean(&format!(
                r#"<div style="background-color:#eee;{style}">t</div>"#
            )),
            format!(r#"<div style="background-color:#eee;{kept}">t</div>"#),
            "{style}"
        );
    }
    for style in [
        "margin-left:20px",
        "margin-right:2em",
        "margin:auto",
        "margin-top:auto",
    ] {
        assert_eq!(
            clean(&format!(r#"<div style="{style};color:red">t</div>"#)),
            r#"<div style="color:red">t</div>"#,
            "{style}"
        );
    }
}

#[test]
fn a_zero_font_size_becomes_one_pixel() {
    for zero in ["0", "0px", "0pt", "0em", "0.0px", "0px !important"] {
        assert_eq!(
            clean(&format!(r#"<div style="font-size:{zero}">&nbsp;</div>"#)),
            r#"<div style="font-size:1px">&nbsp;</div>"#,
            "{zero}"
        );
    }
}

#[test]
fn a_very_large_font_size_is_clamped() {
    for big in ["40px", "30pt", "3em", "26.5px", "2.5rem"] {
        assert_eq!(
            clean(&format!(r#"<h1 style="font-size:{big}">t</h1>"#)),
            r#"<h1 style="font-size:26px">t</h1>"#,
            "{big}"
        );
    }
    for ok in ["26px", "14px", "12pt", "1.2em", "large", "120%", "0.5px"] {
        let html = format!(r#"<h1 style="font-size:{ok}">t</h1>"#);
        assert_eq!(clean(&html), html, "{ok}");
    }
}
