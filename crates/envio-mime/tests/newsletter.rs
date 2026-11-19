//! A newsletter-shaped mail (#341): hidden preheaders go, an inner cell's own
//! alignment beats an outer centre, and the font a mail asks for survives.
//! The fixture is synthetic; it has the structure of a real notice mail.

use envio_mime::{RenderOptions, sanitize_html};

fn clean(html: &str) -> String {
    sanitize_html(html, &RenderOptions::default()).html
}

const NEWSLETTER: &str = r##"<html><head><title>Notice</title></head>
<body style="background:#fff;margin:0">
<div style="display:none;font-size:1px;line-height:1px;max-height:0;max-width:0;opacity:0;overflow:hidden;mso-hide:all">PREHEADER-TEXT</div>
<table width="100%" cellpadding="0" cellspacing="0"><tr><td align="center">
  <table width="100%" cellpadding="0" cellspacing="0"><tr>
    <td style="background:#454542;font:18px Verdana, Geneva, sans-serif;color:#fff">LOGO</td></tr></table>
  <table width="600" align="center" style="max-width:600px;margin:0 auto" cellpadding="0" cellspacing="0"><tr>
    <td style="font:15px/24px Verdana, Geneva, sans-serif;color:#454542">
      <p style="margin:0 0 10px">GREETING-TEXT</p></td></tr></table>
  <table width="100%" cellpadding="0" cellspacing="0"><tr>
    <td align="center" style="font:12px Arial, sans-serif">FOOTER-TEXT</td></tr></table>
</td></tr></table></body></html>"##;

/// The `align` attribute and `text-align` in force at `needle`, the way a
/// browser resolves them: the nearest ancestor that sets one.
fn alignment_at(html: &str, needle: &str) -> Option<String> {
    let at = html.find(needle).expect("needle in output");
    let mut open: Vec<(String, Option<String>)> = Vec::new();
    let mut pos = 0;
    while let Some(i) = html[pos..].find('<') {
        let start = pos + i;
        if start >= at {
            break;
        }
        let end = start + html[start..].find('>').unwrap() + 1;
        let tag = &html[start..end];
        pos = end;
        if tag.starts_with("</") {
            open.pop();
        } else if !tag.ends_with("/>") && !tag.starts_with("<br") && !tag.starts_with("<img") {
            let name: String = tag[1..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            let own = attr(tag, "align").or_else(|| {
                attr(tag, "style").and_then(|s| {
                    s.split(';')
                        .find_map(|d| d.strip_prefix("text-align:").map(str::to_owned))
                })
            });
            // A real table is a box: its text does not inherit past it in a
            // mail without a doctype, so it counts as aligned left.
            let own = if name == "table" && own.is_none() {
                Some("left".to_owned())
            } else {
                own
            };
            open.push((name, own));
        }
    }
    open.iter().rev().find_map(|(_, a)| a.clone())
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let key = format!(" {name}=\"");
    let from = tag.find(&key)? + key.len();
    Some(tag[from..from + tag[from..].find('"')?].to_owned())
}

#[test]
fn preheader_hidden_by_a_combination_is_removed() {
    let out = clean(NEWSLETTER);
    assert!(!out.contains("PREHEADER"), "{out}");
    assert!(
        out.contains("GREETING-TEXT") && out.contains("FOOTER-TEXT"),
        "{out}"
    );
}

#[test]
fn each_hiding_style_removes_its_element() {
    for style in [
        "opacity:0",
        "opacity: 0.0 !important",
        "max-height:0;overflow:hidden",
        "max-height:0px; overflow : hidden !important",
        "height:0;overflow:hidden",
        "font-size:0;max-height:0;overflow:hidden",
        "font-size:0px;height:0px;overflow:hidden !important",
    ] {
        let out = clean(&format!(r#"<div style="{style}">HIDDEN</div><p>real</p>"#));
        assert_eq!(out, "<p>real</p>", "{style}");
    }
}

#[test]
fn visible_look_alikes_are_kept() {
    for style in [
        // A zero font size alone is the inline-block gap trick: the children
        // set their own size and show.
        "font-size:0",
        "max-height:0",
        "overflow:hidden",
        "opacity:0.5",
        "opacity:1",
        "mso-hide:none",
        "mso-hide:all",
        "MSO-HIDE: all",
        "max-height:100px;overflow:hidden",
        "line-height:0",
        "font-size:0;line-height:0",
        "font-size:0;overflow:hidden",
        "font-size:0;max-height:0",
    ] {
        let out = clean(&format!(r#"<div style="{style}">SHOWN</div>"#));
        assert!(out.contains("SHOWN"), "{style}: {out}");
    }
}

#[test]
fn greeting_is_not_centred_by_the_outer_cell() {
    let out = clean(NEWSLETTER);
    assert_ne!(
        alignment_at(&out, "GREETING-TEXT").as_deref(),
        Some("center"),
        "{out}"
    );
    assert_ne!(
        alignment_at(&out, "LOGO").as_deref(),
        Some("center"),
        "{out}"
    );
}

#[test]
fn footer_cell_keeps_its_own_centre() {
    let out = clean(NEWSLETTER);
    assert_eq!(
        alignment_at(&out, "FOOTER-TEXT").as_deref(),
        Some("center"),
        "{out}"
    );
}

#[test]
fn inner_align_overrides_outer_align() {
    let out = clean(
        r#"<div align="center"><div align="left">L-TEXT</div><div style="text-align:right">R-TEXT</div><p>C-TEXT</p></div>"#,
    );
    assert_eq!(alignment_at(&out, "L-TEXT").as_deref(), Some("left"));
    assert_eq!(alignment_at(&out, "R-TEXT").as_deref(), Some("right"));
    assert_eq!(alignment_at(&out, "C-TEXT").as_deref(), Some("center"));
}

#[test]
fn a_doctype_keeps_the_inherited_centre_through_tables() {
    let out = clean(
        r#"<!DOCTYPE html><html><body><table><tr><td align="center"><table><tr><td>IN-TEXT</td></tr></table></td></tr></table></body></html>"#,
    );
    assert!(out.contains("IN-TEXT"), "{out}");
    assert!(!out.contains(r#"align="left""#), "{out}");
}

#[test]
fn font_shorthand_gives_the_family_and_size() {
    let out = clean(r#"<p style="font:15px/24px Verdana, Geneva, sans-serif">T</p>"#);
    assert!(
        out.contains("font-family:Verdana, Geneva, sans-serif"),
        "{out}"
    );
    assert!(out.contains("font-size:15px"), "{out}");
}

#[test]
fn font_shorthand_weight_and_style_come_through() {
    let out = clean(r#"<p style="font:italic 700 12px Arial, Helvetica, sans-serif">T</p>"#);
    assert!(out.contains("font-style:italic"), "{out}");
    assert!(out.contains("font-weight:700"), "{out}");
    assert!(
        out.contains("font-family:Arial, Helvetica, sans-serif"),
        "{out}"
    );
}

#[test]
fn a_sans_family_without_a_generic_gets_sans_serif() {
    let out = clean(r#"<p style="font-family:Arial">T</p><p style="font:14px Helvetica">U</p>"#);
    assert!(out.contains("font-family:Arial, sans-serif"), "{out}");
    assert!(out.contains("font-family:Helvetica, sans-serif"), "{out}");
}

#[test]
fn a_serif_family_is_left_alone() {
    let out = clean(r#"<p style="font-family:Georgia, serif">T</p>"#);
    assert!(out.contains("font-family:Georgia, serif"), "{out}");
    assert!(!out.contains("sans-serif"), "{out}");
}

#[test]
fn hostile_font_shorthand_is_dropped() {
    for style in [
        "font:caption",
        "font:url(x) 12px Arial",
        "font:12px",
        "font:",
        "font:bogus 12px Arial",
    ] {
        let out = clean(&format!(r#"<p style="{style}">T</p>"#));
        assert!(!out.contains("font-family"), "{style}: {out}");
    }
}

#[test]
fn an_empty_coloured_spacer_cell_leaves_no_bar() {
    // Qt gives an empty block one line of its background, over the next
    // block; a browser sizes it by its CSS height, which Qt does not read.
    let out = clean(
        r##"<table width="100%"><tr><td style="background:#454542"></td></tr></table><table width="100%"><tr><td style="color:#00a0e3">HEADING</td></tr></table>"##,
    );
    assert!(!out.contains("#454542"), "{out}");
    assert!(out.contains("HEADING"), "{out}");
}

#[test]
fn a_cell_with_content_keeps_its_colour() {
    let out = clean(
        r##"<table width="100%"><tr><td style="background:#454542">&nbsp;</td><td style="background:#222222"><b>x</b></td></tr></table>"##,
    );
    assert!(out.contains("#454542") && out.contains("#222222"), "{out}");
}

fn keeps_with_images(html: &str) -> String {
    sanitize_html(
        html,
        &RenderOptions {
            allow_remote_images: true,
            ..RenderOptions::default()
        },
    )
    .html
}

#[test]
fn zero_font_size_and_line_height_wrapper_keeps_image_and_button() {
    let out = keeps_with_images(
        r#"<div style="font-size:0;line-height:0"><a href="https://x.example/"><img src="https://x.example/hero.png" width="500" height="200"></a></div><div style="font-size:0px;line-height:0px"><a href="https://x.example/b">BUTTONTEXT</a></div>"#,
    );
    assert!(out.contains("hero.png"), "{out}");
    assert!(out.contains("BUTTONTEXT"), "{out}");
}

#[test]
fn zero_font_size_and_line_height_cell_keeps_its_content() {
    let out = keeps_with_images(
        r#"<table width="100%"><tr><td style="font-size:0;line-height:0"><img src="https://x.example/hero.png" width="500" height="200"></td></tr></table>"#,
    );
    assert!(out.contains("hero.png"), "{out}");
}

#[test]
fn clipped_zero_font_size_wrapper_keeps_a_child_with_its_own_size() {
    let out = clean(
        r#"<div style="font-size:0;overflow:hidden"><span style="font-size:16px">TEXTB</span></div>"#,
    );
    assert!(out.contains("TEXTB"), "{out}");
}

#[test]
fn an_xhtml_prolog_before_the_doctype_keeps_standards_mode() {
    let out = clean(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.0 Transitional//EN" "http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd"><html><body><table><tr><td align="center"><table><tr><td>IN-TEXT</td></tr></table></td></tr></table></body></html>"#,
    );
    assert!(out.contains("IN-TEXT"), "{out}");
    assert!(!out.contains(r#"align="left""#), "{out}");
}

#[test]
fn mso_hide_only_hides_in_outlook_so_the_content_stays() {
    let out = keeps_with_images(
        r#"<div style="mso-hide:all">VISIBLE-TEXT <img src="https://x.example/gif.png" width="300" height="100"></div>"#,
    );
    assert!(out.contains("VISIBLE-TEXT"), "{out}");
    assert!(out.contains("gif.png"), "{out}");
}
