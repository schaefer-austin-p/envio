//! The Undo book: offers that can be taken back until their window ends.
//! Pure: the worker passes the time in.

use std::collections::{BTreeMap, BTreeSet};

use envio_store::{FolderId, InviteAnswer, MessageRowId};

use crate::api::command::UndoId;

/// Something the user may take back until `expires_at_ms`.
#[derive(Debug)]
pub struct Offer {
    pub what: Offered,
    pub expires_at_ms: i64,
}

#[derive(Debug)]
pub enum Offered {
    /// Actions on messages, and what to refresh when they are undone.
    Actions {
        undos: Vec<envio_sync::driver::Undo>,
        accounts: BTreeSet<String>,
        folders: BTreeSet<FolderId>,
    },
    /// Local drafts the user deleted, held until the window ends: their
    /// files go then, and an Undo puts their rows back. `rest` is the
    /// server-side half of the same Act, if it had one.
    LocalDrafts {
        ids: Vec<String>,
        rest: Option<Box<Offered>>,
    },
    /// An invite answer: its reply waits, held, in the Outbox.
    Answer {
        message: MessageRowId,
        outbox_item: String,
        account: String,
        uid: String,
        recurrence_id: String,
        /// The answer this one replaced, to restore.
        previous: Option<InviteAnswer>,
    },
}

#[derive(Debug, Default)]
pub struct UndoBook {
    last: u64,
    offers: BTreeMap<UndoId, Offer>,
}

impl UndoBook {
    pub fn offer(&mut self, offer: Offer) -> UndoId {
        self.last += 1;
        let id = UndoId(self.last);
        self.offers.insert(id, offer);
        id
    }

    /// Removes and returns the answer offer for (account, uid, recurrence id), if any.
    pub fn take_answer(
        &mut self,
        account: &str,
        uid: &str,
        rid: &str,
    ) -> Option<(UndoId, Offered)> {
        let (id, _) = self.find_answer(account, uid, rid)?;
        self.offers.remove(&id).map(|offer| (id, offer.what))
    }

    /// The Outbox item of the answer offer for (account, uid, recurrence id),
    /// if one is open.
    pub fn answer_item(&self, account: &str, uid: &str, rid: &str) -> Option<&str> {
        self.find_answer(account, uid, rid).map(|(_, item)| item)
    }

    fn find_answer(&self, account: &str, uid: &str, rid: &str) -> Option<(UndoId, &str)> {
        self.offers
            .iter()
            .find_map(|(id, offer)| match &offer.what {
                Offered::Answer {
                    account: a,
                    uid: u,
                    recurrence_id: r,
                    outbox_item,
                    ..
                } if a == account && u == uid && r == rid => Some((*id, outbox_item.as_str())),
                _ => None,
            })
    }

    /// The offer, if its window is still open. An expired one is dropped
    /// either way: its expiry has been or is being handled by `expire`.
    pub fn take(&mut self, id: UndoId, now_ms: i64) -> Option<Offer> {
        self.offers
            .remove(&id)
            .filter(|offer| now_ms < offer.expires_at_ms)
    }

    /// Removes and returns the offers whose window has ended, oldest first.
    pub fn expire(&mut self, now_ms: i64) -> Vec<(UndoId, Offer)> {
        let ended: Vec<UndoId> = self
            .offers
            .iter()
            .filter(|(_, offer)| offer.expires_at_ms <= now_ms)
            .map(|(id, _)| *id)
            .collect();
        ended
            .into_iter()
            .filter_map(|id| self.offers.remove(&id).map(|offer| (id, offer)))
            .collect()
    }

    pub fn next_expiry(&self) -> Option<i64> {
        self.offers.values().map(|offer| offer.expires_at_ms).min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer(expires_at_ms: i64) -> Offer {
        Offer {
            what: Offered::Actions {
                undos: Vec::new(),
                accounts: BTreeSet::new(),
                folders: BTreeSet::new(),
            },
            expires_at_ms,
        }
    }

    #[test]
    fn an_offer_expires_after_the_window() {
        let mut book = UndoBook::default();
        let id = book.offer(offer(5_000));
        assert!(book.expire(4_999).is_empty());
        let ended = book.expire(5_000);
        assert_eq!(ended.len(), 1);
        assert_eq!(ended[0].0, id);
        assert!(book.expire(9_000).is_empty());
    }

    #[test]
    fn take_after_expiry_is_none() {
        let mut book = UndoBook::default();
        let early = book.offer(offer(5_000));
        let open = book.offer(offer(5_000));
        assert!(book.take(early, 5_000).is_none());
        assert!(book.take(open, 4_999).is_some());
        // Taken once only.
        assert!(book.take(open, 4_999).is_none());
    }

    #[test]
    fn next_expiry_is_the_earliest() {
        let mut book = UndoBook::default();
        assert_eq!(book.next_expiry(), None);
        book.offer(offer(9_000));
        book.offer(offer(7_000));
        assert_eq!(book.next_expiry(), Some(7_000));
        book.expire(7_000);
        assert_eq!(book.next_expiry(), Some(9_000));
    }
}
