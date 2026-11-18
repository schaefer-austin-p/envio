//! Reading one FETCH reply's attributes (what async-imap's `Fetch` gave,
//! now that `checked_fetch` reads the replies itself). Each picks the same
//! attribute `Fetch`'s method of that name did.

use async_imap::imap_proto::{AttributeValue, BodyStructure, MessageSection, SectionPath};
use async_imap::types::Flag;

use super::ops::message_flags;
use super::query::MessageFlags;

pub(super) type Attributes<'a> = &'a [AttributeValue<'a>];

pub(super) fn uid(attributes: Attributes<'_>) -> Option<u32> {
    attributes.iter().find_map(|a| match a {
        AttributeValue::Uid(uid) => Some(*uid),
        _ => None,
    })
}

pub(super) fn size(attributes: Attributes<'_>) -> Option<u32> {
    attributes.iter().find_map(|a| match a {
        AttributeValue::Rfc822Size(size) => Some(*size),
        _ => None,
    })
}

pub(super) fn modseq(attributes: Attributes<'_>) -> Option<u64> {
    attributes.iter().find_map(|a| match a {
        AttributeValue::ModSeq(modseq) => Some(*modseq),
        _ => None,
    })
}

pub(super) fn flags(attributes: Attributes<'_>) -> MessageFlags {
    let names = attributes.iter().find_map(|a| match a {
        AttributeValue::Flags(names) => Some(names),
        _ => None,
    });
    message_flags(
        names
            .into_iter()
            .flatten()
            .map(|name| Flag::from(name.as_ref())),
    )
}

/// `BODY[HEADER]`, `BODY[HEADER.FIELDS ..]` or `RFC822.HEADER`.
pub(super) fn header<'a>(attributes: Attributes<'a>) -> Option<&'a [u8]> {
    attributes.iter().find_map(|a| match a {
        AttributeValue::BodySection {
            section: Some(SectionPath::Full(MessageSection::Header)),
            data: Some(data),
            ..
        }
        | AttributeValue::Rfc822Header(Some(data)) => Some(data.as_ref()),
        _ => None,
    })
}

/// `BODY[]` or `RFC822`.
pub(super) fn body<'a>(attributes: Attributes<'a>) -> Option<&'a [u8]> {
    attributes.iter().find_map(|a| match a {
        AttributeValue::BodySection {
            section: None,
            data: Some(data),
            ..
        }
        | AttributeValue::Rfc822(Some(data)) => Some(data.as_ref()),
        _ => None,
    })
}

/// `BODY[<path>]`.
pub(super) fn section<'a>(attributes: Attributes<'a>, path: &SectionPath) -> Option<&'a [u8]> {
    attributes.iter().find_map(|a| match a {
        AttributeValue::BodySection {
            section: Some(found),
            data: Some(data),
            ..
        } if found == path => Some(data.as_ref()),
        _ => None,
    })
}

pub(super) fn bodystructure<'a>(attributes: Attributes<'a>) -> Option<&'a BodyStructure<'a>> {
    attributes.iter().find_map(|a| match a {
        AttributeValue::BodyStructure(structure) => Some(structure),
        _ => None,
    })
}

/// INTERNALDATE as Unix seconds.
pub(super) fn internal_date(attributes: Attributes<'_>) -> Option<i64> {
    attributes.iter().find_map(|a| match a {
        AttributeValue::InternalDate(text) => parse_date(text),
        _ => None,
    })
}

/// `8-Nov-2026 09:05:00 +0100` (the day may have a leading space or zero)
/// as Unix seconds.
fn parse_date(text: &str) -> Option<i64> {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mut words = text.split_whitespace();
    let mut date = words.next()?.split('-');
    let day: i64 = date.next()?.parse().ok()?;
    let name = date.next()?;
    let month = MONTHS.iter().position(|m| m.eq_ignore_ascii_case(name))? as i64 + 1;
    let year: i64 = date.next()?.parse().ok()?;
    let mut time = words.next()?.split(':');
    let mut part = || time.next()?.parse::<i64>().ok();
    let (hour, minute, second) = (part()?, part()?, part()?);
    let zone = words.next()?;
    let sign = match zone.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let digits: i64 = zone.get(1..)?.parse().ok()?;
    let offset = sign * (digits / 100 * 3_600 + digits % 100 * 60);
    let days = days_from_civil(year, month, day);
    Some(days * 86_400 + hour * 3_600 + minute * 60 + second - offset)
}

/// Days since 1970-01-01 of a proleptic Gregorian date (H. Hinnant).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = year - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_dates_are_read_with_their_zone() {
        assert_eq!(parse_date("08-Nov-2026 10:00:00 +0000"), Some(1_794_132_000));
        assert_eq!(parse_date("8-Nov-2026 11:30:00 +0130"), Some(1_794_132_000));
        assert_eq!(parse_date("08-Nov-2026 05:00:00 -0500"), Some(1_794_132_000));
        assert_eq!(parse_date("01-Jan-1970 00:00:00 +0000"), Some(0));
        assert_eq!(parse_date("nonsense"), None);
    }
}
