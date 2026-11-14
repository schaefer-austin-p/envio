//! A cid image with no width is capped to the reader width by its natural
//! width.

use std::collections::HashMap;

use envio_mime::{RenderOptions, sanitize_html};

#[test]
fn caps_a_cid_image_with_no_width_to_the_reader_width() {
    let options = RenderOptions {
        cid_urls: HashMap::from([
            ("big@example.org".to_owned(), "file:///c/big.png".to_owned()),
            ("small@example.org".to_owned(), "file:///c/s.png".to_owned()),
        ]),
        cid_widths: HashMap::from([
            ("big@example.org".to_owned(), 2000),
            ("small@example.org".to_owned(), 100),
        ]),
        blocked_label: "[image]".to_owned(),
        max_image_width: 600,
        ..RenderOptions::default()
    };
    let r = sanitize_html(
        r#"<img src="cid:big@example.org"><img src="cid:small@example.org">"#,
        &options,
    );
    assert_eq!(
        r.html,
        r#"<img src="file:///c/big.png" width="600"><img src="file:///c/s.png">"#
    );
}
