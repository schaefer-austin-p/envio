//! DKIM canonicalisation and the hashes (RFC 6376 3.4, 3.7, 5.4.2).
//!
//! Everything here works on bytes: simple canonicalisation must hash exactly
//! what the signer hashed, whatever the encoding of the message.
use std::borrow::Cow;
use std::collections::HashMap;

use ring::digest::{SHA256, digest};

use super::signature::{Canon, Signature};

const CRLF: &[u8] = b"\r\n";

fn is_wsp(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

fn is_fws(b: u8) -> bool {
    is_wsp(b) || b == b'\r' || b == b'\n'
}

/// Bytes mail-parser skips between fields (ASCII whitespace but LF).
fn is_blank_byte(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | 0x0c)
}

/// The header block: the byte range of each field, and where the body
/// starts (None when no blank line ends the headers). A line ends at LF, with
/// or without a CR before it; a line starting with WSP continues the field.
/// The header block ends at a line that is empty or holds only CR, space,
/// tab or form feed and is not a continuation: this is mail-parser's rule
/// (it skips ASCII whitespace but LF before each field and stops at an LF),
/// so `"\r\r\n"` ends the headers for DKIM as it does for the reader (the
/// body-injection finding on PR #336).
fn split_header(raw: &[u8]) -> (Vec<(usize, usize)>, Option<usize>) {
    let mut fields: Vec<(usize, usize)> = Vec::new();
    let mut pos = 0;
    while pos < raw.len() {
        let (content_end, next, terminated) = match raw[pos..].iter().position(|&b| b == b'\n') {
            Some(i) => (pos + i, pos + i + 1, true),
            None => (raw.len(), raw.len(), false),
        };
        let content = &raw[pos..content_end];
        let continuation = !fields.is_empty() && is_wsp(raw[pos]);
        if terminated && !continuation && content.iter().all(|&b| is_blank_byte(b)) {
            return (fields, Some(next));
        }
        match fields.last_mut() {
            Some(last) if continuation => last.1 = next,
            _ => fields.push((pos, next)),
        }
        pos = next;
    }
    (fields, None)
}

/// The raw header fields in order, each as its exact bytes (name, colon,
/// value, folded lines and the final line ending). A bare LF ends a line like
/// CRLF does.
pub(crate) fn header_fields(raw: &[u8]) -> Vec<&[u8]> {
    let (ranges, _) = split_header(raw);
    ranges.into_iter().map(|(s, e)| &raw[s..e]).collect()
}

/// Whether `field` is named `name` (lowercase), case-insensitively, by
/// [`header_name`].
pub(crate) fn field_named(field: &[u8], name: &[u8]) -> bool {
    header_name(field).is_some_and(|n| n.eq_ignore_ascii_case(name))
}

/// The body after the blank line, LF normalised to CRLF.
pub(crate) fn body(raw: &[u8]) -> Cow<'_, [u8]> {
    let Some(start) = body_start(raw) else {
        return Cow::Borrowed(&[]);
    };
    normalise_crlf(&raw[start..])
}

/// Where the body starts: None when no blank line ends the headers.
pub(crate) fn body_start(raw: &[u8]) -> Option<usize> {
    split_header(raw).1
}

fn normalise_crlf(data: &[u8]) -> Cow<'_, [u8]> {
    let bare = data
        .iter()
        .enumerate()
        .any(|(i, &b)| b == b'\n' && (i == 0 || data[i - 1] != b'\r'));
    if !bare {
        return Cow::Borrowed(data);
    }
    let mut out = Vec::with_capacity(data.len() + 16);
    for &b in data {
        if b == b'\n' && out.last() != Some(&b'\r') {
            out.push(b'\r');
        }
        out.push(b);
    }
    Cow::Owned(out)
}

/// The lines of `data`, ended by CRLF; a last line without one still counts.
fn crlf_lines(data: &[u8]) -> Vec<&[u8]> {
    let mut lines = Vec::new();
    let mut rest = data;
    while !rest.is_empty() {
        match rest.windows(2).position(|w| w == CRLF) {
            Some(i) => {
                lines.push(&rest[..i]);
                rest = &rest[i + 2..];
            }
            None => {
                lines.push(rest);
                break;
            }
        }
    }
    lines
}

/// WSP runs to one SP, WSP at both ends removed.
fn squeeze_wsp(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut pending = false;
    for &b in data {
        if is_wsp(b) {
            pending = !out.is_empty();
        } else {
            if pending {
                out.push(b' ');
                pending = false;
            }
            out.push(b);
        }
    }
    out
}

/// RFC 6376 3.4.3 and 3.4.4 on a body that already has CRLF line ends.
pub(super) fn canon_body(body: &[u8], canon: Canon) -> Vec<u8> {
    let mut lines = crlf_lines(body);
    let relaxed_lines: Vec<Vec<u8>>;
    if canon == Canon::Relaxed {
        // Leading WSP stays (reduced to one SP); only trailing WSP goes.
        relaxed_lines = lines
            .iter()
            .map(|l| {
                let lead = l.iter().take_while(|&&b| is_wsp(b)).count();
                let mut v = squeeze_wsp(l);
                if lead > 0 && !v.is_empty() {
                    v.insert(0, b' ');
                }
                v
            })
            .collect();
        lines = relaxed_lines.iter().map(Vec::as_slice).collect();
    }
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    if lines.is_empty() {
        return if canon == Canon::Simple {
            CRLF.to_vec()
        } else {
            Vec::new()
        };
    }
    let mut out = Vec::with_capacity(body.len() + 2);
    for l in lines {
        out.extend_from_slice(l);
        out.extend_from_slice(CRLF);
    }
    out
}

/// The SHA-256 of the canonical body. With `l`, the canonical body must be
/// exactly that long: shorter would let content be appended after the signed
/// part (decision 9), longer is a malformed signature.
pub(crate) fn body_hash(body: &[u8], canon: Canon, l: Option<u64>) -> Option<[u8; 32]> {
    let canonical = canon_body(body, canon);
    if l.is_some_and(|l| l != canonical.len() as u64) {
        return None;
    }
    digest(&SHA256, &canonical).as_ref().try_into().ok()
}

fn without_terminator(field: &[u8]) -> &[u8] {
    let f = field.strip_suffix(b"\n").unwrap_or(field);
    f.strip_suffix(b"\r").unwrap_or(f)
}

/// The field name, the one rule every name lookup uses (header selection for
/// `h=`, From and Content-Type counting, the well-formed guard): the bytes
/// before the first colon with trailing SP and TAB removed, and only when
/// every remaining byte is printable ASCII (33 to 126). A control byte (FF,
/// VT, CR, NUL), a space inside the name or an empty name gives None:
/// mail-parser skips such bytes and would read a different name than DKIM.
pub(crate) fn header_name(field: &[u8]) -> Option<&[u8]> {
    let colon = field.iter().position(|&b| b == b':')?;
    let name = &field[..colon];
    let end = name.iter().rposition(|&b| !is_wsp(b)).map_or(0, |i| i + 1);
    let name = &name[..end];
    (!name.is_empty() && name.iter().all(|b| (33..=126).contains(b))).then_some(name)
}

/// RFC 6376 3.4.1 and 3.4.2 for one field, ending in CRLF.
pub(super) fn canon_field(field: &[u8], canon: Canon) -> Vec<u8> {
    let field = without_terminator(field);
    let mut out = match canon {
        Canon::Simple => normalise_crlf(field).into_owned(),
        Canon::Relaxed => {
            let (name, value) = match field.iter().position(|&b| b == b':') {
                Some(c) => (&field[..c], &field[c + 1..]),
                None => (field, &b""[..]),
            };
            let mut out = squeeze_wsp(name).to_ascii_lowercase();
            out.push(b':');
            // Unfold: a line break before WSP goes; so does any other.
            let mut unfolded = Vec::with_capacity(value.len());
            for (i, &b) in value.iter().enumerate() {
                let cr_of_crlf = b == b'\r' && value.get(i + 1) == Some(&b'\n');
                if b != b'\n' && !cr_of_crlf {
                    unfolded.push(b);
                }
            }
            out.extend(squeeze_wsp(&unfolded));
            out
        }
    };
    out.extend_from_slice(CRLF);
    out
}

/// The DKIM-Signature field (no final line ending) with the value of its `b=`
/// tag removed, "including all surrounding whitespace" (RFC 6376 3.5): the
/// tag name and its `=` stay, and so does the `;` that ends it.
fn empty_b(field: &[u8]) -> Vec<u8> {
    let mut start = field.iter().position(|&b| b == b':').map_or(0, |c| c + 1);
    loop {
        let end = field[start..]
            .iter()
            .position(|&b| b == b';')
            .map_or(field.len(), |i| start + i);
        let tag = &field[start..end];
        if let Some(eq) = tag.iter().position(|&b| b == b'=') {
            let name = &tag[..eq];
            let from = name.iter().position(|&b| !is_fws(b)).unwrap_or(name.len());
            let to = name.iter().rposition(|&b| !is_fws(b)).map_or(0, |i| i + 1);
            if &name[from..to.max(from)] == b"b" {
                let mut out = field[..start + eq + 1].to_vec();
                out.extend_from_slice(&field[end..]);
                return out;
            }
        }
        if end >= field.len() {
            return field.to_vec();
        }
        start = end + 1;
    }
}

/// The data the signature covers (RFC 6376 3.7, 5.4.2): for each name in
/// `h=`, the next unused field of that name counting from the bottom (none
/// left: nothing, which is how over-signing works), canonicalised, then the
/// DKIM-Signature field itself (`fields[sig_index]`, never selected by `h=`)
/// canonicalised with `b=` emptied and no trailing CRLF. None when
/// `sig_index` is out of range.
pub(crate) fn signed_data(fields: &[&[u8]], sig_index: usize, sig: &Signature) -> Option<Vec<u8>> {
    let sig_field = *fields.get(sig_index)?;
    // Per name, the field indexes top to bottom: pop() gives the lowest left.
    let mut by_name: HashMap<Vec<u8>, Vec<usize>> = HashMap::new();
    for (i, f) in fields.iter().enumerate() {
        // The signature being checked is added last, with `b=` emptied; an
        // `h=dkim-signature` must never select it (RFC 6376 5.4.2).
        if i == sig_index {
            continue;
        }
        if let Some(name) = header_name(f) {
            by_name
                .entry(name.to_ascii_lowercase())
                .or_default()
                .push(i);
        }
    }
    let mut out = Vec::new();
    for name in &sig.signed_headers {
        let idx = by_name
            .get_mut(name.to_ascii_lowercase().as_bytes())
            .and_then(Vec::pop);
        if let Some(i) = idx {
            out.extend(canon_field(fields[i], sig.header_canon));
        }
    }
    let emptied = empty_b(without_terminator(sig_field));
    let mut own = canon_field(&emptied, sig.header_canon);
    own.truncate(own.len() - CRLF.len());
    out.extend(own);
    Some(out)
}
