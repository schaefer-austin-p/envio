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
    for dropped in ["cellpadding", "bgcolor=", "width", "valign"] {
        assert!(!html.contains(dropped), "{dropped} kept in {html}");
    }
    assert!(html.contains("background-color:red"), "{html}");
}

#[test]
fn a_linearised_cell_keeps_its_background_colour() {
    let html = clean(
        r##"<table><tr><td bgcolor="#1a1f36"><font color="#ffffff">Pay now</font></td></tr></table>"##,
    );
    assert!(
        html.contains(r#"<div style="background-color:#1a1f36;">"#),
        "{html}"
    );
    assert!(!html.contains("bgcolor="), "{html}");
}

#[test]
fn a_cell_style_follows_the_background_so_it_wins() {
    let html = clean(r##"<table><tr><td bgcolor="#abc" style="color:red">x</td></tr></table>"##);
    assert!(
        html.contains(r#"style="background-color:#abc;color:red""#),
        "{html}"
    );
}

#[test]
fn a_row_background_reaches_its_cells_unless_the_cell_has_one() {
    let html = clean(
        r##"<table><tr bgcolor="#111111"><td>a</td></tr><tr bgcolor="#111111"><td bgcolor="blue">b</td></tr><tr><td>c</td></tr></table>"##,
    );
    assert!(
        html.contains(r#"<div style="background-color:#111111;">a"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<div style="background-color:blue;">b"#),
        "{html}"
    );
    assert!(!html.contains("#111111;\">b"), "{html}");
    assert!(html.contains("<div>c"), "{html}");
}

#[test]
fn a_junk_background_value_is_dropped() {
    for junk in ["red;position:fixed", "#12", "#ggg", "url(x)", ""] {
        let html = clean(&format!(
            r#"<table><tr><td bgcolor="{junk}">x</td></tr></table>"#
        ));
        assert!(!html.contains("background"), "{junk}: {html}");
        assert!(!html.contains("position"), "{junk}: {html}");
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

fn with_images(html: &str) -> String {
    let options = RenderOptions {
        max_image_width: 360,
        allow_remote_images: true,
        ..RenderOptions::default()
    };
    sanitize_html(html, &options).html
}

/// A newsletter header: logo | picture | tagline, side by side at 600 px.
const HEADER: &str = r#"<table width="600"><tr>
    <td width="200"><img src="https://news.example/logo.png" alt="Logo" width="180" height="40"></td>
    <td width="200"><img src="https://news.example/hero.png" alt="Hero" width="180" height="120"></td>
    <td width="200">Find your next home</td>
    </tr></table>"#;

#[test]
fn a_multi_column_row_of_images_stacks_whether_or_not_they_load() {
    let blocked = clean(HEADER);
    let shown = with_images(HEADER);
    assert!(!blocked.contains("<img"), "{blocked}");
    assert_eq!(count(&shown, "img"), 2, "{shown}");
    for html in [blocked, shown] {
        for tag in ["table", "tr", "td"] {
            assert_eq!(count(&html, tag), 0, "{tag} left in {html}");
        }
        assert_eq!(count(&html, "div"), 4, "{html}");
        in_order(&html, &["Logo", "Hero", "Find your next home"]);
    }
}

#[test]
fn a_nested_table_in_a_wide_row_stacks() {
    let html = clean(
        r#"<table><tr><td>left</td><td><table><tr><td>a</td><td>b</td></tr></table></td></tr></table>"#,
    );
    // The outer table stacks; the inner one holds short text and stays.
    assert_eq!(count(&html, "table"), 1, "{html}");
    assert_eq!(count(&html, "td"), 2, "{html}");
    in_order(&html, &["left", "<table", "a", "b"]);
}

#[test]
fn block_content_in_a_wide_row_stacks() {
    let html = clean(r#"<table><tr><td><h1>News</h1></td><td><p>Read more</p></td></tr></table>"#);
    assert_eq!(count(&html, "table"), 0, "{html}");
    in_order(&html, &["News", "Read more"]);
}

#[test]
fn a_presentation_table_stacks_even_with_short_text() {
    for role in ["presentation", "none", "Presentation"] {
        let html = clean(&format!(
            r#"<table role="{role}"><tr><td>Home</td><td>Search</td></tr></table>"#
        ));
        assert_eq!(count(&html, "table"), 0, "{role}: {html}");
        assert!(!html.contains("role="), "{role}: {html}");
        in_order(&html, &["Home", "Search"]);
    }
}

#[test]
fn a_header_cell_keeps_a_presentation_table() {
    let html = clean(
        r#"<table role="presentation"><tr><th>Item</th><th>Price</th></tr><tr><td><p>Tea</p></td><td>$3</td></tr></table>"#,
    );
    assert_eq!(count(&html, "table"), 1, "{html}");
    assert_eq!(count(&html, "td"), 2, "{html}");
}

#[test]
fn a_receipt_of_short_text_keeps_its_columns() {
    let html = clean(
        r#"<table width="100%"><tr><td>Tea</td><td align="right">$3.00</td></tr><tr><td><b>Total</b></td><td align="right"><b>$3.00</b></td></tr></table>"#,
    );
    assert_eq!(count(&html, "table"), 1, "{html}");
    assert_eq!(count(&html, "td"), 4, "{html}");
}

#[test]
fn a_narrow_icon_column_beside_the_content_stays() {
    // An avatar or icon at a small fixed width leaves the rest of a phone
    // screen to the text, so the columns are kept.
    let html = clean(
        r#"<table><tr><td width="48"><img src="https://news.example/a.png" alt="A" width="40" height="40"></td><td><p>Ada commented</p></td></tr></table>"#,
    );
    assert_eq!(count(&html, "table"), 1, "{html}");
    assert_eq!(count(&html, "td"), 2, "{html}");
}

const ICONS: &str = r#"<table><tr>
    <td><img src="https://news.example/f.png" alt="F" width="32" height="32"></td>
    <td><img src="https://news.example/t.png" alt="T" width="32" height="32"></td>
    <td><img src="https://news.example/i.png" alt="I" width="32" height="32"></td>
    </tr><tr><td><img src="https://news.example/logo.png" alt="Logo" width="40" height="40"></td><td>Acme Inc</td></tr></table>"#;

#[test]
fn icons_sized_by_their_images_keep_their_columns() {
    // The cells have no width of their own; their images do.
    for html in [clean(ICONS), with_images(ICONS)] {
        assert_eq!(count(&html, "table"), 1, "{html}");
        assert_eq!(count(&html, "td"), 5, "{html}");
    }
}

/// Icons, pretty-printed, with a spacer `&nbsp;` beside a smaller one. Taken
/// as text, that cell would put the other two (150 px) over `SIDE_MAX`.
const SPACED_ICONS: &str = "<table><tr>
    <td>
        <img src=\"https://news.example/f.png\" alt=\"F\" width=\"100\" height=\"100\">
    </td>
    <td>&nbsp;<img src=\"https://news.example/t.png\" alt=\"T\" width=\"50\" height=\"50\">&nbsp; </td>
    <td>
        <img src=\"https://news.example/i.png\" alt=\"I\" width=\"50\" height=\"50\">
    </td>
    </tr></table>";

#[test]
fn whitespace_and_spacers_beside_an_icon_are_not_text() {
    for html in [clean(SPACED_ICONS), with_images(SPACED_ICONS)] {
        assert_eq!(count(&html, "table"), 1, "{html}");
        assert_eq!(count(&html, "td"), 3, "{html}");
    }
}

const CARDS: &str = r#"<table><tr>
    <td><img src="https://news.example/a.png" alt="A" width="100" height="80"> A flat by the river with a view</td>
    <td><img src="https://news.example/b.png" alt="B" width="100" height="80"> A house with a garden</td>
    </tr></table>"#;

#[test]
fn a_picture_with_a_caption_takes_the_width_of_the_text() {
    for html in [clean(CARDS), with_images(CARDS)] {
        assert_eq!(count(&html, "table"), 0, "{html}");
        in_order(&html, &["A", "river", "B", "garden"]);
    }
}

#[test]
fn pictures_side_by_side_in_a_cell_add_up() {
    let html = clean(
        r#"<table><tr><td><img src="https://news.example/a.png" alt="A" width="100" height="40"><img src="https://news.example/b.png" alt="B" width="100" height="40"></td><td>Text</td></tr></table>"#,
    );
    assert_eq!(count(&html, "table"), 0, "{html}");
    in_order(&html, &["A", "B", "Text"]);
}

#[test]
fn images_without_a_width_leave_their_cells_free() {
    let html = clean(
        r#"<table><tr><td><img src="https://news.example/a.png" alt="A"></td><td><img src="https://news.example/b.png" alt="B"></td></tr></table>"#,
    );
    assert_eq!(count(&html, "table"), 0, "{html}");
    in_order(&html, &["A", "B"]);
}

#[test]
fn two_free_width_columns_beside_an_icon_stack() {
    let html = clean(
        r#"<table><tr><td width="48"><img src="https://news.example/a.png" alt="A" width="40" height="40"></td><td><p>one</p></td><td><p>two</p></td></tr></table>"#,
    );
    assert_eq!(count(&html, "table"), 0, "{html}");
    in_order(&html, &["A", "one", "two"]);
}

#[test]
fn wide_fixed_side_columns_stack() {
    let html = clean(
        r#"<table><tr><td width="150"><p>left</p></td><td width="300"><p>main</p></td></tr></table>"#,
    );
    assert_eq!(count(&html, "table"), 0, "{html}");
    in_order(&html, &["left", "main"]);
}

#[test]
fn a_hidden_table_does_not_shift_the_verdicts() {
    // `fit` drops hidden elements; the table after one is still judged on
    // its own content.
    let html = clean(
        r#"<div style="display:none"><table><tr><td><p>x</p></td><td><p>y</p></td></tr></table></div><table><tr><td>Tea</td><td>$3</td></tr></table>"#,
    );
    assert_eq!(count(&html, "table"), 1, "{html}");
    assert_eq!(count(&html, "td"), 2, "{html}");
}
