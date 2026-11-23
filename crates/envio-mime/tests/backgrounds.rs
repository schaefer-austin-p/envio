//! A block container's background reaches every block inside it: Qt 5
//! paints it only behind the first one (#161).

use envio_mime::{RenderOptions, sanitize_html};

fn clean(html: &str) -> String {
    let options = RenderOptions {
        max_image_width: 360,
        max_font_px: 0,
        ..RenderOptions::default()
    };
    sanitize_html(html, &options).html
}

fn count(html: &str, needle: &str) -> usize {
    html.matches(needle).count()
}

#[test]
fn every_paragraph_in_a_dark_div_gets_its_background() {
    let paragraphs: String = (0..10).map(|i| format!("<p>line {i}</p>")).collect();
    let html = clean(&format!(
        r#"<div style="background-color:#222;color:#fff">{paragraphs}</div>"#
    ));
    assert_eq!(
        count(&html, r#"<p style="background-color:#222222">"#),
        10,
        "{html}"
    );
}

#[test]
fn a_child_keeps_its_own_background() {
    let html = clean(
        r#"<div style="background-color:#222"><p>a</p><p style="background-color:white;color:red">b</p></div>"#,
    );
    assert!(
        html.contains(r#"<p style="background-color:#222222">a"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<p style="background-color:white;color:red">b"#),
        "{html}"
    );
}

#[test]
fn nested_containers_pass_on_the_nearest_background() {
    let html = clean(
        r#"<div style="background-color:#222"><div style="background-color:#eee"><p>inner</p></div><p>outer</p></div>"#,
    );
    assert!(
        html.contains(r#"<p style="background-color:#eeeeee">inner"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<p style="background-color:#222222">outer"#),
        "{html}"
    );
}

#[test]
fn deeper_blocks_inherit_through_inline_elements_and_lists() {
    let html = clean(
        r#"<div style="background-color:#000"><span><b><div>deep</div></b></span><ul><li>one</li><li>two</li></ul></div>"#,
    );
    assert!(
        html.contains(r#"<div style="background-color:#000000">deep"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<ul style="background-color:#000000">"#),
        "{html}"
    );
    assert_eq!(
        count(&html, r#"<li style="background-color:#000000">"#),
        2,
        "{html}"
    );
    // Inline elements are left alone.
    assert!(html.contains("<span><b>"), "{html}");
}

#[test]
fn a_child_style_keeps_its_other_declarations_after_the_background() {
    let html = clean(r#"<div style="background-color:navy"><p style="color:#fff">x</p></div>"#);
    assert!(
        html.contains(r#"<p style="background-color:#000080;color:#fff">x"#),
        "{html}"
    );
}

#[test]
fn a_transparent_child_background_is_replaced() {
    let html = clean(
        r#"<div style="background-color:#222"><p style="color:#fff;background-color:transparent">x</p><p style="background-color:rgba(0,0,0,0)">y</p></div>"#,
    );
    assert!(
        html.contains(r#"<p style="background-color:#222222;color:#fff">x"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<p style="background-color:#222222">y"#),
        "{html}"
    );
}

#[test]
fn a_linearised_cell_background_reaches_its_blocks() {
    let html =
        clean(r##"<table><tr><td bgcolor="#1a1f36"><p>one</p><p>two</p></td></tr></table>"##);
    assert_eq!(
        count(&html, r#"<p style="background-color:#1a1f36">"#),
        2,
        "{html}"
    );
}

#[test]
fn a_data_table_cell_inherits_from_its_container_unless_it_has_its_own() {
    let html = clean(
        r##"<div style="background-color:#222"><table><tr><th>k</th><td>a</td><td bgcolor="#fff">b</td></tr></table></div>"##,
    );
    assert!(
        html.contains(r#"<th style="background-color:#222222">k"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<td style="background-color:#222222">a"#),
        "{html}"
    );
    assert!(html.contains(r##"<td bgcolor="#fff">b"##), "{html}");
}

#[test]
fn only_an_opaque_colour_is_pushed_down() {
    for bg in [
        "rgba(0,0,0,0.5)",
        "transparent",
        "inherit",
        "#12345",
        "currentcolor",
    ] {
        let html = clean(&format!(
            r#"<div style="background-color:{bg}"><p>a</p><p>b</p></div>"#
        ));
        assert_eq!(count(&html, "<p>"), 2, "{bg}: {html}");
    }
}

#[test]
fn the_pushed_colour_is_always_a_normalised_hex() {
    let html = clean(
        r#"<div style="background-color:hsl(0, 100%, 50%) !important"><p>a</p></div><div style="background-color:RGB(1,2,3)"><p>b</p></div>"#,
    );
    assert!(
        html.contains(r#"<p style="background-color:#ff0000">a"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<p style="background-color:#010203">b"#),
        "{html}"
    );
}

#[test]
fn without_a_background_nothing_changes() {
    let input = r#"<div style="color:#fff"><p>a</p><p>b</p></div>"#;
    assert_eq!(clean(input), input);
}

/// Every `<p` tag in `html`: bare, or with only a pushed `#rrggbb`.
fn assert_paragraphs_safe(html: &str, input: &str) {
    for (at, _) in html.match_indices("<p") {
        let tag = &html[at..at + html[at..].find('>').unwrap() + 1];
        if tag == "<p>" {
            continue;
        }
        let hex = tag
            .strip_prefix(r#"<p style="background-color:#"#)
            .and_then(|rest| rest.strip_suffix(r#"">"#))
            .unwrap_or_else(|| panic!("{tag} from {input}"));
        assert!(
            hex.len() == 6 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
            "{tag} from {input}"
        );
    }
}

const HOSTILE: &[&str] = &[
    "#222&quot;><script>x()</script>",
    "#222\" onclick=\"x()",
    "#222\"",
    "#222>",
    "#222'",
    "url(https://x.example/a.png)",
    "#222 url(x)",
    "expression(alert(1))",
    "#222;color:red",
    "red;position:fixed",
    "rgb(1,2,3);color:red",
    "#222 !important; color:red",
    "#22222g",
    "\\23 222",
];

#[test]
fn a_hostile_background_value_pushes_nothing_or_a_normalised_hex() {
    for value in HOSTILE {
        let escaped = value.replace('"', "&quot;");
        for input in [
            format!(r#"<div style="background-color:{escaped}"><p>a</p><p>b</p></div>"#),
            format!(r#"<table><tr><td bgcolor="{escaped}"><p>a</p><p>b</p></td></tr></table>"#),
            format!(
                r#"<table><tr><th>k</th><td bgcolor="{escaped}"><p>a</p><p>b</p></td></tr></table>"#
            ),
            format!(r#"<table bgcolor="{escaped}"><tr><th>k</th><td><p>a</p></td></tr></table>"#),
        ] {
            let html = clean(&input);
            assert_paragraphs_safe(&html, &input);
            // The container's own attribute is the sanitiser's (escaped,
            // inert); what this pass writes is checked above.
            assert!(!html.contains("<script"), "{html} from {input}");
        }
    }
}

#[test]
fn deep_nesting_near_the_limit_finishes_with_one_background_per_block() {
    let depth = 250;
    let input = format!(
        r#"<div style="background-color:#000">{}text{}</div>"#,
        "<div>".repeat(depth),
        "</div>".repeat(depth)
    );
    let html = clean(&input);
    assert_eq!(
        count(&html, r#"<div style="background-color:#000000">"#),
        depth,
        "{html}"
    );
    assert!(html.contains("text"));
    assert!(html.len() < input.len() * 10, "{} bytes", html.len());
}
