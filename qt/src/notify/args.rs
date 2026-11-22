//! The arguments of `org.freedesktop.Notifications.Notify` for an alert.
//! The C++ side (qt/notify.cpp) only puts them into the call.

use envio_core::Alert;

const APP_NAME: &str = "Envio";
const ICON: &str = "mail-unread";
const CATEGORY: &str = "email.arrived";
/// The same cap as the Postal card (`Alert::to_postal_json`).
const LIMIT: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NotifyArgs {
    pub app_name: &'static str,
    pub replaces_id: u32,
    pub icon: &'static str,
    pub summary: String,
    pub body: String,
    pub category: &'static str,
}

impl NotifyArgs {
    /// `markup`: the server renders the spec's body markup (capability
    /// `body-markup`), so the plain-text body is escaped; otherwise it is
    /// sent as is.
    /// The actions (`["default", ""]`: a click on the card) and the timeout
    /// (-1: the server's default) never vary, so C++ holds them.
    pub(crate) fn from_alert(alert: &Alert, replaces_id: u32, markup: bool) -> Self {
        Self {
            app_name: APP_NAME,
            replaces_id,
            icon: ICON,
            summary: limit(&alert.summary),
            body: if markup {
                escape_markup(&limit(&alert.body))
            } else {
                limit(&alert.body)
            },
            category: CATEGORY,
        }
    }
}

fn limit(text: &str) -> String {
    text.chars().take(LIMIT).collect()
}

/// Servers that take the spec's body markup would read a sender's `<b>` as
/// markup; the body is plain text.
fn escape_markup(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alert(summary: &str, body: &str) -> Alert {
        Alert {
            tag: "new-1700".into(),
            summary: summary.into(),
            body: body.into(),
            account: "1700".into(),
        }
    }

    #[test]
    fn the_fixed_fields_and_the_replaced_id_are_set() {
        let a = NotifyArgs::from_alert(&alert("Ana", "Lunch?"), 41, true);
        assert_eq!(a.app_name, "Envio");
        assert_eq!(a.replaces_id, 41);
        assert_eq!(a.icon, "mail-unread");
        assert_eq!(a.category, "email.arrived");
        assert_eq!(a.summary, "Ana");
        assert_eq!(a.body, "Lunch?");
    }

    #[test]
    fn summary_and_body_are_cut_at_200_characters_not_bytes() {
        let long = "é".repeat(300);
        let a = NotifyArgs::from_alert(&alert(&long, &long), 0, true);
        assert_eq!(a.summary.chars().count(), 200);
        assert_eq!(a.body.chars().count(), 200);
    }

    #[test]
    fn the_body_is_escaped_for_markup_after_the_cut() {
        let a = NotifyArgs::from_alert(&alert("a<b", "<b>x</b> & y"), 0, true);
        assert_eq!(a.body, "&lt;b&gt;x&lt;/b&gt; &amp; y");
        assert_eq!(a.summary, "a<b");
        // The cut is on the text, so an entity is never split.
        let body = "<".repeat(300);
        let a = NotifyArgs::from_alert(&alert("s", &body), 0, true);
        assert_eq!(a.body, "&lt;".repeat(200));
    }

    #[test]
    fn a_server_without_body_markup_gets_the_text_unescaped() {
        let a = NotifyArgs::from_alert(&alert("s", "<b>x</b> & y"), 0, false);
        assert_eq!(a.body, "<b>x</b> & y");
    }
}
