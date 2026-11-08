//! Images and widths: remote, cid, trackers, scaling, wide tables.

use std::collections::HashMap;

use envio_mime::{RenderOptions, sanitize_html};

fn blocked() -> RenderOptions {
    RenderOptions {
        blocked_label: "[image]".to_owned(),
        max_image_width: 600,
        ..RenderOptions::default()
    }
}

fn allowed() -> RenderOptions {
    RenderOptions {
        allow_remote_images: true,
        ..blocked()
    }
}

fn clean(html: &str) -> String {
    sanitize_html(html, &blocked()).html
}

#[test]
fn blocks_remote_images_by_default() {
    let r = sanitize_html(
        r#"<img src="https://example.com/a.png" alt="A &amp; B"><img src="http://example.com/b.png">"#,
        &blocked(),
    );
    assert_eq!(r.html, "A &amp; B[image]");
    assert_eq!(r.blocked_images, 2);
    assert_eq!(r.removed_trackers, 0);
}

#[test]
fn shows_remote_images_when_allowed() {
    let r = sanitize_html(
        r#"<img src="https://example.com/a.png" width="200" height="100">"#,
        &allowed(),
    );
    assert_eq!(
        r.html,
        r#"<img src="https://example.com/a.png" width="200" height="100">"#
    );
    assert_eq!(r.blocked_images, 0);
}

#[test]
fn removes_tracking_pixels_even_when_allowed() {
    let html = concat!(
        r#"<img src="https://example.com/p.gif" width="1" height="1">"#,
        r#"<img src="https://example.com/q.gif" style="width:0;height:0">"#,
        r#"<img src="https://www.google-analytics.com/collect?v=1">"#,
        r#"<img src="https://mandrillapp.com/track/open.php">"#,
    );
    let r = sanitize_html(html, &allowed());
    assert_eq!(r.html, "");
    assert_eq!(r.removed_trackers, 4);
}

#[test]
fn tracker_hosts_match_whole_labels() {
    let r = sanitize_html(
        r#"<img src="https://notdoubleclick.net/a.png" width="10" height="10">"#,
        &allowed(),
    );
    assert_eq!(r.removed_trackers, 0);
    assert!(r.html.contains("notdoubleclick.net"));
}

#[test]
fn maps_cid_images_and_drops_unknown_ones() {
    let options = RenderOptions {
        cid_urls: HashMap::from([("logo@example.org".to_owned(), "image://envio/3".to_owned())]),
        ..blocked()
    };
    let r = sanitize_html(
        r#"<img src="cid:logo@example.org" alt="Logo"><img src="cid:other@example.org">"#,
        &options,
    );
    assert_eq!(r.html, r#"<img src="image://envio/3" alt="Logo">"#);
    assert_eq!(r.blocked_images, 0);
}

#[test]
fn scales_wide_images_down() {
    let r = sanitize_html(
        r#"<img src="https://example.com/a.png" width="1200" height="800">"#,
        &allowed(),
    );
    assert_eq!(
        r.html,
        r#"<img src="https://example.com/a.png" width="600" height="400">"#
    );
}

#[test]
fn image_size_from_style_becomes_attributes() {
    let r = sanitize_html(
        r#"<img src="https://example.com/a.png" style="width:300px;height:150px">"#,
        &allowed(),
    );
    assert_eq!(
        r.html,
        r#"<img src="https://example.com/a.png" width="300" height="150">"#
    );
}

#[test]
fn drops_wide_table_widths() {
    assert_eq!(
        clean(
            r#"<table width="1200"><tr><td width="700">a</td><td width="100">b</td></tr></table>"#
        ),
        r#"<table><tbody><tr><td>a</td><td width="100">b</td></tr></tbody></table>"#
    );
}

#[test]
fn no_width_limit_when_zero() {
    let options = RenderOptions {
        max_image_width: 0,
        ..allowed()
    };
    let html = r#"<img src="https://example.com/a.png" width="5000">"#;
    assert_eq!(sanitize_html(html, &options).html, html);
}
