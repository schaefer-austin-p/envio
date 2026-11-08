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
        "<i>".repeat(60_000),
        format!("<!x {}", "<div>".repeat(n)),
        format!("<?x {}", "<div>".repeat(n)),
    ] {
        assert!(fell_back(&html), "{}", &html[..20]);
    }
}

/// An attribute, comment or title that runs to the end swallows the tags
/// after it: html5ever builds no nesting from them, so neither does the
/// guard, and the sanitizer gets nothing deep.
#[test]
fn constructs_cut_off_at_the_end_hide_their_tags() {
    let divs = "<div>".repeat(10_000);
    for html in [
        format!("<div a='{divs}"),
        format!("<!-- never closed {divs}"),
        format!("<title>{divs}"),
    ] {
        assert!(!fell_back(&html), "{}", &html[..20]);
        assert!(!render(&html).contains("<div"), "{}", &html[..20]);
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

#[test]
fn depth_limit_is_exactly_256() {
    assert!(!fell_back(&"<div>".repeat(256)));
    assert!(fell_back(&"<div>".repeat(257)));
}

#[test]
fn start_tag_backstop_counts_closed_elements() {
    assert!(!fell_back(&"<span></span>".repeat(1_000)));
    assert!(fell_back(&"<span></span>".repeat(50_001)));
}

/// The repros of the final review: each one nests deeply in html5ever.
#[test]
fn parser_rules_the_old_scan_got_wrong() {
    let n = 10_000;
    let divs = "<div>".repeat(n);
    for (finding, html) in [
        ("1 ruby", "<rb>".repeat(n)),
        ("2 optgroup", "<optgroup><option>".repeat(n)),
        ("3 svg cells", format!("<svg>{}", "<td>".repeat(n))),
        (
            "3 svg style",
            format!("<svg><style>{}</style>", "<g>".repeat(n)),
        ),
        ("4 name", "<div></div-x>".repeat(n)),
        ("4 name quote", format!("<div-a=\"x>{divs}<p title=\"y\">")),
        ("5 empty comment", format!("<!-->{divs}-->")),
        ("5 dash comment", format!("<!--->{divs}-->")),
        ("5 bang comment", format!("<!-- a --!>{divs}-->")),
        ("6 end tag attribute", "<div></x a=\"></div>\">".repeat(n)),
        (
            "6 cdata",
            format!("<svg>{}", "<g><![CDATA[></g>]]>".repeat(n)),
        ),
        ("8 formatting end", "<ul><b><li></b>".repeat(2_000)),
    ] {
        assert!(fell_back(&html), "finding {finding}");
    }
}

#[test]
fn unterminated_comments_scan_in_linear_time() {
    let html = "<!-- >".repeat(170_000);
    let _ = render(&html);
}

#[test]
fn inline_svg_with_many_self_closed_paths_goes_through() {
    let html = format!(
        "<p>logo</p><svg viewBox=\"0 0 10 10\">{}</svg><p>after</p>",
        "<path d=\"M0 0L1 1\"/>".repeat(400)
    );
    assert!(!fell_back(&html));
}

#[test]
fn deep_sloppy_tables_go_through() {
    let mut html = String::new();
    for level in 0..40 {
        // Every other level misnests its formatting tags: 20 in all.
        let format = if level % 2 == 0 {
            "<b><i>x</b></i>"
        } else {
            "<b>x</b>"
        };
        html.push_str(&format!(
            "<table><tr><td>cell<p>para<br>{format}<tr><td>more<td>cells "
        ));
    }
    assert!(!fell_back(&html));
}
