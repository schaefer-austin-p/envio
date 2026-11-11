//! A tapped link split for the link sheet: the real host stands apart, what
//! comes before the `@` is flagged, and invisible characters are written out.
//! The sheet only shows these parts; Open and Copy use the link as given.

use envio_core::{link_malformed, userinfo_decoration, written_as};
use envio_mime::link_parts;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinkView {
    /// `scheme://` (for `mailto:`, `mailto:` and the local part with its @);
    /// never the userinfo of an http(s) link.
    pub before_host: String,
    /// Text before the @ of an http(s) link, without the @.
    pub userinfo: String,
    /// The host as people read it (Unicode); empty for other schemes.
    pub host: String,
    /// The ASCII form of the host; empty when it is the same.
    pub punycode: String,
    /// `:port` and the path, query and fragment.
    pub after_host: String,
    /// Translated; empty when there is nothing to flag, else one line per
    /// thing, separated by a line break.
    pub warning: String,
    pub openable: bool,
}

/// Invisible and direction-changing characters, written as `[U+XXXX]`.
fn visible(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '\u{00AD}' | '\u{061C}' | '\u{180E}' | '\u{200B}'..='\u{200F}'
            | '\u{2028}'..='\u{202E}' | '\u{2060}'..='\u{2069}' | '\u{FEFF}')
        {
            out.push_str(&format!("[U+{:04X}]", u32::from(c)));
        } else {
            out.push(c);
        }
    }
    out
}

fn is_web_or_mail(url: &str) -> bool {
    url.split_once(':').is_some_and(|(scheme, _)| {
        ["http", "https", "mailto"]
            .iter()
            .any(|k| scheme.trim().eq_ignore_ascii_case(k))
    })
}

pub fn link_view(url: &str) -> LinkView {
    let Some(parts) = link_parts(url) else {
        // A web or mail link the splitter refuses is not opened; any other
        // scheme is shown whole, as before. (Mail bodies only keep http,
        // https and mailto, so that branch is not reached today; if the
        // sanitizer widens, make it fail closed.)
        return LinkView {
            after_host: visible(url),
            warning: if is_web_or_mail(url) || !url.contains(':') {
                link_malformed()
            } else {
                String::new()
            },
            openable: !(is_web_or_mail(url) || !url.contains(':')),
            ..LinkView::default()
        };
    };
    let mut warnings = Vec::new();
    let (before_host, userinfo) = match (&parts.userinfo, parts.scheme == "mailto") {
        (Some(local), true) => (format!("mailto:{}@", visible(local)), String::new()),
        (None, true) => ("mailto:".to_owned(), String::new()),
        (Some(user), false) => {
            warnings.push(userinfo_decoration(&parts.host));
            (format!("{}://", parts.scheme), visible(user))
        }
        (None, false) => (format!("{}://", parts.scheme), String::new()),
    };
    let punycode = parts.punycode.unwrap_or_default();
    if !punycode.is_empty() {
        warnings.push(written_as(&punycode));
    }
    let port = parts.port.map(|p| format!(":{p}")).unwrap_or_default();
    LinkView {
        before_host,
        userinfo,
        host: parts.host,
        punycode,
        after_host: visible(&format!("{port}{}", parts.rest)),
        warning: warnings.join("\n"),
        openable: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_https_link_has_nothing_to_flag() {
        let v = link_view("https://example.org:8443/a?b=1#c");
        assert_eq!(v.before_host, "https://");
        assert_eq!(v.host, "example.org");
        assert_eq!(v.after_host, ":8443/a?b=1#c");
        assert_eq!((v.userinfo.as_str(), v.punycode.as_str()), ("", ""));
        assert_eq!(v.warning, "");
        assert!(v.openable);
    }

    #[test]
    fn text_before_the_at_is_set_apart_and_the_real_host_named() {
        let v = link_view("https://bank.example@evil.example/login");
        assert_eq!(v.before_host, "https://");
        assert_eq!(v.userinfo, "bank.example");
        assert_eq!(v.host, "evil.example");
        assert_eq!(v.after_host, "/login");
        assert_eq!(
            v.warning,
            "Before the @ is decoration. This link opens evil.example."
        );
        assert!(v.openable);
    }

    #[test]
    fn an_idn_host_shows_unicode_and_its_punycode() {
        let v = link_view("https://xn--bcher-kva.example/");
        assert_eq!(v.host, "b\u{fc}cher.example");
        assert_eq!(v.punycode, "xn--bcher-kva.example");
        assert_eq!(
            v.warning,
            "The address is written as xn--bcher-kva.example."
        );
    }

    #[test]
    fn both_warnings_may_apply_one_per_line() {
        let v = link_view("https://a@xn--bcher-kva.example/");
        assert_eq!(v.warning.lines().count(), 2);
    }

    #[test]
    fn a_mailto_link_is_not_flagged_for_its_local_part() {
        let v = link_view("mailto:ada@example.org?subject=hi");
        assert_eq!(v.before_host, "mailto:ada@");
        assert_eq!(v.userinfo, "");
        assert_eq!(v.host, "example.org");
        assert_eq!(v.after_host, "?subject=hi");
        assert_eq!(v.warning, "");
        assert!(v.openable);
    }

    #[test]
    fn another_scheme_is_shown_whole_without_a_host() {
        let v = link_view("tel:+15551234");
        assert_eq!(v.host, "");
        assert_eq!(v.after_host, "tel:+15551234");
        assert!(v.openable);
    }

    #[test]
    fn a_malformed_web_link_cannot_be_opened() {
        let v = link_view("https://exa mple.org/");
        assert!(!v.openable);
        assert_eq!(v.host, "");
        assert_eq!(v.after_host, "https://exa mple.org/");
        assert_eq!(v.warning, "This link looks malformed and can't be opened.");
    }

    #[test]
    fn invisible_characters_are_written_out_and_never_applied() {
        let v = link_view("https://good.example/\u{202E}format.exe");
        assert_eq!(v.after_host, "/[U+202E]format.exe");
        assert!(!v.after_host.contains('\u{202E}'));
    }
}
