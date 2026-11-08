//! Listing a message's attachments and fetching one's decoded bytes.

use mail_parser::{MessagePart, MimeHeaders};

use crate::mail::{Parsed, is_broken_multipart, parse_mail};
use crate::thread::normalize_id;

/// One attachment as the reader lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentInfo {
    /// Position in the message's attachment list; pass it to
    /// [`attachment_bytes`].
    pub index: usize,
    /// As the sender wrote it, so untrusted: it can hold `/`, `..` or
    /// control characters. Sanitize it before using it as a file name or a
    /// Content Hub name.
    pub filename: Option<String>,
    /// `type/subtype`, lower case.
    pub content_type: String,
    /// Decoded size in bytes.
    pub size: usize,
    /// Content-ID without angle brackets (what `cid:` URLs name).
    pub content_id: Option<String>,
    /// Shown inside the body (an inline image) rather than as a file.
    pub inline: bool,
}

/// Every attachment, `text/calendar` parts included. Empty for a broken
/// multipart message.
pub(crate) fn list(message: &Parsed<'_>) -> Vec<AttachmentInfo> {
    if is_broken_multipart(message) {
        return Vec::new();
    }
    message
        .attachments()
        .enumerate()
        .map(|(index, part)| info(index, part))
        .collect()
}

/// The decoded bytes of attachment `index`, as listed by [`crate::parse`].
pub fn attachment_bytes(raw: &[u8], index: usize) -> Option<Vec<u8>> {
    let message = parse_mail(raw)?;
    if is_broken_multipart(&message) {
        return None;
    }
    let index = u32::try_from(index).ok()?;
    Some(message.attachment(index)?.contents().to_vec())
}

fn info(index: usize, part: &MessagePart<'_>) -> AttachmentInfo {
    let content_type = part.content_type().map_or_else(
        || "application/octet-stream".to_owned(),
        |ct| match ct.subtype() {
            Some(sub) => format!("{}/{}", ct.ctype(), sub).to_ascii_lowercase(),
            None => ct.ctype().to_ascii_lowercase(),
        },
    );
    let content_id = part.content_id().and_then(normalize_id);
    let disposition = part.content_disposition();
    let inline = match disposition {
        Some(d) => d.is_inline(),
        None => content_id.is_some(),
    };
    AttachmentInfo {
        index,
        filename: part.attachment_name().map(str::to_owned),
        content_type,
        size: part.len(),
        content_id,
        inline,
    }
}
