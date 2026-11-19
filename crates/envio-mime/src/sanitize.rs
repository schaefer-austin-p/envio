//! HTML mail to Qt rich text: ammonia does the cleaning, then
//! [`crate::images`] decides about images and over-wide tables, and
//! [`crate::hidden`] removes elements that CSS hides; [`crate::linear`]
//! turns layout tables into blocks and [`crate::backgrounds`] repaints the
//! blocks inside a coloured container.

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::LazyLock;

use ammonia::{Builder, UrlRelative};

use crate::style::filter_style;
use crate::tagnames::stripped_names;

/// Tags kept: Qt's "Supported HTML Subset" (Qt 5.15) minus document-level
/// tags (html, head, body, title, meta, qt).
pub(crate) const TAGS: &[&str] = &[
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
    "col",
    "colgroup",
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
pub(crate) const DROP_WITH_CONTENT: &[&str] = &[
    "script",
    "style",
    "iframe",
    "object",
    "embed",
    "applet",
    "form",
    "meta",
    "base",
    "link",
    "svg",
    "math",
    "title",
    "head",
    "noscript",
    "template",
    "frame",
    "frameset",
    "select",
    "textarea",
    "button",
    "xmp",
    "noembed",
    "noframes",
    "plaintext",
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
            "role",
        ],
    );
    allow("col", &["width"]);
    allow("td", CELL_ATTRS);
    allow("th", CELL_ATTRS);
    for tag in ["div", "p", "dl", "dt", "h1", "h2", "h3", "h4", "h5", "h6"] {
        allow(tag, BLOCK_ATTRS);
    }
    map
}

/// What the reader decides about images, and the labels it shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenderOptions {
    /// Load http and https images. Off by default (spec: privacy).
    pub allow_remote_images: bool,
    /// Content-ID (without angle brackets) to the URL the UI can load the
    /// inline part from.
    pub cid_urls: HashMap<String, String>,
    /// Natural pixel size (width, height) of the inline images whose header
    /// gave one, by Content-ID. A cid image with no width of its own is
    /// capped to the room it has when it would draw wider.
    pub cid_sizes: HashMap<String, (u32, u32)>,
    /// Natural pixel size (width, height) of remote images, by URL as the
    /// message writes it (and [`Rendered::remote_images`] lists it): the
    /// reader reports them once loaded. Capped like a cid image.
    pub image_sizes: HashMap<String, (u32, u32)>,
    /// Shown in place of a blocked image, or a `cid:` image the message
    /// doesn't hold, that has no alt text. Translated by the caller.
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
    /// The http and https images the HTML loads, each once, in order. The
    /// reader preloads them: Qt 5's rich `TextEdit` reserves a remote
    /// image's space but never draws it once it arrives (#163).
    pub remote_images: Vec<String>,
    /// The remote images written with no width, which a reported natural
    /// size could still cap (see [`RenderOptions::image_sizes`]).
    pub unsized_images: Vec<crate::Unsized>,
    /// Tracking pixels removed (even when remote images are on).
    pub removed_trackers: usize,
    /// The sanitised HTML sets its own background or dark text colour, so
    /// the reader draws it on a light card (see [`crate::sets_colours`]).
    pub own_colours: bool,
}

static CLEANER: LazyLock<Builder<'static>> = LazyLock::new(|| cleaner(&NO_EXTRA));

static NO_EXTRA: BTreeSet<String> = BTreeSet::new();

/// The cleaner, also letting `extra` tags through (see [`crate::tagnames`]).
fn cleaner(extra: &BTreeSet<String>) -> Builder<'_> {
    let mut builder = Builder::empty();
    builder
        .tags(
            TAGS.iter()
                .copied()
                .chain(extra.iter().map(String::as_str))
                .collect(),
        )
        .clean_content_tags(DROP_WITH_CONTENT.iter().copied().collect())
        .tag_attributes(tag_attributes())
        .generic_attributes(["style"].into_iter().collect())
        .url_schemes(["http", "https", "mailto", "cid"].into_iter().collect())
        // Ammonia checks URLs before `filter_attribute` and drops a relative
        // one there under `Deny`; this policy runs after the filter, so a
        // `//host` link it gave a scheme survives and every other one goes.
        .url_relative(UrlRelative::Custom(Box::new(no_relative_url)))
        .link_rel(None)
        .strip_comments(true)
        .attribute_filter(filter_attribute);
    builder
}

fn no_relative_url(_: &str) -> Option<Cow<'_, str>> {
    None
}

/// Sanitizes HTML mail into Qt rich text.
///
/// Mail nested deeper than [`crate::nesting::MAX_DEPTH`] skips ammonia and Qt
/// layout (both blow up on deep nesting) and shows as escaped plain text.
pub fn sanitize_html(html: &str, options: &RenderOptions) -> Rendered {
    if crate::nesting::too_deep(html) {
        let text = mail_parser::decoders::html::html_to_text(html);
        return Rendered {
            html: crate::plain::plain_to_rich(&text, false, false),
            ..Rendered::default()
        };
    }
    let extra = stripped_names(html);
    let clean = if extra.is_empty() {
        CLEANER.clean(html)
    } else {
        cleaner(&extra).clean(html)
    };
    let clean = clean.to_string();
    let mut images = Vec::new();
    let mut rendered = crate::images::fit(&clean, options, &mut images);
    rendered.html = crate::linear::linearise(&rendered.html, &images, !has_doctype(html));
    rendered.html = crate::emptydiv::drop_empty(&rendered.html);
    rendered.html = crate::backgrounds::fill(&rendered.html);
    rendered.html = crate::fontsize::fit(&rendered.html);
    rendered.own_colours = crate::colours::sets_colours(&rendered.html);
    rendered
}

/// Whether the mail starts with a doctype (after a BOM, whitespace,
/// comments or an `<?xml ...?>` prolog, which a browser reads as a bogus
/// comment), so it would be laid out in standards mode.
fn has_doctype(html: &str) -> bool {
    let mut rest = html.trim_start_matches(['\u{feff}', ' ', '\t', '\r', '\n']);
    loop {
        let skipped = if let Some(tail) = rest.strip_prefix("<!--") {
            tail.find("-->").map(|end| &tail[end + 3..])
        } else if let Some(tail) = rest.strip_prefix("<?") {
            tail.find('>').map(|end| &tail[end + 1..])
        } else {
            break;
        };
        match skipped {
            Some(next) => rest = next.trim_start(),
            None => return false,
        }
    }
    rest.get(..9)
        .is_some_and(|p| p.eq_ignore_ascii_case("<!doctype"))
}

/// Runs on every attribute ammonia kept. Links keep their exact address
/// (the reader shows it) but only for http, https and mailto.
fn filter_attribute<'u>(element: &str, attribute: &str, value: &'u str) -> Option<Cow<'u, str>> {
    match attribute {
        "href" => match with_scheme(as_browser_reads(value)) {
            Cow::Borrowed(href) => web_href(href),
            Cow::Owned(href) => web_href(&href).map(|h| Cow::Owned(h.into_owned())),
        },
        "src" => has_scheme(value, &["http:", "https:", "cid:"]).then_some(Cow::Borrowed(value)),
        "style" => filter_style(element, value).map(Cow::Owned),
        _ => Some(Cow::Borrowed(value)),
    }
}

/// An http, https or mailto href made safe to show, else `None`.
fn web_href(href: &str) -> Option<Cow<'_, str>> {
    (clean_url(href) && has_scheme(href, &["http:", "https:", "mailto:"]))
        .then(|| crate::link::safe_href(href))
        .flatten()
}

/// The href as a browser reads it (WHATWG URL): spaces and C0 controls
/// trimmed from both ends, tabs and newlines dropped anywhere. Mailers that
/// wrap lines break long links this way. Any other control character stays,
/// so `clean_url` still refuses it.
fn as_browser_reads(href: &str) -> Cow<'_, str> {
    let trimmed = href.trim_matches(|c: char| c <= ' ');
    if trimmed.contains(['\t', '\n', '\r']) {
        Cow::Owned(
            trimmed
                .chars()
                .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
                .collect(),
        )
    } else {
        Cow::Borrowed(trimmed)
    }
}

/// A scheme-less `//host` or `www.` link with https added. A mail has no
/// page address to resolve it against, and plain-text `www.` links open over
/// https too. Only a host starting with a letter or digit counts: `///x`,
/// `//?q`, `// host` or `//@x` name none, so they are left as they are and
/// lose their href rather than open a sheet that can't open them.
fn with_scheme(href: Cow<'_, str>) -> Cow<'_, str> {
    let names_host = |rest: &str| rest.chars().next().is_some_and(char::is_alphanumeric);
    let www = href
        .get(..4)
        .is_some_and(|p| p.eq_ignore_ascii_case("www."));
    match href.strip_prefix("//") {
        Some(rest) if names_host(rest) => Cow::Owned(format!("https:{href}")),
        _ if www && names_host(&href[4..]) => Cow::Owned(format!("https://{href}")),
        _ => href,
    }
}

/// No control character anywhere and no whitespace inside the scheme.
fn clean_url(url: &str) -> bool {
    let scheme = url.trim_start().split(':').next().unwrap_or_default();
    !url.chars().any(char::is_control) && !scheme.contains(char::is_whitespace)
}

fn has_scheme(url: &str, schemes: &[&str]) -> bool {
    let url = url.trim_start().to_ascii_lowercase();
    schemes.iter().any(|s| url.starts_with(s))
}
