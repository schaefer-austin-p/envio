//! The arguments of `org.freedesktop.Notifications.Notify` for an alert.
//! The C++ side (qt/notify.cpp) only puts them into the call.

use envio_core::Alert;

const APP_NAME: &str = "Envio";
/// Shown when no icon file could be put on disk.
const FALLBACK_ICON: &str = "mail-unread";
const CATEGORY: &str = "email.arrived";
/// The desktop file's id without `.desktop` (envio.desktop in the repo
/// root, which click installs under the same name), so servers group and
/// name the cards as Envio.
const DESKTOP_ENTRY: &str = "envio";
/// The spec's "normal" urgency.
const URGENCY_NORMAL: u8 = 1;
/// The same cap as the Postal card (`Alert::to_postal_json`).
const LIMIT: usize = 200;

/// Whether the server renders the spec's body markup (capability
/// `body-markup`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Markup {
    Yes,
    No,
    /// The capabilities are not known (yet): escape, never trust mail text.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NotifyArgs {
    pub app_name: &'static str,
    pub replaces_id: u32,
    /// A `file://` URI of the app icon, else the theme name `mail-unread`.
    pub icon: String,
    pub summary: String,
    pub body: String,
    pub category: &'static str,
    pub desktop_entry: &'static str,
    pub urgency: u8,
    /// The `image-path` hint (the same URI as `icon`); empty: no hint.
    pub image_path: String,
}

impl NotifyArgs {
    /// `markup`: with `Yes` or `Unknown` the body is escaped (a server that
    /// reads markup must not see mail's `<b>` as its own); `No` is plain
    /// text. The summary (the sender) is never echoed in the body: servers
    /// draw it bold already. `icon_file`: the app icon's absolute path, when there is
    /// one.
    /// The actions (`["default", ""]`: a click on the card) and the timeout
    /// (-1: the server's default) never vary, so C++ holds them.
    pub(crate) fn from_alert(
        alert: &Alert,
        replaces_id: u32,
        markup: Markup,
        icon_file: Option<&str>,
    ) -> Self {
        let summary = limit(&alert.summary);
        let body = limit(&alert.body);
        let body = match markup {
            Markup::Yes | Markup::Unknown => escape_markup(&body),
            Markup::No => body,
        };
        let uri = icon_file.map(file_uri);
        Self {
            app_name: APP_NAME,
            replaces_id,
            icon: uri.clone().unwrap_or_else(|| FALLBACK_ICON.to_owned()),
            summary,
            body,
            category: CATEGORY,
            desktop_entry: DESKTOP_ENTRY,
            urgency: URGENCY_NORMAL,
            image_path: uri.unwrap_or_default(),
        }
    }
}

fn limit(text: &str) -> String {
    text.chars().take(LIMIT).collect()
}

/// `file://` plus the absolute path, percent-encoded outside the unreserved
/// characters and `/`.
fn file_uri(path: &str) -> String {
    let mut out = String::from("file://");
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Servers that take the spec's body markup would read a sender's `<b>` as
/// markup; mail text is plain text.
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

    fn plain(summary: &str, body: &str) -> NotifyArgs {
        NotifyArgs::from_alert(&alert(summary, body), 0, Markup::No, None)
    }

    fn marked(summary: &str, body: &str) -> NotifyArgs {
        NotifyArgs::from_alert(&alert(summary, body), 0, Markup::Yes, None)
    }

    #[test]
    fn the_fixed_fields_and_the_replaced_id_are_set() {
        let a = NotifyArgs::from_alert(&alert("Ana", "Lunch?"), 41, Markup::No, None);
        assert_eq!(a.app_name, "Envio");
        assert_eq!(a.replaces_id, 41);
        assert_eq!(a.summary, "Ana");
        assert_eq!(a.body, "Lunch?");
    }

    #[test]
    fn the_hints_are_category_urgency_normal_and_the_desktop_entry() {
        let a = plain("Ana", "Lunch?");
        assert_eq!(a.category, "email.arrived");
        assert_eq!(a.urgency, 1);
        assert_eq!(a.desktop_entry, "envio");
    }

    #[test]
    fn the_icon_is_a_file_uri_and_the_image_path_hint_else_the_theme_name() {
        let a = NotifyArgs::from_alert(&alert("a", "b"), 0, Markup::No, Some("/home/a b/c.svg"));
        assert_eq!(a.icon, "file:///home/a%20b/c.svg");
        assert_eq!(a.image_path, a.icon);
        let a = plain("a", "b");
        assert_eq!(a.icon, "mail-unread");
        assert_eq!(a.image_path, "");
    }

    #[test]
    fn summary_and_body_are_cut_at_200_characters_not_bytes() {
        let long = "é".repeat(300);
        let a = plain(&long, &long);
        assert_eq!(a.summary.chars().count(), 200);
        assert_eq!(a.body.chars().count(), 200);
    }

    #[test]
    fn the_body_never_repeats_the_summary() {
        // Servers draw the summary bold already; the sender is not echoed.
        for m in [Markup::Yes, Markup::No, Markup::Unknown] {
            let a = NotifyArgs::from_alert(&alert("Ana", "Lunch?"), 0, m, None);
            assert_eq!(a.body, "Lunch?");
            assert_eq!(a.summary, "Ana");
        }
    }

    #[test]
    fn with_markup_everything_from_mail_is_escaped() {
        let a = marked("<a&b>", "<b>x</b> & y");
        assert_eq!(a.body, "&lt;b&gt;x&lt;/b&gt; &amp; y");
        // The summary is plain text for every server.
        assert_eq!(a.summary, "<a&b>");
        // The cut is on the text, so an entity is never split.
        let a = marked("s", &"<".repeat(300));
        assert_eq!(a.body, "&lt;".repeat(200));
    }

    #[test]
    fn without_markup_the_body_is_the_text_as_is() {
        let a = plain("<a&b>", "<b>x</b> & y");
        assert_eq!(a.body, "<b>x</b> & y");
    }

    #[test]
    fn unknown_capabilities_escape() {
        let a = NotifyArgs::from_alert(&alert("<a&b>", "<i>x</i>"), 0, Markup::Unknown, None);
        assert_eq!(a.body, "&lt;i&gt;x&lt;/i&gt;");
    }
}
