//! Folder names: modified UTF-7 (RFC 3501 5.1.3) for display, and which
//! folder is Sent, Drafts, Trash and so on (RFC 6154 special use).

use async_imap::types::NameAttribute;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Inbox,
    Sent,
    Drafts,
    Trash,
    Junk,
    Archive,
    All,
    Flagged,
}

/// The folder's role: INBOX by name (any case, RFC 3501 5.1), the rest
/// from the server's special-use attributes.
pub fn role_of(path: &str, attributes: &[NameAttribute<'_>]) -> Option<Role> {
    if path.eq_ignore_ascii_case("INBOX") {
        return Some(Role::Inbox);
    }
    attributes.iter().find_map(|attribute| match attribute {
        NameAttribute::Sent => Some(Role::Sent),
        NameAttribute::Drafts => Some(Role::Drafts),
        NameAttribute::Trash => Some(Role::Trash),
        NameAttribute::Junk => Some(Role::Junk),
        NameAttribute::Archive => Some(Role::Archive),
        NameAttribute::All => Some(Role::All),
        NameAttribute::Flagged => Some(Role::Flagged),
        _ => None,
    })
}

/// Whether the folder can be opened (`\Noselect` and RFC 5258
/// `\NonExistent` can't).
pub fn selectable(attributes: &[NameAttribute<'_>]) -> bool {
    !attributes.iter().any(|attribute| match attribute {
        NameAttribute::NoSelect => true,
        NameAttribute::Extension(name) => name.eq_ignore_ascii_case("\\NonExistent"),
        _ => false,
    })
}

/// The display form of a modified UTF-7 name. None if it's malformed;
/// callers then show the raw name.
pub fn decode_utf7(raw: &str) -> Option<String> {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after.find('-')?;
        let encoded = &after[..end];
        if encoded.is_empty() {
            out.push('&');
        } else {
            out.push_str(&decode_run(encoded)?);
        }
        rest = &after[end + 1..];
    }
    if !rest.bytes().all(|b| (0x20..0x7f).contains(&b)) {
        return None;
    }
    out.push_str(rest);
    Some(out)
}

/// One `&...-` run: modified base64 (`,` for `/`, no padding) of UTF-16BE.
fn decode_run(encoded: &str) -> Option<String> {
    let mut bits: u32 = 0;
    let mut count = 0;
    let mut bytes = Vec::new();
    for c in encoded.bytes() {
        let value = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b',' => 63,
            _ => return None,
        };
        bits = (bits << 6) | u32::from(value);
        count += 6;
        if count >= 8 {
            count -= 8;
            bytes.push((bits >> count) as u8);
            bits &= (1 << count) - 1;
        }
    }
    if bytes.len() % 2 != 0 || bits != 0 {
        return None;
    }
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_be_bytes(*pair))
        .collect();
    let text = String::from_utf16(&units).ok()?;
    // A run must hold something that couldn't be written plainly.
    if text.chars().any(|c| (' '..='~').contains(&c)) {
        return None;
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modified_utf7_decodes() {
        // RFC 3501's own example.
        assert_eq!(
            decode_utf7("~peter/mail/&U,BTFw-/&ZeVnLIqe-").as_deref(),
            Some("~peter/mail/台北/日本語")
        );
        assert_eq!(decode_utf7("Entw&APw-rfe").as_deref(), Some("Entwürfe"));
        assert_eq!(decode_utf7("Tom &- Jerry").as_deref(), Some("Tom & Jerry"));
        assert_eq!(decode_utf7("INBOX").as_deref(), Some("INBOX"));
    }

    #[test]
    fn malformed_utf7_is_refused() {
        assert_eq!(decode_utf7("&U,BTFw"), None); // no closing '-'
        assert_eq!(decode_utf7("&!!-"), None);
        assert_eq!(decode_utf7("&AGEAYgBj-"), None); // "abc" needn't be encoded
        assert_eq!(decode_utf7("caf\u{e9}"), None); // raw 8-bit
    }

    #[test]
    fn roles_come_from_special_use_and_inbox_from_its_name() {
        assert_eq!(role_of("inbox", &[]), Some(Role::Inbox));
        assert_eq!(
            role_of("Sent Items", &[NameAttribute::Sent]),
            Some(Role::Sent)
        );
        assert_eq!(
            role_of("Junk Mail", &[NameAttribute::Marked, NameAttribute::Junk]),
            Some(Role::Junk)
        );
        assert_eq!(role_of("Sent", &[]), None);
    }

    #[test]
    fn noselect_and_nonexistent_folders_cannot_be_opened() {
        assert!(selectable(&[NameAttribute::Marked]));
        assert!(!selectable(&[NameAttribute::NoSelect]));
        assert!(!selectable(&[NameAttribute::Extension(
            "\\NonExistent".into()
        )]));
    }
}
