//! The final refusal check: a fixture that still holds anything that could
//! identify the test account is never written. The reasons name the kind of
//! leak, never the leaked text.

use super::redact::{emails, guids, ip_literals};

/// What the account is called, from its own sign-in and `/me`. Kept in
/// memory only, to look for these in the redacted output.
#[derive(Default)]
pub struct Own {
    pub address: Vec<String>,
    pub display_name: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    OwnAddress,
    OwnLocalPart,
    OwnName,
    ConsumerDomain,
    Guid,
    IpLiteral,
}

impl Refusal {
    pub fn describe(&self) -> &'static str {
        match self {
            Self::OwnAddress => "the account's address",
            Self::OwnLocalPart => "the local part of the account's address",
            Self::OwnName => "the account's display name",
            Self::ConsumerDomain => "an outlook, hotmail or live address",
            Self::Guid => "a GUID other than the consumer tenant",
            Self::IpLiteral => "an IP literal",
        }
    }
}

/// Terms shorter than this are only checked as whole addresses: a one- or
/// two-letter name would match inside any word.
const MIN_TERM: usize = 3;

/// Ok if `text` (a file's content, as written) is clean. `consumer_tid` is
/// the one GUID allowed.
pub fn refuse(text: &str, own: &Own, consumer_tid: &str) -> Result<(), Refusal> {
    let lower = text.to_ascii_lowercase();
    for addr in own.address.iter().map(|a| a.to_ascii_lowercase()) {
        if !addr.is_empty() && lower.contains(&addr) {
            return Err(Refusal::OwnAddress);
        }
        let local = addr.split('@').next().unwrap_or_default();
        if local.len() >= MIN_TERM && lower.contains(local) {
            return Err(Refusal::OwnLocalPart);
        }
    }
    if let Some(name) = &own.display_name {
        let name = name.trim().to_ascii_lowercase();
        if name.len() >= MIN_TERM && lower.contains(&name) {
            return Err(Refusal::OwnName);
        }
    }
    for domain in ["@outlook.", "@hotmail.", "@live."] {
        if lower.contains(domain) {
            return Err(Refusal::ConsumerDomain);
        }
    }
    if guids(text)
        .iter()
        .any(|r| !text[r.clone()].eq_ignore_ascii_case(consumer_tid))
    {
        return Err(Refusal::Guid);
    }
    if !ip_literals(text).is_empty() {
        return Err(Refusal::IpLiteral);
    }
    // Any address that is not an example.org one is a leak too.
    if emails(text).iter().any(|r| {
        !text[r.clone()]
            .to_ascii_lowercase()
            .ends_with("@example.org")
    }) {
        return Err(Refusal::OwnAddress);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TID: &str = "9188040d-6c67-4c5b-b112-36a304b66dad";

    fn own() -> Own {
        Own {
            address: vec!["Jane.Doe@outlook.com".into()],
            display_name: Some("Jane Doe".into()),
        }
    }

    #[test]
    fn clean_text_and_the_consumer_tid_pass() {
        let text =
            format!(r#"{{"tid":"{TID}","a":"alice@example.org","d":"2026-11-21T10:20:30Z"}}"#);
        assert_eq!(refuse(&text, &own(), TID), Ok(()));
    }

    #[test]
    fn each_leak_is_refused() {
        let cases = [
            ("jane.doe@outlook.com", Refusal::OwnAddress),
            ("hello jane.doe", Refusal::OwnLocalPart),
            ("by JANE DOE", Refusal::OwnName),
            ("x@hotmail.fr", Refusal::ConsumerDomain),
            ("x@live.com", Refusal::ConsumerDomain),
            ("id 12345678-1234-1234-1234-123456789abc", Refusal::Guid),
            ("from 192.168.1.20 by", Refusal::IpLiteral),
            ("from [2001:db8::1] by", Refusal::IpLiteral),
            ("bob@example.com", Refusal::OwnAddress),
        ];
        for (text, want) in cases {
            assert_eq!(refuse(text, &own(), TID), Err(want), "{text}");
        }
    }

    #[test]
    fn a_short_name_does_not_match_inside_words() {
        let own = Own {
            address: vec!["al@outlook.com".into()],
            display_name: Some("Al".into()),
        };
        assert_eq!(refuse("a talent and a ballad", &own, TID), Ok(()));
    }

    #[test]
    fn refusals_never_quote_the_leak() {
        let r = refuse("jane.doe@outlook.com", &own(), TID).unwrap_err();
        assert!(!r.describe().contains("jane"));
    }
}
