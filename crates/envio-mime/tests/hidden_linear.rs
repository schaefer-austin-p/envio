//! Removing hidden elements is linear in the input, also for void tags and
//! for tags that never close.
//!
//! CI runs the tests under coverage instrumentation on a shared runner, so a
//! wall-clock bound is flaky. The tests compare growth instead: four times the
//! input should take about four times as long (linear), not sixteen
//! (quadratic). Each size takes its fastest of three runs to damp noise.

use std::time::{Duration, Instant};

use envio_mime::{RenderOptions, sanitize_html};

const SMALL: usize = 2_000;
const LARGE: usize = 4 * SMALL;
/// Linear growth gives a ratio near 4, quadratic near 16.
const MAX_RATIO: f64 = 8.0;

fn best_of_three(html: &str) -> (String, Duration) {
    let mut best = Duration::MAX;
    let mut out = String::new();
    for _ in 0..3 {
        let start = Instant::now();
        out = sanitize_html(html, &RenderOptions::default()).html;
        best = best.min(start.elapsed());
    }
    (out, best)
}

/// Sanitizes `make(SMALL)` and `make(LARGE)`, checks both outputs, and
/// asserts the larger input took less than `MAX_RATIO` times as long.
fn assert_linear(label: &str, make: impl Fn(usize) -> String, expected: &str) {
    let (small_out, small) = best_of_three(&make(SMALL));
    let (large_out, large) = best_of_three(&make(LARGE));
    assert_eq!(small_out, expected, "{label} (small)");
    assert_eq!(large_out, expected, "{label} (large)");
    let ratio = large.as_secs_f64() / small.as_secs_f64().max(1e-6);
    assert!(
        ratio < MAX_RATIO,
        "{label}: {small:?} for {SMALL}, {large:?} for {LARGE} (ratio {ratio:.1})"
    );
}

#[test]
fn hidden_void_tags_stay_linear() {
    for tag in ["bgsound", "basefont", "frame", "input", "wbr"] {
        assert_linear(
            tag,
            |n| {
                format!(
                    "{}<p>after</p>",
                    format!(r#"<{tag} style="display:none">"#).repeat(n)
                )
            },
            "<p>after</p>",
        );
    }
}

#[test]
fn many_hidden_elements_stay_linear() {
    assert_linear(
        "span",
        |n| {
            format!(
                "{}<p>after</p>",
                r#"<span style="display:none">x</span>"#.repeat(n)
            )
        },
        "<p>after</p>",
    );
}

#[test]
fn nested_hidden_elements_are_removed() {
    let html = format!(
        "{}x{}<p>after</p>",
        r#"<span style="display:none">"#.repeat(200),
        "</span>".repeat(200)
    );
    let out = sanitize_html(&html, &RenderOptions::default()).html;
    assert_eq!(out, "<p>after</p>");
}
