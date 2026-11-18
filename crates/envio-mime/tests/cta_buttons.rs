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
    assert!(html.contains("background-color:#0a6cff"), "{html}");
    assert!(html.contains("Pay now"), "{html}");
}

fn style_of(value: &str) -> String {
    clean(&format!(r#"<p style="{value}">x</p>"#))
}

#[test]
fn unsafe_parts_drop_the_whole_shorthand() {
    for value in [
        "background:#fff url(http://tracker/x.png)",
        "background:red expression(alert(1))",
        "background:red var(--x)",
        "background:r\\65d",
        "background:#fff \\75rl(x)",
        "background:red /* x */",
        "background:red @import",
        "background:red javascript:alert(1)",
        "background:red env(x)",
        "background:url(javascript:alert(1))",
    ] {
        let html = style_of(value);
        assert!(!html.contains("background"), "{value}: {html}");
        assert!(!html.contains("tracker"), "{value}: {html}");
    }
}

#[test]
fn accepted_forms_are_written_normalised() {
    for (value, want) in [
        ("background:#0A6CFF", "background-color:#0a6cff"),
        ("background:RED", "background-color:#ff0000"),
        ("background:#fff !important", "background-color:#ffffff"),
        ("background:#fff!important", "background-color:#ffffff"),
        ("background:RGB(10, 108, 255)", "background-color:#0a6cff"),
        ("background:#111 #eee", "background-color:#111111"),
        (
            "background:no-repeat 50% 0 blue",
            "background-color:#0000ff",
        ),
        ("background:transparent", "background-color:transparent"),
        (
            "background-color:red;background:blue",
            "background-color:red;background-color:#0000ff",
        ),
    ] {
        let html = style_of(value);
        assert!(
            html.contains(&format!(r#"style="{want}""#)),
            "{value}: {html}"
        );
    }
}

#[test]
fn shorthand_without_a_colour_resets_the_background() {
    for value in [
        "background:none",
        "background:linear-gradient(#000,#fff)",
        "background:no-repeat",
    ] {
        let html = style_of(&format!("background-color:red;{value}"));
        assert!(
            html.contains(r#"style="background-color:red;background-color:transparent""#),
            "{value}: {html}"
        );
    }
}
