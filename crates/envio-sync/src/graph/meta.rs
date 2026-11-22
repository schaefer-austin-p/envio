//! What a delta item says about a message, as the map stores it: the flag
//! bits and the categories. Pure.

use envio_net::graph::model::GChange;
use envio_store::Flags;

/// The state after `change`: a property the item does not carry keeps its
/// old value (a read flip arrives as `id` and `isRead` only).
pub(super) fn merged(old: &(u8, Vec<String>), change: &GChange) -> (u8, Vec<String>) {
    let mut bits = old.0;
    for (value, flag) in [
        (change.is_read, Flags::SEEN),
        (change.flagged, Flags::FLAGGED),
        (change.is_draft, Flags::DRAFT),
    ] {
        let bit = u8::from(flag);
        match value {
            Some(true) => bits |= bit,
            Some(false) => bits &= !bit,
            None => {}
        }
    }
    let categories = change.categories.clone().unwrap_or_else(|| old.1.clone());
    (bits, categories)
}

/// Folds a later item for the same id into an earlier one.
pub(super) fn fold(into: &mut GChange, later: GChange) {
    into.received_ms = later.received_ms.or(into.received_ms);
    into.is_read = later.is_read.or(into.is_read);
    into.flagged = later.flagged.or(into.flagged);
    into.is_draft = later.is_draft.or(into.is_draft);
    into.categories = later.categories.or(into.categories.take());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(read: Option<bool>) -> GChange {
        GChange {
            id: "m".into(),
            received_ms: None,
            is_read: read,
            flagged: None,
            categories: None,
            is_draft: None,
        }
    }

    #[test]
    fn a_missing_property_keeps_the_old_value() {
        let old = (u8::from(Flags::FLAGGED), vec!["Red".to_string()]);
        let got = merged(&old, &change(Some(true)));
        assert_eq!(
            got,
            (
                u8::from(Flags::FLAGGED) | u8::from(Flags::SEEN),
                old.1.clone()
            )
        );
        assert_eq!(merged(&old, &change(None)), old);
    }

    #[test]
    fn a_later_item_overrides_only_what_it_carries() {
        let mut first = change(Some(false));
        first.flagged = Some(true);
        fold(&mut first, change(Some(true)));
        assert_eq!((first.is_read, first.flagged), (Some(true), Some(true)));
    }
}
