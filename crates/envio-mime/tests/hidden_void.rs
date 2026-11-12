//! A hidden void element (no end tag) removes only itself.

use envio_mime::{RenderOptions, sanitize_html};

fn clean(html: &str) -> String {
    sanitize_html(html, &RenderOptions::default()).html
}

#[test]
fn hidden_input_keeps_the_rest() {
    let out = clean(r#"<p>a</p><input style="display:none"><p>after</p>"#);
    assert_eq!(out, "<p>a</p><p>after</p>");
}

#[test]
fn hidden_col_keeps_the_rest() {
    let out = clean(
        r#"<table><colgroup><col style="visibility:hidden"></colgroup><tr><td>x</td><td>y</td></tr></table><p>after</p>"#,
    );
    assert!(
        out.contains("<td>x</td>") && out.ends_with("<p>after</p>"),
        "{out}"
    );
}

#[test]
fn hidden_wbr_keeps_the_rest() {
    let out = clean(r#"<p>a<wbr style="display:none">b</p><p>after</p>"#);
    assert_eq!(out, "<p>ab</p><p>after</p>");
}

#[test]
fn every_html_void_element_is_safe() {
    for tag in [
        "area", "base", "br", "col", "embed", "hr", "img", "input", "keygen", "link", "meta",
        "param", "source", "track", "wbr",
    ] {
        let out = clean(&format!(
            r#"<p>a</p><{tag} style="display:none"><p>after</p>"#
        ));
        assert!(out.ends_with("<p>after</p>"), "{tag}: {out}");
    }
}
