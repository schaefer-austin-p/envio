//! Single-column layout tables become blocks; data tables stay tables.

use envio_mime::{RenderOptions, sanitize_html};

fn clean(html: &str) -> String {
    let options = RenderOptions {
        max_image_width: 360,
        ..RenderOptions::default()
    };
    sanitize_html(html, &options).html
}

fn count(html: &str, tag: &str) -> usize {
    let open = format!("<{tag}");
    html.match_indices(&open)
        .filter(|(i, _)| html[i + open.len()..].starts_with([' ', '>', '/']))
        .count()
}

/// Byte positions of `needles` in `html`, asserting they appear in order.
fn in_order(html: &str, needles: &[&str]) {
    let mut at = 0;
    for needle in needles {
        let found = html[at..]
            .find(needle)
            .unwrap_or_else(|| panic!("{needle} missing or out of order in {html}"));
        at += found + needle.len();
    }
}

#[test]
fn a_single_column_table_becomes_blocks() {
    let html = clean(
        r#"<table width="100%"><tbody><tr><td>one</td></tr><tr><td>two</td></tr></tbody></table>"#,
    );
    for tag in ["table", "tbody", "tr", "td"] {
        assert_eq!(count(&html, tag), 0, "{tag} left in {html}");
    }
    assert_eq!(count(&html, "div"), 3, "{html}");
    in_order(&html, &["one", "two"]);
}

#[test]
fn nested_single_column_tables_in_a_two_column_row_become_blocks() {
    let html = clean(
        r#"<table><tr><td width="76">pic</td><td>
        <table width="100%"><tr><td>head</td></tr></table>
        <table width="100%"><tr><td>body</td></tr></table>
        </td></tr></table>"#,
    );
    assert_eq!(count(&html, "table"), 1, "{html}");
    assert_eq!(count(&html, "tr"), 1, "{html}");
    assert_eq!(count(&html, "td"), 2, "{html}");
    assert!(html.contains(r#"width="76""#), "{html}");
    in_order(&html, &["pic", "head", "body"]);
}

#[test]
fn a_table_with_th_is_never_linearised() {
    let html = clean(r#"<table><tr><th>Item</th></tr><tr><td>Tea</td></tr></table>"#);
    assert_eq!(count(&html, "table"), 1, "{html}");
    assert_eq!(count(&html, "th"), 1, "{html}");
    assert_eq!(count(&html, "td"), 1, "{html}");
}

#[test]
fn a_two_column_table_is_kept() {
    let html = clean(r#"<table><tr><td>Tea</td><td>$3</td></tr><tr><td>Cake</td></tr></table>"#);
    assert_eq!(count(&html, "table"), 1, "{html}");
    assert_eq!(count(&html, "tr"), 2, "{html}");
    assert_eq!(count(&html, "td"), 3, "{html}");
}

#[test]
fn linearising_keeps_text_order_and_alignment() {
    let html = clean(
        r#"<table cellpadding="4" bgcolor="red"><tr><td align="right" style="color:red" width="20" valign="top">first <b>bold</b></td></tr><tr><td align="center">second</td></tr></table>"#,
    );
    in_order(&html, &["first", "<b>bold</b>", "second"]);
    assert!(
        html.contains(r#"<div align="right" style="color:red">first"#),
        "{html}"
    );
    assert!(html.contains(r#"<div align="center">second"#), "{html}");
    for dropped in ["cellpadding", "bgcolor", "width", "valign"] {
        assert!(!html.contains(dropped), "{dropped} kept in {html}");
    }
}

#[test]
fn colspan_cell_alone_in_its_row_counts_as_one() {
    let html = clean(r#"<table><tr><td colspan="3">wide</td></tr><tr><td>next</td></tr></table>"#);
    assert_eq!(count(&html, "table"), 0, "{html}");
    assert!(!html.contains("colspan"), "{html}");
    in_order(&html, &["wide", "next"]);
}

#[test]
fn a_table_without_rows_is_kept() {
    // Nothing to judge: keep what the sanitiser produced.
    let html = clean(r#"<table></table>"#);
    assert_eq!(count(&html, "table"), 1, "{html}");
}
