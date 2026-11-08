//! Elements hidden with CSS are removed with their content.

use envio_mime::{RenderOptions, sanitize_html};

fn clean(html: &str) -> String {
    sanitize_html(html, &RenderOptions::default()).html
}

#[test]
fn display_none_preheader_is_removed() {
    let out = clean(r#"<div style="display:none;max-height:0">PREHEADER JUNK</div><p>real</p>"#);
    assert!(out.contains("real"), "{out}");
    assert!(!out.contains("PREHEADER"), "{out}");
}

#[test]
fn display_none_in_any_spelling() {
    for style in [
        "DISPLAY:NONE",
        "display : none",
        "display:none !important",
        "color:red; Display: None ! IMPORTANT",
        "display:none;display:block",
    ] {
        let out = clean(&format!(r#"<div style="{style}">HIDDEN</div><p>real</p>"#));
        assert_eq!(out, "<p>real</p>", "{style}");
    }
}

#[test]
fn hidden_element_takes_nested_content_and_same_name_children() {
    let out = clean(
        r#"<p>a</p><div style="display:none"><div>x <b>HIDDEN</b></div><p>more</p></div><p>b</p>"#,
    );
    assert_eq!(out, "<p>a</p><p>b</p>");
}

#[test]
fn hidden_table_and_void_elements_are_removed() {
    let out = clean(
        r#"a<table style="display:none"><tr><td>HIDDEN</td></tr></table>b<br style="display:none">c<img src="cid:x" style="display:none">d"#,
    );
    assert_eq!(out, "abcd");
}

#[test]
fn visible_displays_are_not_removed() {
    let out =
        clean(r#"<div style="display:block">shown</div><p style="display:nonexistent">x</p>"#);
    assert!(out.contains("shown") && out.contains('x'), "{out}");
}

#[test]
fn visibility_hidden_is_removed_too() {
    let out = clean(r#"<span style="visibility:hidden">HIDDEN</span><p>real</p>"#);
    assert_eq!(out, "<p>real</p>");
}
