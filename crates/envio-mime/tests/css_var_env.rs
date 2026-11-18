//! `var(` and `env(` never reach Qt rich text on any allowed property (#276).

use envio_mime::{RenderOptions, sanitize_html};

fn clean(style: &str) -> String {
    sanitize_html(
        &format!(r#"<p style="{style}">x</p>"#),
        &RenderOptions::default(),
    )
    .html
}

#[test]
fn var_and_env_drop_the_declaration() {
    for style in [
        "color:var(--x)",
        "font-size:env(safe-area-inset-top)",
        "background-color:env(safe-area-inset-top)",
        "color:VAR(--x)",
        "color:Env(safe-area-inset-top)",
        "color:v\\61r(--x)",
        "color:e\\6e v(safe-area-inset-top)",
    ] {
        let html = clean(style);
        assert!(!html.contains("var("), "{style}: {html}");
        assert!(!html.contains("env("), "{style}: {html}");
        assert!(!html.contains("style="), "{style}: {html}");
    }
}

#[test]
fn other_declarations_survive_beside_a_dropped_one() {
    let html = clean("color:var(--x);font-weight:bold");
    assert!(html.contains("font-weight:bold"), "{html}");
    assert!(!html.contains("var("), "{html}");
}

#[test]
fn plain_colour_still_passes() {
    assert!(clean("color:#336699").contains("color:#336699"));
}
