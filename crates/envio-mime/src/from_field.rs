//! The one `From:` field of a message (plan #285, decision 6). DKIM's
//! classic attack is a second `From:` field: the signature covers the lower
//! one, the reader shows the upper one. A message with two, or one with
//! several mailboxes, is never verified, by either path.

use mail_parser::{Address, HeaderName, MessageParser};

use crate::dkim::{field_named, header_fields};

/// The domain of the only mailbox in the only `From:` field of `raw`'s
/// header, lowercased, without a trailing dot; None for zero or several
/// From fields, several mailboxes, a group, or no domain.
pub(crate) fn single_from_domain(raw: &[u8]) -> Option<String> {
    let mut froms = header_fields(raw)
        .into_iter()
        .filter(|f| field_named(f, b"from"));
    let field = froms.next()?;
    if froms.next().is_some() {
        return None;
    }
    let parsed = MessageParser::default().parse_headers(field)?;
    let Some(Address::List(list)) = parsed
        .header_values(HeaderName::From)
        .next()
        .and_then(|v| v.as_address())
    else {
        return None;
    };
    let [mailbox] = list.as_slice() else {
        return None;
    };
    let (_, domain) = mailbox.address()?.rsplit_once('@')?;
    let domain = domain.trim().trim_end_matches('.').to_ascii_lowercase();
    (!domain.is_empty()).then_some(domain)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn domain(header: &str) -> Option<String> {
        single_from_domain(format!("{header}Subject: x\r\n\r\nbody\r\n").as_bytes())
    }

    #[test]
    fn one_mailbox_gives_its_domain() {
        assert_eq!(
            domain("From: Bob <bob@Example.COM>\r\n").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            domain("From: bob@example.com\r\n").as_deref(),
            Some("example.com")
        );
    }

    #[test]
    fn a_folded_and_quoted_field_still_reads() {
        assert_eq!(
            domain("From: \"Bob, the <b@evil.net>\"\r\n <bob@example.com>\r\n").as_deref(),
            Some("example.com")
        );
    }

    #[test]
    fn two_from_fields_give_none() {
        assert_eq!(
            domain("From: a@example.com\r\nFrom: b@example.com\r\n"),
            None
        );
        assert_eq!(
            domain("From: a@example.com\r\nfrom: b@example.net\r\n"),
            None
        );
    }

    #[test]
    fn several_mailboxes_or_a_group_give_none() {
        assert_eq!(domain("From: a@example.com, b@example.net\r\n"), None);
        assert_eq!(domain("From: team: a@example.com;\r\n"), None);
    }

    #[test]
    fn no_from_or_no_domain_gives_none() {
        assert_eq!(domain("To: a@example.com\r\n"), None);
        assert_eq!(domain("From: bob\r\n"), None);
        assert_eq!(domain(""), None);
    }

    #[test]
    fn a_from_in_the_body_does_not_count() {
        let raw = b"From: a@example.com\r\n\r\nFrom: b@example.net\r\n";
        assert_eq!(single_from_domain(raw).as_deref(), Some("example.com"));
    }
}
