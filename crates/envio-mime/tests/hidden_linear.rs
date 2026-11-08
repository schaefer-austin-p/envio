//! Removing hidden elements is linear in the input, also for void tags and
//! for tags that never close.

use std::time::{Duration, Instant};

use envio_mime::{RenderOptions, sanitize_html};

fn timed(html: &str) -> (String, Duration) {
    let start = Instant::now();
    let out = sanitize_html(html, &RenderOptions::default()).html;
    (out, start.elapsed())
}

const N: usize = 20_000;
const BOUND: Duration = Duration::from_secs(1);

#[test]
fn hidden_void_tags_stay_linear() {
    for tag in ["bgsound", "basefont", "frame", "input", "wbr"] {
        let html = format!(
            r#"{}<p>after</p>"#,
            format!(r#"<{tag} style="display:none">"#).repeat(N)
        );
        let (out, took) = timed(&html);
        assert_eq!(out, "<p>after</p>", "{tag}");
        assert!(took < BOUND, "{tag}: {took:?}");
    }
}

#[test]
fn many_hidden_elements_stay_linear() {
    let html = format!(
        "{}<p>after</p>",
        r#"<span style="display:none">x</span>"#.repeat(N)
    );
    let (out, took) = timed(&html);
    assert_eq!(out, "<p>after</p>");
    assert!(took < BOUND, "{took:?}");
}

#[test]
fn nested_hidden_elements_stay_linear() {
    let html = format!(
        "{}x{}<p>after</p>",
        r#"<span style="display:none">"#.repeat(200),
        "</span>".repeat(200)
    );
    let (out, took) = timed(&html);
    assert_eq!(out, "<p>after</p>");
    assert!(took < BOUND, "{took:?}");
}
