//! The tag tables of the nesting guard, from html5ever's tree builder.

use html5ever::tokenizer::states::RawKind;
use html5ever::tokenizer::{Tag, TokenSinkResult};

/// HTML elements that have no end tag.
pub(super) const VOID: &[&str] = &[
    "area", "base", "basefont", "bgsound", "br", "col", "embed", "frame", "hr", "image", "img",
    "input", "keygen", "link", "meta", "param", "source", "track", "wbr",
];

/// Formatting elements: their end tag runs the adoption agency, which keeps
/// the special elements above them open, so it only pops from the top.
pub(super) const FORMATTING: &[&str] = &[
    "a", "b", "big", "code", "em", "font", "i", "nobr", "s", "small", "strike", "strong", "tt", "u",
];

/// Special elements whose end tag closes the implied-end elements above them
/// (when no table cell is in the way).
pub(super) const BLOCK: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "button",
    "center",
    "details",
    "dialog",
    "dir",
    "div",
    "dl",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "header",
    "hgroup",
    "listing",
    "main",
    "menu",
    "nav",
    "ol",
    "pre",
    "search",
    "section",
    "summary",
    "ul",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
];

/// Start tags that leave svg or math (html5ever `step_foreign`).
pub(super) const BREAKOUT: &[&str] = &[
    "b",
    "big",
    "blockquote",
    "body",
    "br",
    "center",
    "code",
    "dd",
    "div",
    "dl",
    "dt",
    "em",
    "embed",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "hr",
    "i",
    "img",
    "li",
    "listing",
    "menu",
    "meta",
    "nobr",
    "ol",
    "p",
    "pre",
    "ruby",
    "s",
    "small",
    "span",
    "strong",
    "strike",
    "sub",
    "sup",
    "table",
    "tt",
    "u",
    "ul",
    "var",
];

/// `<font>` leaves foreign content only with one of these attributes.
pub(super) fn breaks_out(tag: &Tag) -> bool {
    let name = &*tag.name;
    BREAKOUT.contains(&name)
        || (name == "font"
            && tag
                .attrs
                .iter()
                .any(|attr| matches!(&*attr.name.local, "color" | "face" | "size")))
}

/// The tokenizer state html5ever's tree builder picks after an HTML start
/// tag (scripting on, as in ammonia).
pub(super) fn raw_text(name: &str) -> TokenSinkResult<()> {
    match name {
        "script" => TokenSinkResult::RawData(RawKind::ScriptData),
        "style" | "xmp" | "iframe" | "noembed" | "noframes" | "noscript" => {
            TokenSinkResult::RawData(RawKind::Rawtext)
        }
        "textarea" | "title" => TokenSinkResult::RawData(RawKind::Rcdata),
        "plaintext" => TokenSinkResult::Plaintext,
        _ => TokenSinkResult::Continue,
    }
}
