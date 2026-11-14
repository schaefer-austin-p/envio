//! A remote image with no width that would draw wider than its room is
//! capped to the room once the reader has reported its natural size (#170).

use std::collections::HashMap;

use envio_mime::{RenderOptions, sanitize_html};

const BIG: &str = "https://example.org/big.png";
const SMALL: &str = "https://example.org/small.png";
const FLAT: &str = "https://example.org/flat.png";

fn options() -> RenderOptions {
    RenderOptions {
        allow_remote_images: true,
        image_sizes: HashMap::from([
            (BIG.to_owned(), (2000, 1000)),
            (SMALL.to_owned(), (100, 100)),
            (FLAT.to_owned(), (4000, 400)),
        ]),
        blocked_label: "[image]".to_owned(),
        max_image_width: 600,
        ..RenderOptions::default()
    }
}

#[test]
fn caps_a_remote_image_with_no_width_to_the_reader_width() {
    let html = format!(r#"<img src="{BIG}"><img src="{SMALL}">"#);
    let r = sanitize_html(&html, &options());
    assert_eq!(
        r.html,
        format!(r#"<img src="{BIG}" width="600"><img src="{SMALL}">"#)
    );
    assert_eq!(r.remote_images, [BIG, SMALL]);
}

#[test]
fn leaves_an_image_whose_size_is_not_known_yet() {
    let html = r#"<img src="https://example.org/other.png">"#;
    let r = sanitize_html(html, &options());
    assert_eq!(r.html, html);
}

#[test]
fn keeps_a_width_the_message_gave() {
    let html = format!(r#"<img src="{BIG}" width="300">"#);
    let r = sanitize_html(&html, &options());
    assert_eq!(r.html, html);
}

#[test]
fn caps_a_height_only_remote_image_that_would_draw_too_wide() {
    // 4000x400 at height 50 would draw 500 wide: fine at 600, too wide at 360.
    let html = format!(r#"<img src="{FLAT}" height="50">"#);
    let r = sanitize_html(&html, &options());
    assert_eq!(r.html, html);
    let narrow = RenderOptions {
        max_image_width: 360,
        ..options()
    };
    let r = sanitize_html(&html, &narrow);
    assert_eq!(r.html, format!(r#"<img src="{FLAT}" width="360">"#));
}

#[test]
fn caps_a_remote_image_in_a_narrow_cell_to_the_cell_width() {
    let html =
        format!(r#"<table><tr><td width="200"><img src="{BIG}"></td><td>x</td></tr></table>"#);
    let r = sanitize_html(&html, &options());
    assert!(
        r.html
            .contains(&format!(r#"<img src="{BIG}" width="200">"#)),
        "{}",
        r.html
    );
}

#[test]
fn a_blocked_image_stays_a_label() {
    let blocked = RenderOptions {
        allow_remote_images: false,
        ..options()
    };
    let r = sanitize_html(&format!(r#"<img src="{BIG}">"#), &blocked);
    assert_eq!(r.html, "[image]");
}
