//! Building IMAP command arguments: UID sets, quoted strings, flag lists
//! and search keys. Anything that could smuggle a second command (CR, LF,
//! NUL) is refused here, before it reaches the wire.

use crate::error::{NetError, Result};

/// A message flag envio reads and sets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Flag {
    Seen,
    Answered,
    Flagged,
    Deleted,
    Draft,
    /// A keyword such as `$Forwarded` or `$Junk`.
    Keyword(String),
}

/// The flags on one message.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MessageFlags {
    pub seen: bool,
    pub answered: bool,
    pub flagged: bool,
    pub deleted: bool,
    pub draft: bool,
    pub keywords: Vec<String>,
}

impl MessageFlags {
    pub fn from_flags(flags: impl IntoIterator<Item = Flag>) -> Self {
        let mut out = Self::default();
        for flag in flags {
            match flag {
                Flag::Seen => out.seen = true,
                Flag::Answered => out.answered = true,
                Flag::Flagged => out.flagged = true,
                Flag::Deleted => out.deleted = true,
                Flag::Draft => out.draft = true,
                Flag::Keyword(word) => out.keywords.push(word),
            }
        }
        out
    }
}

/// `1:3,7,9:10` for UIDs in any order; None for no UIDs.
pub fn uid_set(uids: &[u32]) -> Option<String> {
    let mut sorted = uids.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut parts = Vec::new();
    let mut iter = sorted.into_iter().peekable();
    while let Some(start) = iter.next() {
        let mut end = start;
        while let Some(next) = iter.next_if(|&uid| Some(uid) == end.checked_add(1)) {
            end = next;
        }
        parts.push(if start == end {
            start.to_string()
        } else {
            format!("{start}:{end}")
        });
    }
    (!parts.is_empty()).then(|| parts.join(","))
}

/// An IMAP quoted string. Non-ASCII goes through [`search_text`] instead.
pub fn quote(text: &str) -> Result<String> {
    if text
        .bytes()
        .any(|b| matches!(b, b'\r' | b'\n' | 0) || !b.is_ascii())
    {
        return Err(NetError::Refused(
            "text with line breaks or non-ASCII can't be quoted".into(),
        ));
    }
    Ok(format!(
        "\"{}\"",
        text.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

/// `(\Seen $Forwarded)`.
pub fn flag_list(flags: &[Flag]) -> Result<String> {
    let mut words = Vec::with_capacity(flags.len());
    for flag in flags {
        words.push(match flag {
            Flag::Seen => "\\Seen".to_owned(),
            Flag::Answered => "\\Answered".to_owned(),
            Flag::Flagged => "\\Flagged".to_owned(),
            Flag::Deleted => "\\Deleted".to_owned(),
            Flag::Draft => "\\Draft".to_owned(),
            Flag::Keyword(word) if is_atom(word) => word.clone(),
            Flag::Keyword(word) => {
                return Err(NetError::Refused(format!("not a keyword: {word:?}")));
            }
        });
    }
    Ok(format!("({})", words.join(" ")))
}

/// RFC 3501 atom, minus the characters with meaning in flags.
fn is_atom(word: &str) -> bool {
    !word.is_empty()
        && word.bytes().all(|b| {
            b.is_ascii_graphic()
                && !matches!(b, b'(' | b')' | b'{' | b'%' | b'*' | b'"' | b'\\' | b']')
        })
}

/// The FETCH items for a message summary.
pub fn summary_items(condstore: bool) -> &'static str {
    if condstore {
        "(UID FLAGS RFC822.SIZE INTERNALDATE MODSEQ BODY.PEEK[HEADER])"
    } else {
        "(UID FLAGS RFC822.SIZE INTERNALDATE BODY.PEEK[HEADER])"
    }
}

/// The flags-only FETCH items; with `changed_since`, only messages whose
/// flags changed after that mod-sequence (RFC 7162 CONDSTORE).
pub fn flag_items(changed_since: Option<u64>) -> String {
    match changed_since {
        Some(modseq) => format!("(UID FLAGS MODSEQ) (CHANGEDSINCE {modseq})"),
        None => "(UID FLAGS)".to_owned(),
    }
}

/// The search key for one Message-ID header.
pub fn search_message_id(message_id: &str) -> Result<String> {
    Ok(format!("HEADER Message-ID {}", quote(message_id)?))
}

/// The Message-ID in a `BODY[HEADER.FIELDS (MESSAGE-ID)]` answer: the
/// header name in any case, folded lines joined, spaces trimmed.
pub fn message_id_of(fields: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(fields);
    let mut value: Option<String> = None;
    for line in text.split("\r\n") {
        if line.starts_with([' ', '\t']) {
            if let Some(value) = value.as_mut() {
                value.push_str(line.trim());
            }
            continue;
        }
        if value.is_some() {
            break;
        }
        if let Some((name, rest)) = line.split_once(':')
            && name.trim().eq_ignore_ascii_case("message-id")
        {
            value = Some(rest.trim().to_owned());
        }
    }
    value.filter(|id| !id.is_empty())
}

/// Whether two Message-IDs name the same message. Surrounding whitespace
/// and one pair of angle brackets don't count: `<id@x>` and `id@x` match.
pub fn same_message_id(found: &str, wanted: &str) -> bool {
    fn bare(text: &str) -> &str {
        let text = text.trim();
        text.strip_prefix('<')
            .and_then(|inner| inner.strip_suffix('>'))
            .unwrap_or(text)
            .trim()
    }
    let (found, wanted) = (bare(found), bare(wanted));
    !wanted.is_empty() && found == wanted
}

/// A full-text search. ASCII is quoted; anything else needs a literal,
/// sent non-synchronizing (`{n+}`), which needs LITERAL+ or LITERAL-.
pub fn search_text(text: &str, literal_plus: bool) -> Result<String> {
    if text.is_ascii() {
        return Ok(format!("TEXT {}", quote(text)?));
    }
    if text.contains(['\r', '\n', '\0']) {
        return Err(NetError::Refused("search text with line breaks".into()));
    }
    if !literal_plus {
        return Err(NetError::Refused(
            "this server can't search for non-ASCII text".into(),
        ));
    }
    Ok(format!("CHARSET UTF-8 TEXT {{{}+}}\r\n{text}", text.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uid_sets_collapse_runs() {
        assert_eq!(
            uid_set(&[7, 1, 2, 3, 9, 10, 3]).as_deref(),
            Some("1:3,7,9:10")
        );
        assert_eq!(uid_set(&[5]).as_deref(), Some("5"));
        assert_eq!(uid_set(&[]), None);
        assert_eq!(
            uid_set(&[u32::MAX - 1, u32::MAX]).as_deref(),
            Some("4294967294:4294967295")
        );
    }

    #[test]
    fn quoting_escapes_and_refuses_line_breaks() {
        assert_eq!(quote(r#"a"b\c"#).unwrap(), r#""a\"b\\c""#);
        assert!(quote("x\r\nA1 DELETE INBOX").is_err());
        assert!(quote("caf\u{e9}").is_err());
    }

    #[test]
    fn flag_lists_refuse_keywords_that_are_not_atoms() {
        let flags = [Flag::Seen, Flag::Keyword("$Forwarded".into())];
        assert_eq!(flag_list(&flags).unwrap(), r"(\Seen $Forwarded)");
        assert!(flag_list(&[Flag::Keyword("two words".into())]).is_err());
        assert!(flag_list(&[Flag::Keyword("a)b".into())]).is_err());
        assert_eq!(flag_list(&[]).unwrap(), "()");
    }

    #[test]
    fn condstore_items_ask_only_for_changes() {
        assert_eq!(flag_items(None), "(UID FLAGS)");
        assert_eq!(flag_items(Some(42)), "(UID FLAGS MODSEQ) (CHANGEDSINCE 42)");
        assert!(summary_items(true).contains("MODSEQ"));
        assert!(!summary_items(false).contains("MODSEQ"));
    }

    #[test]
    fn non_ascii_search_needs_a_literal() {
        assert_eq!(search_text("hello", false).unwrap(), "TEXT \"hello\"");
        assert_eq!(
            search_text("Grüße", true).unwrap(),
            "CHARSET UTF-8 TEXT {7+}\r\nGrüße"
        );
        assert!(search_text("Grüße", false).is_err());
        assert!(search_text("Grüße\r\nA1 LOGOUT", true).is_err());
    }

    #[test]
    fn the_message_id_is_read_from_header_fields() {
        assert_eq!(
            message_id_of(b"Message-ID: <a.1@x.test>\r\n\r\n").as_deref(),
            Some("<a.1@x.test>")
        );
        assert_eq!(
            message_id_of(b"message-id:\r\n  <folded@x.test>\r\n\r\n").as_deref(),
            Some("<folded@x.test>")
        );
        assert_eq!(message_id_of(b"\r\n"), None);
        assert_eq!(message_id_of(b"Message-ID:   \r\n\r\n"), None);
    }

    #[test]
    fn message_ids_match_with_or_without_angle_brackets() {
        assert!(same_message_id("<id@x>", "id@x"));
        assert!(same_message_id("id@x", "<id@x>"));
        assert!(same_message_id(" <id@x> ", "<id@x>"));
        assert!(!same_message_id("<id@x>", "<other@x>"));
        assert!(!same_message_id("<<id@x>>", "id@x"));
        assert!(!same_message_id("<>", ""));
    }

    #[test]
    fn message_flags_gather_system_flags_and_keywords() {
        let flags = MessageFlags::from_flags([Flag::Seen, Flag::Keyword("$Junk".into())]);
        assert!(flags.seen && !flags.flagged);
        assert_eq!(flags.keywords, ["$Junk"]);
    }
}
