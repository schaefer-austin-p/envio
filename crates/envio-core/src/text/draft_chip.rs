//! The words on the chip of a draft the server does not have (#266).

use envio_store::{RefusalKind, ServerCopy};

use crate::i18n::tr;

fn not_uploaded() -> String {
    // TRANSLATORS: chip on a draft that is not on the server; keep to about
    // 20 characters.
    tr("Not uploaded")
}

fn over_quota() -> String {
    // TRANSLATORS: chip, the server refused the draft because the mailbox is
    // full; keep to about 20 characters.
    tr("Mailbox full")
}

fn read_only() -> String {
    // TRANSLATORS: chip, the server does not let drafts be saved in the
    // Drafts folder; keep to about 20 characters.
    tr("Drafts read-only")
}

fn too_large() -> String {
    // TRANSLATORS: chip, the server refused the draft because it is too big;
    // keep to about 20 characters.
    tr("Too large")
}

fn other() -> String {
    // TRANSLATORS: chip, the server refused the draft for another reason;
    // keep to about 20 characters.
    tr("Server refused")
}

/// "Not uploaded", or the reason when the server refused this very save
/// (`failed_ms == updated_at_ms`) and the kind is known. A newer save shows
/// "Not uploaded" again.
pub fn chip_text(server: &ServerCopy, updated_at_ms: i64) -> String {
    let kind = server.refusal.filter(|_| server.failed_ms == updated_at_ms);
    match kind {
        None => not_uploaded(),
        Some(RefusalKind::OverQuota) => over_quota(),
        Some(RefusalKind::ReadOnly) => read_only(),
        Some(RefusalKind::TooLarge) => too_large(),
        Some(RefusalKind::Other) => other(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(kind: Option<RefusalKind>, failed_ms: i64) -> ServerCopy {
        ServerCopy {
            refusal: kind,
            failed_ms,
            failures: 1,
            failed_at_ms: 9,
            ..ServerCopy::default()
        }
    }

    #[test]
    fn each_kind_has_its_words() {
        let text = |kind| chip_text(&refused(Some(kind), 5), 5);
        assert_eq!(text(RefusalKind::OverQuota), "Mailbox full");
        assert_eq!(text(RefusalKind::ReadOnly), "Drafts read-only");
        assert_eq!(text(RefusalKind::TooLarge), "Too large");
        assert_eq!(text(RefusalKind::Other), "Server refused");
    }

    #[test]
    fn no_reason_or_a_newer_save_says_not_uploaded() {
        assert_eq!(chip_text(&ServerCopy::default(), 5), "Not uploaded");
        assert_eq!(chip_text(&refused(None, 5), 5), "Not uploaded");
        let old = refused(Some(RefusalKind::OverQuota), 5);
        assert_eq!(chip_text(&old, 6), "Not uploaded");
    }
}
