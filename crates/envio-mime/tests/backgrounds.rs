//! A block container's background reaches every block inside it: Qt 5
//! paints it only behind the first one (#161).

use envio_mime::{RenderOptions, sanitize_html};

fn clean(html: &str) -> String {
    let options = RenderOptions {
        max_image_width: 360,
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
