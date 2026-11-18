//! The helper's user-facing text (domain `envio.aps`, read by
//! tools/i18n/update.sh).

use crate::card::Text;

fn tr(msgid: &str) -> String {
    gettextrs::dgettext("envio.aps", msgid)
}

/// The generic card's words, in the user's language when a catalogue is
/// readable, else English.
pub fn generic() -> Text {
    Text {
        summary: "Envio".into(),
        body: tr("New mail"),
    }
}
