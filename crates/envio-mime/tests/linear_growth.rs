//! `sanitize_html` does work linear in the input (#365). The work counter
//! (`envio_mime::take_work`, behind the `test-hooks` feature that
//! dev-dependencies turn on) counts the bytes and nodes every pass visits,
//! so doubling the input must about double the count. Unlike a timing
//! bound it is exact, so llvm-cov and a busy runner cannot move it.
//! A rescan of the input or of the output so far, in any pass, shows up
//! as a ratio near 4.
//!
//! Each shape names a mark that only its target pass leaves in the output,
//! so a shape that stops reaching that pass (an early return, markup that
//! html5ever drops) fails instead of passing vacuously.

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
        max_font_px: 0,
        ..RenderOptions::default()
    }
}

fn work(html: &str, options: &RenderOptions) -> u64 {
    take_work();
    let _ = sanitize_html(html, options);
    take_work()
}

/// Asserts that the shape ran its target pass (`ran` is in the output),
/// that the counter is wired in (more than the one unit per input byte
/// `sanitize_html` ticks at entry, twice over), then that 2N costs at most
/// `MAX_RATIO` times N.
fn assert_linear(shape: &str, options: &RenderOptions, build: impl Fn(usize) -> String, ran: &str) {
    assert_linear_without(shape, options, build, (ran, ""));
}

/// [`assert_linear`], and `gone` (when not empty) must be absent from the
/// output at both sizes: what the target pass removes.
fn assert_linear_without(
    shape: &str,
    options: &RenderOptions,
    build: impl Fn(usize) -> String,
    (ran, gone): (&str, &str),
) {
    let (small, large) = (build(N), build(2 * N));
    for html in [&small, &large] {
        let out = sanitize_html(html, options).html;
        assert!(
            out.contains(ran),
            "{shape}: the target pass did not run at {} bytes: no {ran:?} in the output",
            html.len()
        );
        assert!(
            gone.is_empty() || !out.contains(gone),
            "{shape}: the target pass did not run at {} bytes: {gone:?} is still in the output",
            html.len()
        );
    }
    let (w1, w2) = (work(&small, options), work(&large, options));
    assert!(
        w1 > 2 * small.len() as u64,
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
    let shape = "distinct urls, allowed";
    // `images::fit` scales a 900 px image to the 600 px reader width.
    let build = |n| imgs(n, r#" width="900""#);
    assert_linear(shape, &options(true), build, r#"width="600""#);
}

#[test]
fn distinct_image_urls_blocked_with_alt_labels() {
    assert_linear(
        "distinct urls, blocked",
        &options(false),
        |n| imgs(n, r#" alt="photo" width="300" style="height:90px""#),
        r#"<span style="text-decoration:none">photo</span>"#,
    );
}

#[test]
fn many_sized_images_of_one_url() {
    assert_linear(
        "one url, many sizes",
        &options(true),
        |n| {
            (0..n)
                .map(|h| {
                    let tag = r#"<img src="https://example.com/a.png" width="900" height=""#;
                    format!("{tag}{}\">", h + 1)
                })
                .collect()
        },
        r#"width="600""#,
    );
}

#[test]
fn images_inside_many_open_cells() {
    // `images::fit` looks up the enclosing cell's width for each image:
    // 30 nested cells, each block holding one image at the bottom.
    assert_linear(
        "open cells",
        &options(true),
        |n| {
            let open = r#"<table width="100%"><tr><td width="50">"#;
            let close = "</td></tr></table>";
            let block = format!(
                r#"{}<img src="https://e.com/a.png" width="40">{}"#,
                open.repeat(30),
                close.repeat(30)
            );
            block.repeat(n / 40)
        },
        r#"<div align="left">"#,
    );
}

#[test]
fn deep_nesting_repeated() {
    // 200 levels, under the 256 guard; scaling the repeats, not the depth.
    assert_linear(
        "deep nesting",
        &options(true),
        |n| {
            let block = format!("{}x{}", "<div>".repeat(200), "</div>".repeat(200));
            block.repeat(n / 100)
        },
        "<div>",
    );
}

#[test]
fn long_text() {
    assert_linear(
        "long text",
        &options(true),
        |n| format!("<p>{}</p>", "word &amp; <b>bold</b> ".repeat(n)),
        "word &amp; <b>bold</b>",
    );
}

#[test]
fn many_links() {
    assert_linear(
        "links",
        &options(true),
        |n| {
            (0..n)
                .map(|i| format!(r#"<a href="https://example.com/{i}">link {i}</a> "#))
                .collect()
        },
        r#"<a href="https://example.com/1">"#,
    );
}

#[test]
fn many_table_rows_linearised() {
    // One cell per row: a layout table, written out as blocks.
    assert_linear(
        "table rows",
        &options(true),
        |n| {
            let row = r#"<tr><td align="center" style="padding:4px"><b>cell</b> text</td></tr>"#;
            format!(r#"<table width="100%">{}</table>"#, row.repeat(n))
        },
        "<div",
    );
}

#[test]
fn many_hidden_elements() {
    // `images::fit` drops a hidden element with its content.
    let hidden = r#"<div style="display:none"><span>gone</span></div>kept "#;
    let shape = "hidden elements";
    assert_linear_without(
        shape,
        &options(true),
        |n| hidden.repeat(n),
        ("kept", "gone"),
    );
}

#[test]
fn many_font_sizes() {
    // `fontsize::fit` turns a heading into an explicit size.
    assert_linear(
        "font sizes",
        &options(true),
        |n| r#"<font size="7">big</font><h1>t</h1>"#.repeat(n),
        r#"<h1 style="font-size:x-large">"#,
    );
}

#[test]
fn many_backgrounds() {
    // `backgrounds::fill` runs only when `background-color` or `bgcolor`
    // survives parsing (a stray `<td bgcolor>` is dropped). It writes the
    // container's colour, in six digits, onto the block inside it.
    let shape = |n: usize| {
        let item = r##"<div style="background-color:#eee"><p>x</p><table bgcolor="#ccc"><tr><td><div>y</div></td></tr></table></div>"##;
        item.repeat(n / 2)
    };
    assert_linear(
        "backgrounds",
        &options(true),
        shape,
        r#"<p style="background-color:#eeeeee">x</p>"#,
    );
}

#[test]
fn many_nested_backgrounds_under_one_container() {
    // One container holds every block: fill's open stack stays shallow but
    // the inherited colour is copied to each.
    assert_linear(
        "one container",
        &options(true),
        |n| {
            format!(
                r#"<div style="background-color:#eee">{}</div>"#,
                "<p>x</p>".repeat(n)
            )
        },
        r#"<p style="background-color:#eeeeee">x</p>"#,
    );
}
