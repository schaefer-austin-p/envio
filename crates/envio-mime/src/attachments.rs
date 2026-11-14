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
    /// The IMAP section of a part that was left on the server (a big
    /// message is stored without its attachments): its bytes are not here,
    /// [`attachment_bytes`] gives none, and `size` is the server's figure.
    pub section: Option<String>,
}

/// Names of the headers envio-net writes on a part it left out.
const SECTION_HEADER: &str = "x-envio-section";
const SIZE_HEADER: &str = "x-envio-size";

/// The text of the stub header `name` on `part`, if there is one.
fn stub_header<'a>(part: &'a MessagePart<'_>, name: &str) -> Option<&'a str> {
    part.headers()
        .iter()
        .find(|h| h.name.as_str().eq_ignore_ascii_case(name))
        .and_then(|h| h.value().as_text())
}

fn is_stub(part: &MessagePart<'_>) -> bool {
    stub_header(part, SECTION_HEADER).is_some()
}

/// The decoded body of a part fetched on its own: its headers, a blank
/// line, its still transfer-encoded body.
pub fn decode_part(raw: &[u8]) -> Option<Vec<u8>> {
    let message = parse_mail(raw)?;
    Some(message.part(0)?.contents().to_vec())
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
    let part = message.attachment(index).filter(|p| !is_stub(p))?;
    Some(part.contents().to_vec())
}

/// The decoded bytes of each attachment in `indexes`, from one parse of
/// `raw`; `None` for an index the message does not have. Gives what
/// [`attachment_bytes`] gives per index.
pub fn attachments_bytes(raw: &[u8], indexes: &[usize]) -> Vec<Option<Vec<u8>>> {
    let mut out = Vec::with_capacity(indexes.len());
    with_attachments(raw, indexes, |_, bytes| {
        out.push(bytes.map(<[u8]>::to_vec));
    });
    out
}

/// Calls `f` with each of `indexes` (its position in the slice, and the
/// decoded bytes borrowed from one parse of `raw`, `None` for an index the
/// message does not have), in order. Copies nothing.
pub fn with_attachments(raw: &[u8], indexes: &[usize], mut f: impl FnMut(usize, Option<&[u8]>)) {
    let message = parse_mail(raw).filter(|m| !is_broken_multipart(m));
    for (n, &i) in indexes.iter().enumerate() {
        let bytes = message.as_ref().and_then(|m| {
            let i = u32::try_from(i).ok()?;
            let part = m.attachment(i).filter(|p| !is_stub(p))?;
            Some(part.contents())
        });
        f(n, bytes);
    }
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
    let section = stub_header(part, SECTION_HEADER).map(str::to_owned);
    let size = section
        .as_ref()
        .and_then(|_| stub_header(part, SIZE_HEADER)?.trim().parse().ok())
        .unwrap_or_else(|| part.len());
    AttachmentInfo {
        index,
        filename: part.attachment_name().map(str::to_owned),
        content_type,
        size,
        content_id,
        inline,
        section,
    }
}
