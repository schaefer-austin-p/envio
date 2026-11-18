//! The helper's user-facing text (domain `envio.aps`, read by
//! tools/i18n/update.sh).

use crate::card::Text;

fn tr(msgid: &str) -> String {
    gettextrs::dgettext("envio.aps", msgid)
}

fn tr_n(singular: &str, plural: &str, n: u64) -> String {
    let n = u32::try_from(n).unwrap_or(u32::MAX);
    gettextrs::dngettext("envio.aps", singular, plural, n)
}

fn counted(n: u64) -> String {
    // TRANSLATORS: the body of a new-mail card; %d is the unread count.
    tr_n("%d new message", "%d new messages", n).replace("%d", &n.to_string())
}

/// The generic card's words, in the user's language when a catalogue is
/// readable, else English.
pub fn generic() -> Text {
    Text {
        summary: "Envio".into(),
        new_mail: tr("New mail"),
        counted,
    }
}
