//! Headings and size tags (#243, part of #191). Qt 5 sizes `h1`..`h5`,
//! `big` and `small` itself, as a fixed multiple of the document's default
//! font (not of the parent, not compounding), and paints `sub` and `sup` at
//! two thirds of the size they have (QTextFormatPrivate::resolveFont in
//! qtextformat.cpp, QTextEngine::font in qtextengine.cpp, Qt 5.15). The
//! pass models that: `h1` is 32px at a 16px default, so it gets `x-large`
//! (24px); `sub` and `sup` never go under 2px (1px painted).

use envio_mime::{RenderOptions, sanitize_html};

fn clean(html: &str) -> String {
    sanitize_html(html, &RenderOptions::default()).html
}

const H1_FIXED: &str = r#"<h1 style="font-size:x-large">t</h1>"#;

#[test]
fn an_h1_is_brought_under_the_cap() {
    for html in [
        "<h1>t</h1>",
        r#"<h1 style="font-size:150%">t</h1>"#,
        r#"<h1 style="font-size:40px">t</h1>"#,
        r#"<h1 style="font-size:5px">t</h1>"#,
        r#"<h1 style="font-size:xx-large">t</h1>"#,
    ] {
        assert_eq!(clean(html), H1_FIXED, "{html}");
    }
    assert_eq!(
        clean(r#"<h1 style="font-size:40px;color:red">t</h1>"#),
        r#"<h1 style="color:red;font-size:x-large">t</h1>"#
    );
}

#[test]
fn an_h1_with_a_keyword_that_fits_is_kept() {
    for ok in ["small", "medium", "large", "x-large"] {
        let html = format!(r#"<h1 style="font-size:{ok}">t</h1>"#);
        assert_eq!(clean(&html), html, "{ok}");
    }
}

#[test]
fn the_other_headings_already_fit() {
    for tag in ["h2", "h3", "h4", "h5", "h6", "big", "small"] {
        let html = format!("<{tag}>t</{tag}>");
        assert_eq!(clean(&html), html, "{tag}");
    }
}

#[test]
fn a_relative_size_in_a_heading_compounds_from_the_heading() {
    // h1 is 24px after the fix, so 150% of it is 36px.
    assert_eq!(
        clean(r#"<h1 style="font-size:10%"><span style="font-size:150%">t</span></h1>"#),
        r#"<h1 style="font-size:x-large"><span style="font-size:26px">t</span></h1>"#
    );
    // h2 is 24px: 150% of it is 36px, where 150% of 16px would have fitted.
    assert_eq!(
        clean(r#"<h2><em style="font-size:150%">t</em></h2>"#),
        r#"<h2><em style="font-size:26px">t</em></h2>"#
    );
    // And a small enough one stays.
    let html = r#"<h2><em style="font-size:50%">t</em></h2>"#;
    assert_eq!(clean(html), html);
}

#[test]
fn big_does_not_compound_with_big() {
    // Qt gives every `big` 1.2 times the default, however deep (19px).
    let html = "<big><big><big>t</big></big></big>";
    assert_eq!(clean(html), html);
    assert_eq!(
        clean(r#"<big><span style="font-size:150%">t</span></big>"#),
        r#"<big><span style="font-size:26px">t</span></big>"#
    );
    assert_eq!(
        clean(r#"<big><big style="font-size:150%">t</big></big>"#),
        r#"<big><big style="font-size:26px">t</big></big>"#
    );
}

#[test]
fn small_resets_a_tiny_parent() {
    // `small` is 0.8 times the default (13px) whatever its parent: 50% of
    // that is fine, where 50% of a 1px parent would be 1px.
    let html =
        r#"<div style="font-size:1px"><small><span style="font-size:50%">t</span></small></div>"#;
    assert_eq!(clean(html), html);
    let html = r#"<div style="font-size:1px"><h5><span style="font-size:50%">t</span></h5></div>"#;
    assert_eq!(clean(html), html);
}

#[test]
fn sub_and_sup_keep_two_pixels() {
    for tag in ["sub", "sup"] {
        assert_eq!(
            clean(&format!(
                r#"<div style="font-size:1px"><{tag}>t</{tag}></div>"#
            )),
            format!(r#"<div style="font-size:1px"><{tag} style="font-size:2px">t</{tag}></div>"#),
            "{tag}"
        );
        assert_eq!(
            clean(&format!(r#"<{tag} style="font-size:1px">t</{tag}>"#)),
            format!(r#"<{tag} style="font-size:2px">t</{tag}>"#),
            "{tag}"
        );
        assert_eq!(
            clean(&format!(
                r#"<{tag}><span style="font-size:1px">t</span></{tag}>"#
            )),
            format!(r#"<{tag}><span style="font-size:2px">t</span></{tag}>"#),
            "{tag}"
        );
        let html = format!("<{tag}>t</{tag}>");
        assert_eq!(clean(&html), html, "{tag}");
    }
}
