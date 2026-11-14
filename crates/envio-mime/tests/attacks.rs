//! The sanitizer against hostile HTML: properties that must hold for every
//! file in tests/attacks, with remote images off and on.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use envio_mime::{RenderOptions, sanitize_html};

/// Tags Qt may see; anything else in the output is a failure.
const ALLOWED_TAGS: &[&str] = &[
    "a",
    "address",
    "b",
    "big",
    "blockquote",
    "br",
    "center",
    "cite",
    "code",
    "dd",
    "dfn",
    "div",
    "dl",
    "dt",
    "em",
    "font",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "i",
    "img",
    "kbd",
    "li",
    "nobr",
    "ol",
    "p",
    "pre",
    "s",
    "samp",
    "small",
    "span",
    "strong",
    "sub",
    "sup",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "tr",
    "tt",
    "u",
    "ul",
    "var",
];

fn files(dir: &str) -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(dir);
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    files.sort();
    files
}

fn options(allow_remote_images: bool) -> RenderOptions {
    RenderOptions {
        allow_remote_images,
        cid_urls: HashMap::from([(
            "image001.png@01DA0000.12345670".to_owned(),
            "image://envio/part/0".to_owned(),
        )]),
        cid_sizes: HashMap::new(),
        blocked_label: "[image]".to_owned(),
        max_image_width: 600,
    }
}

/// Every tag in `html`, `<` to `>`, skipping `>` inside quoted values.
fn tags(html: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        let mut quote = None;
        let mut end = rest.len();
        for (i, c) in rest[start..].char_indices() {
            match (quote, c) {
                (None, '"' | '\'') => quote = Some(c),
                (Some(q), c) if c == q => quote = None,
                (None, '>') => {
                    end = start + i + 1;
                    break;
                }
                _ => {}
            }
        }
        found.push(&rest[start..end]);
        rest = &rest[end..];
    }
    found
}

fn tag_name(tag: &str) -> String {
    tag.trim_start_matches('<')
        .trim_start_matches('/')
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Attribute names of a start tag (values skipped).
fn attribute_names(tag: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut chars = tag.trim_start_matches('<').chars().peekable();
    while chars.next_if(|c| !c.is_whitespace() && *c != '>').is_some() {}
    loop {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        let name: String = std::iter::from_fn(|| {
            chars.next_if(|c| !c.is_whitespace() && !matches!(c, '=' | '>' | '/'))
        })
        .collect();
        if name.is_empty() {
            break;
        }
        names.push(name.to_ascii_lowercase());
        if chars.next_if_eq(&'=').is_some()
            && let Some(q) = chars.next_if(|c| *c == '"' || *c == '\'')
        {
            for c in chars.by_ref() {
                if c == q {
                    break;
                }
            }
        }
    }
    names
}

fn check(name: &str, html: &str, remote: bool) {
    let lower = html.to_ascii_lowercase();
    assert!(!lower.contains("<script"), "{name}: script in {html}");
    for tag in tags(html) {
        let tag_lower = tag.to_ascii_lowercase();
        let tag_name = tag_name(tag);
        assert!(
            ALLOWED_TAGS.contains(&tag_name.as_str()),
            "{name}: tag {tag} in {html}"
        );
        for attr in attribute_names(tag) {
            assert!(!attr.starts_with("on"), "{name}: {attr} in {tag}");
            assert!(
                !matches!(attr.as_str(), "srcset" | "background" | "class" | "id"),
                "{name}: {attr} in {tag}"
            );
        }
        for bad in [
            "javascript:",
            "vbscript:",
            "data:",
            "url(",
            "expression",
            "position",
        ] {
            assert!(!tag_lower.contains(bad), "{name}: {bad} in {tag}");
        }
        if !remote {
            assert!(
                !tag_lower.contains("src=\"http"),
                "{name}: remote image {tag}"
            );
        }
        if tag_name == "img" {
            assert!(
                !tag_lower.contains("list-manage.com"),
                "{name}: tracker {tag}"
            );
        }
    }
}

#[test]
fn attack_files_render_safely() {
    let files = files("tests/attacks");
    assert!(files.len() >= 12, "attack corpus missing");
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let html = std::fs::read_to_string(&path).unwrap();
        for remote in [false, true] {
            let rendered = sanitize_html(&html, &options(remote));
            check(&name, &rendered.html, remote);
        }
    }
}

#[test]
fn tag_scanner_sees_quoted_angle_brackets() {
    let found = tags(r#"<p title="<img src=x>">t</p>"#);
    assert_eq!(found, vec![r#"<p title="<img src=x>">"#, "</p>"]);
    assert_eq!(
        attribute_names(r#"<img src="a" onerror='b'>"#),
        vec!["src", "onerror"]
    );
}

fn render_file(name: &str, remote: bool) -> envio_mime::Rendered {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/attacks")
        .join(name);
    sanitize_html(&std::fs::read_to_string(path).unwrap(), &options(remote))
}

/// The safe content of an attack file must survive: a sanitizer that
/// deletes everything would pass the negative checks above.
#[test]
fn attack_files_keep_their_safe_content() {
    for remote in [false, true] {
        let forbidden = render_file("forbidden-tags.html", remote);
        assert!(forbidden.html.contains("<p>kept</p>"), "{}", forbidden.html);

        let links = render_file("javascript-links.html", remote);
        assert!(
            links
                .html
                .contains(r#"<a href="https://example.com/ok">ok</a>"#),
            "{}",
            links.html
        );
        assert!(links.html.contains("one"), "text of dropped links stays");

        let pixels = render_file("tracking-pixel.html", remote);
        assert_eq!(pixels.removed_trackers, 3);
        assert!(pixels.html.contains("<p>Newsletter</p>"), "{}", pixels.html);
        assert_eq!(pixels.html.contains("photo.jpg"), remote, "{}", pixels.html);
        assert_eq!(pixels.blocked_images, usize::from(!remote));

        let confusion = render_file("attribute-confusion.html", remote);
        assert!(
            confusion.html.contains(r#"src="image://envio/part/0""#),
            "{}",
            confusion.html
        );
        assert!(confusion.html.contains("cid link"));
    }
}
