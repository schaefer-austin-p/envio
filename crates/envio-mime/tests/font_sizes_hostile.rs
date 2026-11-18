//! For a hostile corpus of HTML mails (huge %, em, px, pt, keywords, nested
//! relative sizes, headings and size tags), no element in `sanitize_html`'s
//! output renders above 26px or below 1px (#191, #243). "Renders" is the
//! model in `common`, from Qt 5's source.

mod common;

use common::{out_of_range, painted_sizes};
use envio_mime::{RenderOptions, sanitize_html};

fn clean(html: &str) -> String {
    sanitize_html(html, &RenderOptions::default()).html
}

const TAGS: [&str; 14] = [
    "div", "span", "em", "b", "h1", "h2", "h3", "h4", "h5", "h6", "big", "small", "sub", "sup",
];

const SIZES: [&str; 18] = [
    "",
    "font-size:99999999%",
    "font-size:300%",
    "font-size:5%",
    "font-size:0.001%",
    "font-size:50em",
    "font-size:0.01em",
    "font-size:999999px",
    "font-size:1px",
    "font-size:0",
    "font-size:99999pt",
    "font-size:xx-large",
    "font-size:xxx-large",
    "font-size:larger",
    "font-size:smaller",
    "font-size:x-small",
    "font-size:1e9px",
    "font-size:150% !important",
];

/// Each element as opening markup: a tag with a size, or a `<font size>`.
fn elements(tags: &[&str], sizes: &[&str]) -> Vec<(String, String)> {
    let mut els = Vec::new();
    for tag in tags {
        for size in sizes {
            let attr = if size.is_empty() {
                String::new()
            } else {
                format!(r#" style="{size}""#)
            };
            els.push((format!("<{tag}{attr}>"), format!("</{tag}>")));
        }
    }
    for size in ["7", "+9", "-5", "1", "3", "+2147483647"] {
        els.push((format!(r#"<font size="{size}">"#), "</font>".to_owned()));
    }
    els
}

fn corpus(depth: usize, els: &[(String, String)]) -> Vec<String> {
    let mut docs = vec![(String::new(), String::new())];
    for _ in 0..depth {
        docs = docs
            .iter()
            .flat_map(|(open, close)| {
                els.iter()
                    .map(move |(o, c)| (format!("{open}{o}"), format!("{c}{close}")))
            })
            .collect();
    }
    docs.into_iter()
        .map(|(open, close)| format!("{open}text{close}"))
        .collect()
}

fn check(docs: &[String]) {
    let mut bad = Vec::new();
    for html in docs {
        let out = clean(html);
        let over = out_of_range(&out);
        if !over.is_empty() {
            bad.push(format!("{html}\n  -> {out}\n  {over:?}"));
        }
    }
    assert!(
        bad.is_empty(),
        "{} of {} mails render out of range, e.g.\n{}",
        bad.len(),
        docs.len(),
        bad.iter().take(5).cloned().collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn no_element_renders_out_of_range_one_deep() {
    check(&corpus(1, &elements(&TAGS, &SIZES)));
}

#[test]
fn no_element_renders_out_of_range_two_deep() {
    check(&corpus(2, &elements(&TAGS, &SIZES)));
}

#[test]
fn no_element_renders_out_of_range_three_deep() {
    let tags = ["div", "h1", "big", "small", "sub"];
    let sizes = [
        "",
        "font-size:300%",
        "font-size:1px",
        "font-size:xx-large",
        "font-size:5%",
    ];
    check(&corpus(3, &elements(&tags, &sizes)));
}

/// The model itself, on the cases the corpus is built around.
#[test]
fn the_model_knows_what_qt_does() {
    let sizes =
        |html: &str| -> Vec<i64> { painted_sizes(html).into_iter().map(|(_, px)| px).collect() };
    assert_eq!(sizes("<h1>t</h1>"), [32]);
    assert_eq!(
        sizes(r#"<h1 style="font-size:10px"><b>t</b></h1>"#),
        [32, 32]
    );
    assert_eq!(sizes("<big><big>t</big></big>"), [19, 19]);
    assert_eq!(sizes("<h2>t</h2><h3>t</h3><h5>t</h5>"), [24, 19, 13]);
    assert_eq!(
        sizes(r#"<div style="font-size:1px"><sub>t</sub></div>"#),
        [1, 0]
    );
    assert_eq!(sizes(r#"<div style="font-size:300%">t</div>"#), [48]);
    assert_eq!(sizes(r#"<font size="7">t</font>"#), [38]);
}
