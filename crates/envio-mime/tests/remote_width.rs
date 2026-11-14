//! A remote image with no width that would draw wider than its room is
//! capped to the room once the reader has reported its natural size (#170).

use std::collections::HashMap;

use envio_mime::{RenderOptions, Unsized, sanitize_html};

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
    assert_eq!(
        r.html,
        r#"<span style="text-decoration:none">[image]</span>"#
    );
}

fn with_size(size: (u32, u32)) -> RenderOptions {
    RenderOptions {
        image_sizes: HashMap::from([(BIG.to_owned(), size)]),
        ..options()
    }
}

#[test]
fn odd_reported_sizes_are_safe() {
    let bare = format!(r#"<img src="{BIG}">"#);
    let tall = format!(r#"<img src="{BIG}" height="50">"#);
    // Nothing to draw: left as it is.
    for size in [(0, 0), (0, 400)] {
        assert_eq!(
            sanitize_html(&bare, &with_size(size)).html,
            bare,
            "{size:?}"
        );
        assert_eq!(
            sanitize_html(&tall, &with_size(size)).html,
            tall,
            "{size:?}"
        );
    }
    // No natural height: the height can't scale the width, so only a bare
    // image is capped.
    assert_eq!(sanitize_html(&tall, &with_size((2000, 0))).html, tall);
    let capped = format!(r#"<img src="{BIG}" width="600">"#);
    assert_eq!(sanitize_html(&bare, &with_size((2000, 0))).html, capped);
    // The largest sizes don't overflow.
    for size in [(u32::MAX, u32::MAX), (u32::MAX, 1), (u32::MAX, 0)] {
        assert_eq!(
            sanitize_html(&bare, &with_size(size)).html,
            capped,
            "{size:?}"
        );
    }
    assert_eq!(sanitize_html(&tall, &with_size((u32::MAX, 1))).html, capped);
}

#[test]
fn a_url_with_quotes_and_brackets_only_gains_a_width() {
    let html = r#"<img src="https://example.org/a&quot;b&lt;c&gt;.png">"#;
    let plain = sanitize_html(html, &options());
    let url = plain.remote_images[0].clone();
    // A key that names nothing in the message changes nothing.
    let other = RenderOptions {
        image_sizes: HashMap::from([(r#""><script>x</script>"#.to_owned(), (2000, 10))]),
        ..options()
    };
    assert_eq!(sanitize_html(html, &other).html, plain.html);
    let sized = RenderOptions {
        image_sizes: HashMap::from([(url, (2000, 10))]),
        ..options()
    };
    let capped = sanitize_html(html, &sized).html;
    assert_eq!(capped, plain.html.replace('>', r#" width="600">"#));
    assert!(!capped.contains("<c>"), "{capped}");
}

#[test]
fn lists_the_remote_images_a_size_could_still_cap() {
    let html = format!(
        "<img src=\"{BIG}\"><img src=\"{SMALL}\" width=\"50\"><img src=\"{FLAT}\" height=\"50\">\
         <table><tr><td width=\"200\"><img src=\"https://example.org/cell.png\"></td>\
         <td>x</td></tr></table>"
    );
    let none = RenderOptions {
        image_sizes: HashMap::new(),
        ..options()
    };
    let r = sanitize_html(&html, &none);
    let open: Vec<(&str, u32, Option<u32>)> = r
        .unsized_images
        .iter()
        .map(|u| (u.url.as_str(), u.room, u.height))
        .collect();
    assert_eq!(
        open,
        [
            (BIG, 600, None),
            (FLAT, 600, Some(50)),
            ("https://example.org/cell.png", 200, None),
        ]
    );
    // Once capped, an image is no longer listed.
    let r = sanitize_html(&html, &options());
    assert!(r.unsized_images.iter().all(|u| u.url != BIG));
}

#[test]
fn caps_says_whether_a_size_would_draw_too_wide() {
    let bare = Unsized {
        url: BIG.to_owned(),
        room: 600,
        height: None,
    };
    assert!(bare.caps((601, 10)));
    assert!(!bare.caps((600, 10)));
    assert!(!bare.caps((0, 0)));
    assert!(bare.caps((u32::MAX, 0)));
    let tall = Unsized {
        height: Some(50),
        ..bare.clone()
    };
    assert!(tall.caps((4000, 100)));
    assert!(!tall.caps((4000, 400)));
    assert!(!tall.caps((4000, 0)));
    let no_room = Unsized { room: 0, ..bare };
    assert!(!no_room.caps((u32::MAX, 1)));
}
