//! `display:none` removes the element whatever its tag, also tags the
//! sanitizer strips; visible stripped tags keep their text.

use envio_mime::{RenderOptions, sanitize_html};

fn clean(html: &str) -> String {
    sanitize_html(html, &RenderOptions::default()).html
}

#[test]
fn hidden_stripped_tags_lose_their_content() {
    for tag in [
        "section",
        "article",
        "header",
        "footer",
        "nav",
        "aside",
        "main",
        "figure",
        "details",
        "mark",
        "time",
        "x-preheader",
        "foo",
    ] {
        let html = format!(r#"<{tag} style="display:none">HIDDEN <b>deep</b></{tag}><p>real</p>"#);
        assert_eq!(clean(&html), "<p>real</p>", "{tag}");
    }
}

#[test]
fn hidden_stripped_tag_nests_with_same_name() {
    let out = clean(
        r#"a<section style="display:none"><section>HIDDEN</section>HIDDEN2</section>b<section>seen</section>"#,
    );
    assert_eq!(out, "abseen");
}

#[test]
fn visible_stripped_tags_keep_their_text() {
    assert_eq!(clean("<article><section>text</section></article>"), "text");
    assert_eq!(
        clean(r#"<x-card style="color:red">a <i>b</i></x-card>"#),
        "a <i>b</i>"
    );
    assert_eq!(
        clean(r#"<section style="display:block">shown</section>"#),
        "shown"
    );
}

#[test]
fn hidden_allowed_tags_still_removed_and_center_font_ok() {
    assert_eq!(
        clean(r#"<center style="display:none">H</center><font style="display:none">H</font>ok"#),
        "ok"
    );
}
