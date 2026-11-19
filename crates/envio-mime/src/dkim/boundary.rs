//! Where the body starts, as the reader sees it (plan #285, decisions 6, 9).
//! DKIM hashes the body after the header block; the reader (mail-parser)
//! shows the body after its own header block. If the two disagree, a signed
//! message can carry text the signature never covered, shown as the body.

use mail_parser::MessageParser;

/// The offset where mail-parser's parsed body starts, or None when it finds
/// no header at all. A message with headers and no blank line has its body
/// at the end.
pub(crate) fn reader_body_start(raw: &[u8]) -> Option<usize> {
    let message = MessageParser::default().parse(raw)?;
    Some(message.root_part().offset_body as usize)
}

/// Every header line is a field the two parsers read alike: it starts with
/// a printable name followed by a colon (WSP may precede the colon), or is a
/// continuation of the one before. A line without a colon, or one that starts
/// with CR, FF or a control byte, is read differently by mail-parser (it
/// skips such lines or bytes), so the message is not trusted.
pub(crate) fn well_formed(raw: &[u8]) -> bool {
    super::canon::header_fields(raw).into_iter().all(|field| {
        let line = field.split(|&b| b == b'\n').next().unwrap_or(field);
        let Some(colon) = line.iter().position(|&b| b == b':') else {
            return false;
        };
        let name = line[..colon].trim_ascii_end();
        !name.is_empty() && name.iter().all(|b| (33..=126).contains(b))
    })
}

/// The header block is well formed and DKIM's body start (the end when no
/// blank line ends the headers) equals the reader's.
pub(crate) fn agrees(raw: &[u8]) -> bool {
    let ours = super::canon::body_start(raw).unwrap_or(raw.len());
    well_formed(raw) && reader_body_start(raw) == Some(ours)
}
