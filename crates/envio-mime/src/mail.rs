//! The one place that calls mail-parser, and the checks for "strange" mail.

use mail_parser::{MessageParser, MimeHeaders, PartType};

pub(crate) type Parsed<'x> = mail_parser::Message<'x>;

/// Parses a full message or a header-only blob. `None` when it doesn't look
/// like mail: mail-parser found no headers, or none of the ones every mail
/// has (From, To, Subject, Date, Message-ID, Content-Type).
pub(crate) fn parse_mail(raw: &[u8]) -> Option<Parsed<'_>> {
    let message = MessageParser::default().parse(raw)?;
    let looks_like_mail = message.from().is_some()
        || message.to().is_some()
        || message.subject().is_some()
        || message.date().is_some()
        || message.message_id().is_some()
        || message.content_type().is_some();
    looks_like_mail.then_some(message)
}

/// A multipart message whose boundary never shows up: mail-parser keeps the
/// whole body as one part. Shown as raw text, not as a lone attachment.
pub(crate) fn is_broken_multipart(message: &Parsed<'_>) -> bool {
    let declared = message
        .content_type()
        .is_some_and(|ct| ct.c_type.eq_ignore_ascii_case("multipart"));
    let root_is_multipart = matches!(message.root_part().body, PartType::Multipart(_));
    declared && !root_is_multipart
}
