//! Every corpus and attack file goes through every entry point without a
//! panic, as mail and as HTML.

use std::path::Path;

use envio_mime::{RenderOptions, envelope, parse, render, sanitize_html};

#[test]
fn every_fixture_survives_every_entry_point() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut count = 0;
    for dir in ["corpus", "attacks"] {
        for entry in std::fs::read_dir(root.join(dir)).unwrap() {
            let raw = std::fs::read(entry.unwrap().path()).unwrap();
            let _ = envelope(&raw);
            for allow_remote_images in [false, true] {
                let options = RenderOptions {
                    allow_remote_images,
                    max_image_width: 600,
                    max_font_px: 0,
                    ..RenderOptions::default()
                };
                let _ = render(&parse(&raw), &options);
                let _ = sanitize_html(&String::from_utf8_lossy(&raw), &options);
            }
            count += 1;
        }
    }
    assert!(count >= 24, "fixtures missing: {count}");
}
