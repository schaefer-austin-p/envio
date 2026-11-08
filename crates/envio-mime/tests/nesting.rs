//! The nesting guard in front of the sanitizer: it must catch deep
//! nesting however it is spelled, spare ordinary sloppy HTML, and never
//! panic on cut-off input.

use envio_mime::{RenderOptions, sanitize_html};

fn render(html: &str) -> String {
    sanitize_html(html, &RenderOptions::default()).html
}

/// The plain-text fallback wraps the text in this block.
fn fell_back(html: &str) -> bool {
    render(html).starts_with(r#"<div style="white-space:pre-wrap">"#)
}

#[test]
fn stray_end_tags_do_not_hide_nesting() {
    for html in [
        "<div></i>".repeat(10_000),
        "<div></p>".repeat(10_000),
        "<div></span></b>".repeat(10_000),
    ] {
        assert!(fell_back(&html), "{}", &html[..20]);
    }
}

#[test]
fn unclosed_optional_end_tags_are_not_nesting() {
    let paragraphs = "<p>para <b>x</b><br>".repeat(300);
    let items = format!("<ul>{}</ul>", "<li>item".repeat(300));
    let rows = format!("<table>{}</table>", "<tr><td>a<td>b".repeat(300));
    let defs = format!("<dl>{}</dl>", "<dt>t<dd>d".repeat(300));
    for html in [paragraphs, items, rows, defs] {
        assert!(!fell_back(&html), "{}", &html[..30]);
    }
    assert!(render(&"<p>para <b>x</b><br>".repeat(300)).contains("<b>x</b>"));
}

#[test]
fn deep_nesting_is_still_caught() {
    let n = 10_000;
    let html = format!("{}x{}", "<div>".repeat(n), "</div>".repeat(n));
    assert!(fell_back(&html));
    assert!(fell_back(&"<div>".repeat(300)));
    assert!(fell_back(&format!(
        "<script>a</script>{}",
        "<span>".repeat(300)
    )));
}

#[test]
fn closed_elements_and_raw_text_do_not_count() {
    let closed = "<div>a</div>".repeat(1_000);
    assert!(!fell_back(&closed));
    let scripts =
        "<script>if (a < b) { x = '<div>'.repeat(9) }</script>".to_owned() + &"<div>".repeat(100);
    assert!(!fell_back(&scripts));
    let fake = format!("<style>{}</style><p>ok</p>", "<div>".repeat(1_000));
    assert!(!fell_back(&fake));
}

#[test]
fn quoted_angle_brackets_do_not_confuse_the_scan() {
    let html = r#"<a title="<div><div>" href='>'>x</a>"#.repeat(500);
    assert!(!fell_back(&html));
}

#[test]
fn cut_off_input_never_panics() {
    let sample = "<p>hi</p><!-- c > x --><!doctype html><?php x ?><a href=\"a>b\" title='>'>é</a>\
                  <br><img src=x/><div class=a\n id=\"b\"><script>1<2</script><textarea>t</textarea>\
                  </div></ZZ><b";
    for (end, _) in sample.char_indices().chain([(sample.len(), ' ')]) {
        let _ = render(&sample[..end]);
    }
    for html in [
        "<",
        "<!",
        "<!--",
        "<?",
        "</",
        "<p>hi</p><!--",
        "<a b='",
        "<script>",
    ] {
        let _ = render(html);
    }
}

#[test]
fn nesting_the_parser_keeps_is_counted() {
    let n = 10_000;
    for html in [
        "<ul><li>".repeat(n),
        "<dl><dd>".repeat(n),
        "<ul><p>".repeat(n),
        "<b><div></b>".repeat(n),
        "<a href=x><div></a>".repeat(n),
        format!("<div =\">{}", "<div>".repeat(n)),
        format!("<div a='{}", "<div>".repeat(n)),
        format!("<!-- never closed {}", "<div>".repeat(n)),
        format!("<title>{}", "<div>".repeat(n)),
        "<i>".repeat(60_000),
    ] {
        assert!(fell_back(&html), "{}", &html[..20]);
    }
}

#[test]
fn realistic_newsletter_shape_goes_through_ammonia() {
    let mut html = String::new();
    for level in 0..40 {
        // Every fourth level misnests its formatting tags.
        let format = if level % 4 == 0 {
            "<b><i>x</b></i>"
        } else {
            "<b>x</b>"
        };
        html.push_str(&format!(
            "<table><tr><td>cell<p>para<br>{format}<tr><td>more<td>cells "
        ));
    }
    for _ in 0..40 {
        html.push_str("</table>");
    }
    assert!(!fell_back(&html));
    assert!(render(&html).contains("<table>"));
    let sloppy = "<b><i>x</b></i> <p>y".repeat(40);
    assert!(!fell_back(&sloppy));
}
