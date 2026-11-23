//! A cid image that would draw wider than its room is capped to the room,
//! by its natural size.

use std::collections::HashMap;

use envio_mime::{RenderOptions, sanitize_html};

fn options() -> RenderOptions {
    RenderOptions {
        cid_urls: HashMap::from([
            ("big@example.org".to_owned(), "file:///c/big.png".to_owned()),
            ("small@example.org".to_owned(), "file:///c/s.png".to_owned()),
            ("flat@example.org".to_owned(), "file:///c/f.png".to_owned()),
        ]),
        cid_sizes: HashMap::from([
            ("big@example.org".to_owned(), (2000, 1000)),
            ("small@example.org".to_owned(), (100, 100)),
            ("flat@example.org".to_owned(), (4000, 400)),
        ]),
        blocked_label: "[image]".to_owned(),
        max_image_width: 600,
        max_font_px: 0,
        ..RenderOptions::default()
    }
}

#[test]
fn caps_a_cid_image_with_no_width_to_the_reader_width() {
    let r = sanitize_html(
        r#"<img src="cid:big@example.org"><img src="cid:small@example.org">"#,
        &options(),
    );
    assert_eq!(
        r.html,
        r#"<img src="file:///c/big.png" width="600"><img src="file:///c/s.png">"#
    );
}

#[test]
fn caps_a_height_only_cid_image_that_would_draw_too_wide() {
    // 4000x400 at height 50 would draw 500 wide: fine at 600, too wide at 360.
    let html = r#"<img src="cid:flat@example.org" height="50">"#;
    let r = sanitize_html(html, &options());
    assert_eq!(r.html, r#"<img src="file:///c/f.png" height="50">"#);
    let narrow = RenderOptions {
        max_image_width: 360,
        max_font_px: 0,
        ..options()
    };
    let r = sanitize_html(html, &narrow);
    assert_eq!(r.html, r#"<img src="file:///c/f.png" width="360">"#);
}

#[test]
fn leaves_a_narrow_height_only_cid_image_alone() {
    let r = sanitize_html(
        r#"<img src="cid:small@example.org" height="80">"#,
        &options(),
    );
    assert_eq!(r.html, r#"<img src="file:///c/s.png" height="80">"#);
}

#[test]
fn caps_a_cid_image_in_a_narrow_cell_to_the_cell_width() {
    // The cell is 200 wide, the reader 600, the image 2000: capped at 200.
    let r = sanitize_html(
        r#"<table><tr><td width="200"><img src="cid:big@example.org"></td><td>x</td></tr></table>"#,
        &options(),
    );
    assert!(
        r.html
            .contains(r#"<img src="file:///c/big.png" width="200">"#),
        "{}",
        r.html
    );
}
