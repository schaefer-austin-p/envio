//! HTML mail to Qt rich text: ammonia does the cleaning, then the image
//! pass (Task 5) decides about images and over-wide tables.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use ammonia::{Builder, UrlRelative};

/// Tags kept: Qt's "Supported HTML Subset" (Qt 5.15) minus document-level
/// tags (html, head, body, title, meta, qt).
const TAGS: &[&str] = &[
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

/// Tags dropped together with everything inside them.
const DROP_WITH_CONTENT: &[&str] = &[
    "script", "style", "iframe", "object", "embed", "applet", "form", "meta", "base", "link",
    "svg", "math", "title", "head", "noscript", "template", "frame", "frameset", "select",
    "textarea", "button",
];

const BLOCK_ATTRS: &[&str] = &["align", "dir"];
const CELL_ATTRS: &[&str] = &[
    "width", "height", "bgcolor", "colspan", "rowspan", "align", "valign",
];

/// Per-tag attribute allowlist (`style` is allowed on every tag).
fn tag_attributes() -> HashMap<&'static str, HashSet<&'static str>> {
    let mut map: HashMap<&'static str, HashSet<&'static str>> = HashMap::new();
    let mut allow = |tag: &'static str, attrs: &[&'static str]| {
        map.entry(tag).or_default().extend(attrs.iter().copied());
    };
    allow("a", &["href"]);
    allow("img", &["src", "alt", "width", "height"]);
    allow("font", &["color", "face", "size"]);
    allow("hr", &["width"]);
    allow("ol", &["type"]);
    allow("ul", &["type"]);
    allow("tr", &["bgcolor"]);
    allow(
        "table",
        &[
            "width",
            "height",
            "border",
            "bgcolor",
            "cellspacing",
            "cellpadding",
            "align",
        ],
    );
    allow("td", CELL_ATTRS);
    allow("th", CELL_ATTRS);
    for tag in ["div", "p", "dl", "dt", "h1", "h2", "h3", "h4", "h5", "h6"] {
        allow(tag, BLOCK_ATTRS);
    }
    map
}

/// CSS properties kept in `style`.
const STYLE_PROPS: &[&str] = &[
    "color",
    "background-color",
    "font-family",
    "font-size",
    "font-style",
    "font-weight",
    "text-decoration",
    "text-align",
    "vertical-align",
    "white-space",
    "margin",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
    "padding",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
];

/// Also kept on `img`, so the image pass can spot 1x1 pixels and size the
/// image; it turns them into attributes (Qt ignores CSS width on images).
const IMG_SIZE_PROPS: &[&str] = &["width", "height"];

/// Substrings that make a CSS value unsafe, checked in lower case.
const BAD_CSS: &[&str] = &[
    "url(",
    "expression",
    "\\",
    "@",
    "/*",
    "<",
    ">",
    "javascript:",
    "vbscript:",
];

/// What the reader decides about images, and the labels it shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenderOptions {
    /// Load http and https images. Off by default (spec: privacy).
    pub allow_remote_images: bool,
    /// Content-ID (without angle brackets) to the URL the UI can load the
    /// inline part from.
    pub cid_urls: HashMap<String, String>,
    /// Shown in place of a blocked image that has no alt text. Translated by
    /// the caller.
    pub blocked_label: String,
    /// Widest image or table in pixels; wider ones are scaled down (images)
    /// or lose their width (tables, cells). 0 means no limit.
    pub max_image_width: u32,
}

/// Rich text for Qt plus what was held back.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rendered {
    pub html: String,
    /// Remote images not loaded because remote images are off.
    pub blocked_images: usize,
    /// Tracking pixels removed (even when remote images are on).
    pub removed_trackers: usize,
}

static CLEANER: LazyLock<Builder<'static>> = LazyLock::new(|| {
    let mut builder = Builder::empty();
    builder
        .tags(TAGS.iter().copied().collect())
        .clean_content_tags(DROP_WITH_CONTENT.iter().copied().collect())
        .tag_attributes(tag_attributes())
        .generic_attributes(["style"].into_iter().collect())
        .url_schemes(["http", "https", "mailto", "cid"].into_iter().collect())
        .url_relative(UrlRelative::Deny)
        .link_rel(None)
        .strip_comments(true)
        .attribute_filter(filter_attribute);
    builder
});

/// Sanitizes HTML mail into Qt rich text.
pub fn sanitize_html(html: &str, _options: &RenderOptions) -> Rendered {
    Rendered {
        html: CLEANER.clean(html).to_string(),
        ..Rendered::default()
    }
}

/// Runs on every attribute ammonia kept. Links keep their exact address
/// (the reader shows it) but only for http, https and mailto.
fn filter_attribute<'u>(element: &str, attribute: &str, value: &'u str) -> Option<Cow<'u, str>> {
    match attribute {
        "href" => {
            has_scheme(value, &["http:", "https:", "mailto:"]).then_some(Cow::Borrowed(value))
        }
        "src" => has_scheme(value, &["http:", "https:", "cid:"]).then_some(Cow::Borrowed(value)),
        "style" => filter_style(element, value).map(Cow::Owned),
        _ => Some(Cow::Borrowed(value)),
    }
}

fn has_scheme(url: &str, schemes: &[&str]) -> bool {
    let url = url.trim_start().to_ascii_lowercase();
    schemes.iter().any(|s| url.starts_with(s))
}

/// Keeps only allowed properties with safe values; `None` when nothing is
/// left.
pub(crate) fn filter_style(element: &str, style: &str) -> Option<String> {
    let kept: Vec<String> = style
        .split(';')
        .filter_map(|declaration| {
            let (prop, value) = declaration.split_once(':')?;
            let prop = prop.trim().to_ascii_lowercase();
            let value = value.trim();
            let allowed = STYLE_PROPS.contains(&prop.as_str())
                || (element == "img" && IMG_SIZE_PROPS.contains(&prop.as_str()));
            (allowed && safe_css_value(value)).then(|| format!("{prop}:{value}"))
        })
        .collect();
    (!kept.is_empty()).then(|| kept.join(";"))
}

fn safe_css_value(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let balanced = |q: char| value.chars().filter(|&c| c == q).count() % 2 == 0;
    !value.is_empty()
        && !BAD_CSS.iter().any(|bad| lower.contains(bad))
        && balanced('"')
        && balanced('\'')
}
