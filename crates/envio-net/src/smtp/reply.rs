//! SMTP replies (RFC 5321 4.2) for the part before TLS, which envio
//! speaks itself, and the mapping from reply codes to `NetError`.

use crate::error::NetError;

/// Longest reply line accepted; RFC 5321 allows 512 octets.
pub(crate) const MAX_LINE: usize = 1024;
/// Most lines in one reply. EHLO lists maybe 15 extensions.
pub(crate) const MAX_LINES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Reply {
    pub code: u16,
    /// The text of each line, without code and separator.
    pub lines: Vec<String>,
}

/// One reply line: its code, whether more lines follow, and its text.
pub(crate) fn parse_line(line: &str) -> Option<(u16, bool, &str)> {
    let line = line.strip_suffix('\n').unwrap_or(line);
    let line = line.strip_suffix('\r').unwrap_or(line);
    let digits = line.get(..3)?;
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let code: u16 = digits.parse().ok()?;
    if !(200..600).contains(&code) {
        return None;
    }
    match line.as_bytes().get(3) {
        None => Some((code, false, "")),
        Some(b' ') => Some((code, false, &line[4..])),
        Some(b'-') => Some((code, true, &line[4..])),
        Some(_) => None,
    }
}

/// Whether an EHLO reply offers STARTTLS. The first line is the greeting,
/// so only the extension lines count.
pub(crate) fn has_starttls(ehlo: &Reply) -> bool {
    ehlo.lines.iter().skip(1).any(|line| {
        line.split_ascii_whitespace()
            .next()
            .is_some_and(|word| word.eq_ignore_ascii_case("STARTTLS"))
    })
}

/// What a failing reply means for the caller. 4xx is "try later"; 5xx
/// during sign-in is a bad password; any other 5xx refuses this message.
pub(crate) fn error_for(code: u16, text: String, signing_in: bool) -> NetError {
    match code {
        400..=499 => NetError::Busy(format!("{code} {text}")),
        // 535: credentials invalid; 534/538: mechanism or TLS needed.
        500..=599 if signing_in => NetError::SignIn(format!("{code} {text}")),
        500..=599 => NetError::Refused(format!("{code} {text}")),
        _ => NetError::Protocol(format!("unexpected reply {code} {text}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_split_into_code_continuation_and_text() {
        assert_eq!(
            parse_line("250-STARTTLS\r\n"),
            Some((250, true, "STARTTLS"))
        );
        assert_eq!(parse_line("250 OK\r\n"), Some((250, false, "OK")));
        assert_eq!(parse_line("220\r\n"), Some((220, false, "")));
    }

    #[test]
    fn garbage_is_not_a_reply() {
        assert_eq!(parse_line("hello\r\n"), None);
        assert_eq!(parse_line("25"), None);
        assert_eq!(parse_line("250x"), None);
        assert_eq!(parse_line("+50 OK"), None);
        assert_eq!(parse_line("700 OK"), None);
    }

    #[test]
    fn starttls_counts_only_as_an_extension_line() {
        let reply = |lines: &[&str]| Reply {
            code: 250,
            lines: lines.iter().map(|s| s.to_string()).collect(),
        };
        assert!(has_starttls(&reply(&[
            "mx greets you",
            "PIPELINING",
            "starttls"
        ])));
        assert!(!has_starttls(&reply(&["STARTTLS ready", "PIPELINING"])));
        assert!(!has_starttls(&reply(&["mx", "STARTTLSX"])));
    }

    #[test]
    fn reply_codes_become_error_kinds() {
        assert!(matches!(
            error_for(421, "busy".into(), false),
            NetError::Busy(_)
        ));
        assert!(matches!(
            error_for(535, "no".into(), true),
            NetError::SignIn(_)
        ));
        assert!(matches!(
            error_for(550, "no".into(), false),
            NetError::Refused(_)
        ));
        assert!(matches!(
            error_for(250, "ok".into(), false),
            NetError::Protocol(_)
        ));
    }
}
