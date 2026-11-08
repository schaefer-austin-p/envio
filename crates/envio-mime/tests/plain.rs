//! Plain text to rich text.

use envio_mime::plain_to_rich;

fn rich(text: &str) -> String {
    plain_to_rich(text, false, false)
}

fn wrap(inner: &str) -> String {
    format!(r#"<div style="white-space:pre-wrap">{inner}</div>"#)
}

#[test]
fn escapes_and_joins_lines() {
    assert_eq!(
        rich("a < b & c\n\"d\""),
        wrap("a &lt; b &amp; c<br>&quot;d&quot;")
    );
}

#[test]
fn empty_text() {
    assert_eq!(rich(""), wrap(""));
}

#[test]
fn quotes_become_nested_blockquotes() {
    const BQ: &str = r#"<blockquote style="white-space:pre-wrap">"#;
    assert_eq!(
        rich("hi\n> one\n> > two\n>> three\nbye"),
        wrap(&format!(
            "hi{BQ}one{BQ}two<br>three</blockquote></blockquote>bye"
        ))
    );
}

#[test]
fn linkifies_addresses() {
    let cases = [
        (
            "see https://example.com/a?b=1&c=2.",
            r#"see <a href="https://example.com/a?b=1&amp;c=2">https://example.com/a?b=1&amp;c=2</a>."#,
        ),
        (
            "(www.example.org)",
            r#"(<a href="https://www.example.org">www.example.org</a>)"#,
        ),
        (
            "mail mailto:bob@example.org now",
            r#"mail <a href="mailto:bob@example.org">mailto:bob@example.org</a> now"#,
        ),
        (
            "https://en.example.org/wiki/Foo_(bar)",
            r#"<a href="https://en.example.org/wiki/Foo_(bar)">https://en.example.org/wiki/Foo_(bar)</a>"#,
        ),
        (
            "HTTP://EXAMPLE.COM",
            r#"<a href="HTTP://EXAMPLE.COM">HTTP://EXAMPLE.COM</a>"#,
        ),
        ("nohttps://example.com", "nohttps://example.com"),
        ("just https:// alone", "just https:// alone"),
        ("javascript:alert(1)", "javascript:alert(1)"),
        (
            "<https://example.com>",
            r#"&lt;<a href="https://example.com">https://example.com</a>&gt;"#,
        ),
    ];
    for (text, inner) in cases {
        assert_eq!(rich(text), wrap(inner), "{text}");
    }
}

#[test]
fn unflows_format_flowed() {
    assert_eq!(
        plain_to_rich("one \ntwo\n three\nfour", true, false),
        wrap("one two<br>three<br>four")
    );
}

#[test]
fn delsp_removes_the_soft_break_space() {
    assert_eq!(plain_to_rich("lo \nng", true, true), wrap("long"));
}

#[test]
fn flowed_quotes_join_only_at_the_same_depth() {
    const BQ: &str = r#"<blockquote style="white-space:pre-wrap">"#;
    assert_eq!(
        plain_to_rich("> a \n> b\n>> c \nd", true, false),
        wrap(&format!("{BQ}a b{BQ}c </blockquote></blockquote>d"))
    );
}

#[test]
fn signature_separator_is_not_a_soft_break() {
    assert_eq!(
        plain_to_rich("text\n-- \nme", true, false),
        wrap("text<br>-- <br>me")
    );
}

#[test]
fn non_ascii_text_survives() {
    assert_eq!(
        rich("Grüße 会議 https://例え.example/"),
        wrap(r#"Grüße 会議 <a href="https://例え.example/">https://例え.example/</a>"#)
    );
}

#[test]
fn quote_depth_is_clamped() {
    let line = format!("{}deep", ">".repeat(100_000));
    let out = rich(&line);
    assert!(out.matches("<blockquote").count() <= 10);
    assert_eq!(out.matches("<blockquote").count(), 10);
    assert!(out.contains("deep"));
    let spaced = format!("{}deep", "> ".repeat(100_000));
    assert_eq!(rich(&spaced).matches("<blockquote").count(), 10);
}
