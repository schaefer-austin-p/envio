//! The largest font size follows the pane's body width (#192): 26px on a
//! phone, more in the wide pane, never past 40px.

use envio_mime::{RenderOptions, font_cap_px, sanitize_html};

fn clean(html: &str, cap: u32) -> String {
    let options = RenderOptions {
        max_font_px: cap,
        ..RenderOptions::default()
    };
    sanitize_html(html, &options).html
}

const BIG: &str = r#"<div style="font-size:36px">t</div>"#;

#[test]
fn the_default_cap_is_26px() {
    assert_eq!(clean(BIG, 0), r#"<div style="font-size:26px">t</div>"#);
}

#[test]
fn a_wider_cap_keeps_larger_sizes_and_clamps_past_it() {
    assert_eq!(clean(BIG, 36), BIG);
    assert_eq!(
        clean(r#"<div style="font-size:300%">t</div>"#, 36),
        r#"<div style="font-size:36px">t</div>"#
    );
}

#[test]
fn an_h1_keeps_its_own_size_when_the_cap_allows_it() {
    assert_eq!(
        clean("<h1>t</h1>", 0),
        r#"<h1 style="font-size:x-large">t</h1>"#
    );
    assert_eq!(clean("<h1>t</h1>", 36), "<h1>t</h1>");
}

#[test]
fn the_cap_follows_the_body_width() {
    assert_eq!(font_cap_px(0), 26);
    assert_eq!(font_cap_px(320), 26);
    assert_eq!(font_cap_px(360), 26);
    assert_eq!(font_cap_px(500), 36);
    assert_eq!(font_cap_px(616), 40);
    assert_eq!(font_cap_px(5000), 40);
}
