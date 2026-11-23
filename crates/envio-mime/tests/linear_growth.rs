//! `sanitize_html` does work linear in the input (#365). The work counter
//! (`envio_mime::take_work`, behind the `test-hooks` feature that
//! dev-dependencies turn on) counts the bytes and nodes every pass visits,
//! so doubling the input must about double the count. Unlike a timing
//! bound it is exact, so llvm-cov and a busy runner cannot move it.
//! A rescan of the input or of the output so far, in any pass, shows up
//! as a ratio near 4.

use envio_mime::{RenderOptions, sanitize_html, take_work};

/// Doubling may cost this much more than double: fixed costs and the
/// image cap bending the curve a little. A quadratic pass gives about 4.
const MAX_RATIO: f64 = 2.2;
const N: usize = 4_000;

fn options(allow: bool) -> RenderOptions {
    RenderOptions {
        allow_remote_images: allow,
        blocked_label: "[image]".to_owned(),
        max_image_width: 600,
        ..RenderOptions::default()
    }
}

fn work(html: &str, options: &RenderOptions) -> u64 {
    take_work();
    let _ = sanitize_html(html, options);
    take_work()
}

/// Asserts the counter works at all, then that 2N costs at most
/// `MAX_RATIO` times N.
fn assert_linear(shape: &str, options: &RenderOptions, build: impl Fn(usize) -> String) {
    let (small, large) = (build(N), build(2 * N));
    let (w1, w2) = (work(&small, options), work(&large, options));
    assert!(
        w1 >= small.len() as u64,
        "{shape}: counter saw {w1} units for {} bytes: it is not wired in",
        small.len()
    );
    let ratio = w2 as f64 / w1 as f64;
    assert!(
        ratio <= MAX_RATIO,
        "{shape}: work grew {ratio:.2}x for 2x input ({w1} -> {w2}); a pass is super-linear"
    );
}

fn imgs(n: usize, attrs: &str) -> String {
    (0..n)
        .map(|i| format!(r#"<img src="https://example.com/{i}.png"{attrs}>"#))
        .collect()
}

#[test]
fn distinct_image_urls_allowed() {
    assert_linear("distinct urls, allowed", &options(true), |n| imgs(n, ""));
}

#[test]
fn distinct_image_urls_blocked_with_alt_labels() {
    assert_linear("distinct urls, blocked", &options(false), |n| {
        imgs(n, r#" alt="photo" width="300" style="height:90px""#)
    });
}

#[test]
fn many_sized_images_of_one_url() {
    assert_linear("one url, many sizes", &options(true), |n| {
        (0..n)
            .map(|h| {
                format!(
                    r#"<img src="https://example.com/a.png" height="{}">"#,
                    h + 1
                )
            })
            .collect()
    });
}

#[test]
fn deep_nesting_repeated() {
    // 200 levels, under the 256 guard; scaling the repeats, not the depth.
    assert_linear("deep nesting", &options(true), |n| {
        let block = format!("{}x{}", "<div>".repeat(200), "</div>".repeat(200));
        block.repeat(n / 100)
    });
}

#[test]
fn long_text() {
    assert_linear("long text", &options(true), |n| {
        format!("<p>{}</p>", "word &amp; <b>bold</b> ".repeat(n))
    });
}

#[test]
fn many_links() {
    assert_linear("links", &options(true), |n| {
        (0..n)
            .map(|i| format!(r#"<a href="https://example.com/{i}">link {i}</a> "#))
            .collect()
    });
}

#[test]
fn many_table_rows_linearised() {
    // One cell per row: a layout table, written out as blocks.
    let table = |n: usize| {
        let row = r#"<tr><td align="center" style="padding:4px"><b>cell</b> text</td></tr>"#;
        format!(r#"<table width="100%">{}</table>"#, row.repeat(n))
    };
    assert!(
        sanitize_html(&table(3), &options(true))
            .html
            .contains("<div")
    );
    assert_linear("table rows", &options(true), table);
}

#[test]
fn many_hidden_and_styled_elements() {
    assert_linear("hidden and styled", &options(true), |n| {
        r##"<div style="display:none"><span>gone</span></div><font size="5" color="red">big</font><td bgcolor="#eee">x</td>"##.repeat(n)
    });
}
