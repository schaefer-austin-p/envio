//! What stands in for an image that is not shown (#161): nothing for a
//! small, decorative one; for a large one its alt text or the caller's
//! label, never underlined.

use envio_mime::{RenderOptions, sanitize_html};

fn blocked() -> RenderOptions {
    RenderOptions {
        blocked_label: "[image]".to_owned(),
        max_image_width: 600,
        max_font_px: 0,
        ..RenderOptions::default()
    }
}

fn clean(html: &str) -> String {
    sanitize_html(html, &blocked()).html
}

/// A label as the reader shows it.
fn label(text: &str) -> String {
    format!(r#"<span style="text-decoration:none">{text}</span>"#)
}

#[test]
fn a_small_image_that_is_not_shown_leaves_nothing() {
    for img in [
        r#"<img src="https://example.com/i.png" alt="info icon" width="40" height="40">"#,
        r#"<img src="https://example.com/i.png" alt="info icon" width="48">"#,
        r#"<img src="https://example.com/i.png" alt="info icon" height="48">"#,
        r#"<img src="https://example.com/i.png" alt="info icon" width="16" height="16">"#,
        r#"<img src="https://example.com/i.png" alt="info icon" style="width:24px">"#,
        r#"<img src="https://example.com/i.png" alt="info icon" height="24">"#,
        r#"<img src="https://example.com/i.png" width="32" height="32">"#,
        r#"<img src="cid:gone@example.org" alt="Logo" width="40">"#,
    ] {
        assert_eq!(clean(img), "", "{img}");
    }
}

#[test]
fn a_side_declared_in_another_unit_counts_as_large() {
    // Only pixel lengths can make an image small: a percentage, `auto` or
    // junk beside a small pixel side may be a wide banner.
    for img in [
        r#"<img src="https://example.com/b.png" alt="Autumn sale" width="100%" height="40">"#,
        r#"<img src="https://example.com/b.png" alt="Autumn sale" style="width:100%;height:40px">"#,
        r#"<img src="https://example.com/b.png" alt="Autumn sale" width="auto" height="30">"#,
        r#"<img src="https://example.com/b.png" alt="Autumn sale" style="width:auto;height:30px">"#,
        r#"<img src="https://example.com/b.png" alt="Autumn sale" width="wide" height="30">"#,
        r#"<img src="https://example.com/b.png" alt="Autumn sale" width="40" height="10em">"#,
        r#"<img src="https://example.com/b.png" alt="Autumn sale" style="width:40px;height:50%">"#,
    ] {
        assert_eq!(clean(img), label("Autumn sale"), "{img}");
    }
}

#[test]
fn a_zero_side_is_a_tracker_and_leaves_nothing() {
    for img in [
        r#"<img src="https://example.com/b.png" alt="x" width="0" height="40">"#,
        r#"<img src="https://example.com/b.png" alt="x" style="width:0;height:40px">"#,
    ] {
        assert_eq!(clean(img), "", "{img}");
    }
}

#[test]
fn a_small_blocked_image_still_counts_as_blocked() {
    let r = sanitize_html(
        r#"<img src="https://example.com/i.png" alt="x" width="40">"#,
        &blocked(),
    );
    assert_eq!(r.blocked_images, 1);
}

#[test]
fn a_large_or_unsized_image_shows_its_label_not_underlined() {
    for (img, text) in [
        (
            r#"<img src="https://example.com/h.png" alt="Autumn routes" width="600" height="120">"#,
            "Autumn routes",
        ),
        (
            r#"<img src="https://example.com/h.png" alt="Hero" width="49">"#,
            "Hero",
        ),
        // Only one side is small: a banner or a logo, not an icon.
        (
            r#"<img src="https://example.com/b.png" alt="Autumn sale" width="600" height="40">"#,
            "Autumn sale",
        ),
        (
            r#"<img src="https://example.com/l.png" alt="Acme" width="180" height="40">"#,
            "Acme",
        ),
        (
            r#"<img src="https://example.com/l.png" alt="Acme" style="width:40px;height:120px">"#,
            "Acme",
        ),
        (
            r#"<img src="https://example.com/h.png" alt=" Hero ">"#,
            "Hero",
        ),
        (
            r#"<img src="https://example.com/h.png" width="300">"#,
            "[image]",
        ),
        (r#"<img src="cid:gone@example.org">"#, "[image]"),
    ] {
        assert_eq!(clean(img), label(text), "{img}");
    }
}

/// An image-only call to action: a 200x44 button picture in a link.
#[test]
fn a_linked_label_is_escaped_and_not_underlined() {
    let html = clean(
        r#"<a href="https://example.com/"><img src="https://example.com/h.png" alt="Read &lt;more&gt;" width="200" height="44"></a>"#,
    );
    assert_eq!(
        html,
        format!(
            r#"<a href="https://example.com/">{}</a>"#,
            label("Read &lt;more&gt;")
        )
    );
}

#[test]
fn a_label_does_not_make_its_cell_text() {
    // A cell holding only a blocked 100 px picture is 100 px wide, a narrow
    // side column that stays beside the text; the label's span and text
    // must not count as the cell's own text (#160).
    let html = clean(
        r#"<table><tr><td><img src="https://example.com/a.png" alt="Avatar" width="100" height="80"></td><td><p>Ada commented</p></td></tr></table>"#,
    );
    assert!(html.contains("<table"), "{html}");
    assert!(html.contains(&label("Avatar")), "{html}");
}
