//! A call-to-action link keeps its button look: the `background` shorthand
//! becomes `background-color`, which Qt 5 rich text paints (#178).

use envio_mime::{RenderOptions, sanitize_html};

fn clean(html: &str) -> String {
    sanitize_html(html, &RenderOptions::default()).html
}

#[test]
fn link_background_shorthand_keeps_its_colour() {
    let html = clean(
        r#"<a href="https://shop.example/o/1" style="background:#0a6cff no-repeat;color:#ffffff;font-weight:bold;padding:12px 24px;border:1px solid #0a6cff;border-radius:6px;display:inline-block">View order details</a>"#,
    );
    assert!(html.contains("background-color:#0a6cff"), "{html}");
    assert!(html.contains("color:#ffffff"), "{html}");
    assert!(html.contains("font-weight:bold"), "{html}");
    assert!(!html.contains("border"), "{html}");
}

#[test]
fn shorthand_with_function_colour_in_a_cell_button() {
    let html = clean(
        r#"<table><tr><td style="background:rgb(10, 108, 255);padding:10px 20px;border-radius:4px"><a href="https://shop.example/o/1" style="color:#fff">Pay now</a></td></tr></table>"#,
    );
    assert!(
        html.contains("background-color:rgb(10, 108, 255)"),
        "{html}"
    );
    assert!(html.contains("Pay now"), "{html}");
}

#[test]
fn shorthand_without_a_colour_adds_nothing_and_stays_safe() {
    let html = clean(
        r#"<p style="background:url(javascript:alert(1))">a</p><p style="background:linear-gradient(#000,#fff)">b</p><p style="background:none">c</p>"#,
    );
    assert!(!html.contains("background"), "{html}");
    assert!(!html.contains("javascript"), "{html}");
}
