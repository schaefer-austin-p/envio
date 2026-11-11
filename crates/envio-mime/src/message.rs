//! A whole message for the reader: envelope, chosen body, attachments.

use mail_parser::{MessagePart, MimeHeaders, PartType};

use crate::attachments::{AttachmentInfo, list};
use crate::envelope::{Envelope, from_parsed};
use crate::mail::{Parsed, is_broken_multipart, parse_mail};
use crate::plain::unflow_text;

/// A parsed message, or the spec's "Strange message": raw text to show as is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Parsed {
        envelope: Box<Envelope>,
        body: Body,
        attachments: Vec<AttachmentInfo>,
    },
    /// The whole message as lossy UTF-8.
    Raw(String),
}

/// The body the reader shows. HTML wins when there is an alternative.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    Html(String),
    /// `flowed` and `delsp` come from `format=flowed; delsp=yes` (RFC 3676).
    Text {
        text: String,
        flowed: bool,
        delsp: bool,
    },
    Empty,
}

/// Parses a full message. Never panics; mail that can't be made sense of
/// comes back as [`Message::Raw`].
pub fn parse(raw: &[u8]) -> Message {
    match parse_mail(raw) {
        Some(message) if !is_broken_multipart(&message) => {
            let attachments = list(&message);
            Message::Parsed {
                envelope: Box::new(from_parsed(&message, &attachments)),
                body: body(&message),
                attachments,
            }
        }
        _ => Message::Raw(String::from_utf8_lossy(raw).into_owned()),
    }
}

/// The plain text to quote in a reply or forward: what envio-core passes to
/// [`crate::reply`] as [`crate::Quote::text`] and to [`crate::forward`] as
/// `original_text`. In order: the text/plain alternative, the HTML body
/// turned into text, the plain body; for mail [`parse`] shows as
/// [`Message::Raw`], the raw text. format=flowed text comes back unflowed
/// (soft line breaks joined, quote prefixes kept).
/// Takes the raw bytes because [`Message`] keeps only the HTML when there
/// is an alternative.
pub fn quote_text(raw: &[u8]) -> String {
    let Some(message) = parse_mail(raw).filter(|m| !is_broken_multipart(m)) else {
        return String::from_utf8_lossy(raw).into_owned();
    };
    if let Some(part) = message.text_part(0) {
        if let Body::Text {
            text,
            flowed: true,
            delsp,
        } = text_body(part)
        {
            return unflow_text(&text, delsp);
        }
        if let PartType::Text(text) = &part.body {
            return text.to_string();
        }
    }
    if let Some(PartType::Html(html)) = message.html_part(0).map(|p| &p.body) {
        return mail_parser::decoders::html::html_to_text(html);
    }
    String::new()
}

fn body(message: &Parsed<'_>) -> Body {
    if let Some(PartType::Html(html)) = message.html_part(0).map(|p| &p.body) {
        return Body::Html(html.to_string());
    }
    match message.text_part(0) {
        Some(part) => text_body(part),
        None => Body::Empty,
    }
}

fn text_body(part: &MessagePart<'_>) -> Body {
    let PartType::Text(text) = &part.body else {
        return Body::Empty;
    };
    let param = |name: &str, want: &str| {
        part.content_type()
            .and_then(|ct| ct.attribute(name))
            .is_some_and(|v| v.trim().eq_ignore_ascii_case(want))
    };
    Body::Text {
        text: text.to_string(),
        flowed: param("format", "flowed"),
        delsp: param("delsp", "yes"),
    }
}
