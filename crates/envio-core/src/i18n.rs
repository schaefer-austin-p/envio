//! Translation lookups for the core's user-facing text (domain `envio.aps`).

pub const DOMAIN: &str = "envio.aps";

/// The translation of `msgid`, or `msgid` itself without a catalogue.
pub fn tr(msgid: &str) -> String {
    gettextrs::dgettext(DOMAIN, msgid)
}

/// The singular or plural translation for `n`.
pub fn tr_n(singular: &str, plural: &str, n: u64) -> String {
    let n = u32::try_from(n).unwrap_or(u32::MAX);
    gettextrs::dngettext(DOMAIN, singular, plural, n)
}

/// Replaces `{name}` with the matching value. Unknown names stay as they
/// are; values go in verbatim and are never scanned again.
pub fn fill(template: &str, args: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let hit = after.find('}').and_then(|close| {
            let name = &after[..close];
            args.iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| (close, *v))
        });
        match hit {
            Some((close, value)) => {
                out.push_str(value);
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_replaces_named_placeholders() {
        let s = fill("{n} of {total} in {n}", &[("n", "3"), ("total", "9")]);
        assert_eq!(s, "3 of 9 in 3");
    }

    #[test]
    fn fill_leaves_unknown_placeholders() {
        assert_eq!(fill("a {x} b {y", &[("z", "1")]), "a {x} b {y");
    }

    #[test]
    fn fill_does_not_expand_inside_values() {
        let s = fill("{a} {b}", &[("a", "{b}"), ("b", "B")]);
        assert_eq!(s, "{b} B");
    }

    #[test]
    fn tr_returns_the_msgid_without_a_catalogue() {
        assert_eq!(tr("Inbox"), "Inbox");
        assert_eq!(tr_n("one message", "many messages", 1), "one message");
        assert_eq!(
            tr_n("one message", "many messages", u64::MAX),
            "many messages"
        );
    }
}
