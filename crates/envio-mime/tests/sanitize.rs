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
        "</td></tr></table>".repeat(n)
    );
    let out = clean(&html);
    assert_eq!(out.matches("<table>").count(), n);
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
    let ok = r#"<p style="margin:0 auto;padding:10px 2em;font-size:12PT !important">t</p>"#;
    assert_eq!(clean(ok), ok);
}
