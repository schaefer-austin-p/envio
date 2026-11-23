//! The `replaces_id` per alert tag. Notify's reply (the card's id) comes
//! back later on the main thread, so a tag is either settled (`Idle`, the
//! last id) or waiting for a reply (`InFlight`, with the newest alert that
//! came meanwhile). A burst leaves one card per tag.

use std::collections::HashMap;

use envio_core::Alert;

enum Slot {
    /// Last known card id; 0 when none.
    Idle(u32),
    InFlight {
        /// The account of the alert being sent.
        account: String,
        /// What the call in flight replaces, kept if it fails.
        replaces: u32,
        /// The newest alert that came meanwhile.
        queued: Option<Alert>,
    },
}

#[derive(Default)]
pub(crate) struct Ids {
    slots: HashMap<String, Slot>,
    /// Card id -> (tag, account), for clicks.
    cards: HashMap<u32, (String, String)>,
    /// Counts the server owners seen: a call carries the number it was
    /// made under, and its reply counts only under the same one.
    epoch: u32,
}

/// An alert to send now, replacing the card with this id (0: a new card),
/// and the epoch the reply must name to count.
pub(crate) type ToSend = (Alert, u32, u32);

impl Ids {
    /// The worker has an alert: `Some` to send now, `None` when it waits for
    /// the in-flight call of its tag.
    pub(crate) fn begin(&mut self, alert: Alert) -> Option<ToSend> {
        if let Some(Slot::InFlight { queued, .. }) = self.slots.get_mut(&alert.tag) {
            *queued = Some(alert);
            return None;
        }
        let replaces = match self.slots.get(&alert.tag) {
            Some(Slot::Idle(id)) => *id,
            _ => 0,
        };
        self.slots.insert(
            alert.tag.clone(),
            Slot::InFlight {
                account: alert.account.clone(),
                replaces,
                queued: None,
            },
        );
        Some((alert, replaces, self.epoch))
    }

    /// Notify's reply for `tag`: `id` is the card's id, 0 when the call
    /// failed; `epoch` is the one the call was sent under, and a reply from
    /// before a `reset` is ignored. Returns the alert that waited, to send
    /// now.
    pub(crate) fn finished(&mut self, tag: &str, id: u32, epoch: u32) -> Option<ToSend> {
        if epoch != self.epoch {
            return None;
        }
        let Some(Slot::InFlight {
            account,
            replaces,
            queued,
        }) = self.slots.remove(tag)
        else {
            return None;
        };
        // A failed call keeps the earlier card's id (it may still be shown).
        let id = if id == 0 { replaces } else { id };
        if id != 0 {
            self.cards.retain(|_, (t, _)| t != tag);
            self.cards.insert(id, (tag.to_owned(), account));
        }
        match queued {
            Some(next) => {
                self.slots.insert(
                    tag.to_owned(),
                    Slot::InFlight {
                        account: next.account.clone(),
                        replaces: id,
                        queued: None,
                    },
                );
                Some((next, id, self.epoch))
            }
            None => {
                self.slots.insert(tag.to_owned(), Slot::Idle(id));
                None
            }
        }
    }

    /// The server closed card `id` (dismissed, expired, replaced): it is not
    /// ours any more, and servers may reuse ids.
    pub(crate) fn closed(&mut self, id: u32) {
        let Some((tag, _)) = self.cards.remove(&id) else {
            return;
        };
        match self.slots.get_mut(&tag) {
            Some(Slot::Idle(last) | Slot::InFlight { replaces: last, .. }) if *last == id => {
                *last = 0;
            }
            _ => {}
        }
    }

    /// The notification server was replaced or went away: every id is the
    /// old server's, and the new one may hand them to other apps' cards.
    /// Forget all and start a new epoch, so replies still in flight are
    /// ignored. An alert that waited for such a reply is not lost: it comes
    /// back to be sent now as a new card.
    pub(crate) fn reset(&mut self) -> Vec<ToSend> {
        self.epoch = self.epoch.wrapping_add(1);
        let old = std::mem::take(&mut self.slots);
        self.cards.clear();
        let mut again = Vec::new();
        for (tag, slot) in old {
            if let Slot::InFlight {
                queued: Some(alert),
                ..
            } = slot
            {
                self.slots.insert(
                    tag,
                    Slot::InFlight {
                        account: alert.account.clone(),
                        replaces: 0,
                        queued: None,
                    },
                );
                again.push((alert, 0, self.epoch));
            }
        }
        again
    }

    #[cfg(test)]
    pub(crate) fn epoch(&self) -> u32 {
        self.epoch
    }

    /// The account a card with this id belongs to, if Envio posted it.
    pub(crate) fn account_of(&self, id: u32) -> Option<&str> {
        self.cards.get(&id).map(|(_, account)| account.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alert(tag: &str, body: &str) -> Alert {
        Alert {
            tag: tag.into(),
            summary: "s".into(),
            body: body.into(),
            account: format!("acc-{tag}"),
        }
    }

    #[test]
    fn the_first_alert_of_a_tag_makes_a_new_card() {
        let mut ids = Ids::default();
        let (a, replaces, _) = ids.begin(alert("t", "1")).unwrap();
        assert_eq!((a.body.as_str(), replaces), ("1", 0));
    }

    #[test]
    fn the_next_alert_replaces_the_card_the_reply_named() {
        let mut ids = Ids::default();
        ids.begin(alert("t", "1")).unwrap();
        assert!(ids.finished("t", 7, 0).is_none());
        let (_, replaces, _) = ids.begin(alert("t", "2")).unwrap();
        assert_eq!(replaces, 7);
    }

    #[test]
    fn tags_do_not_share_ids() {
        let mut ids = Ids::default();
        ids.begin(alert("a", "1")).unwrap();
        ids.finished("a", 7, 0);
        let (_, replaces, _) = ids.begin(alert("b", "1")).unwrap();
        assert_eq!(replaces, 0);
    }

    #[test]
    fn a_burst_sends_the_newest_waiting_alert_once_the_reply_is_in() {
        let mut ids = Ids::default();
        ids.begin(alert("t", "1")).unwrap();
        assert!(ids.begin(alert("t", "2")).is_none());
        assert!(ids.begin(alert("t", "3")).is_none());
        let (next, replaces, _) = ids.finished("t", 9, 0).unwrap();
        assert_eq!((next.body.as_str(), replaces), ("3", 9));
        // That send is in flight now: another one waits again.
        assert!(ids.begin(alert("t", "4")).is_none());
        let (next, replaces, _) = ids.finished("t", 9, 0).unwrap();
        assert_eq!((next.body.as_str(), replaces), ("4", 9));
        assert!(ids.finished("t", 9, 0).is_none());
    }

    #[test]
    fn a_failed_call_keeps_the_earlier_id() {
        let mut ids = Ids::default();
        ids.begin(alert("t", "1")).unwrap();
        ids.finished("t", 7, 0);
        ids.begin(alert("t", "2")).unwrap();
        ids.finished("t", 0, 0);
        let (_, replaces, _) = ids.begin(alert("t", "3")).unwrap();
        assert_eq!(replaces, 7);
    }

    #[test]
    fn a_failed_first_call_leaves_no_id() {
        let mut ids = Ids::default();
        ids.begin(alert("t", "1")).unwrap();
        ids.finished("t", 0, 0);
        let (_, replaces, _) = ids.begin(alert("t", "2")).unwrap();
        assert_eq!(replaces, 0);
        assert_eq!(ids.account_of(0), None);
    }

    #[test]
    fn a_click_finds_the_account_of_envios_own_cards_only() {
        let mut ids = Ids::default();
        ids.begin(alert("t", "1")).unwrap();
        ids.finished("t", 7, 0);
        assert_eq!(ids.account_of(7), Some("acc-t"));
        assert_eq!(ids.account_of(8), None);
        // A new id for the tag forgets the old one.
        ids.begin(alert("t", "2")).unwrap();
        ids.finished("t", 8, 0);
        assert_eq!(ids.account_of(7), None);
        assert_eq!(ids.account_of(8), Some("acc-t"));
    }

    #[test]
    fn a_closed_card_is_forgotten_and_not_replaced() {
        let mut ids = Ids::default();
        ids.begin(alert("t", "1")).unwrap();
        ids.finished("t", 7, 0);
        ids.closed(7);
        assert_eq!(ids.account_of(7), None);
        let (_, replaces, _) = ids.begin(alert("t", "2")).unwrap();
        assert_eq!(replaces, 0);
        // An id that was never ours changes nothing.
        ids.closed(99);
    }

    #[test]
    fn a_close_during_a_call_is_not_undone_by_its_failure() {
        let mut ids = Ids::default();
        ids.begin(alert("t", "1")).unwrap();
        ids.finished("t", 7, 0);
        ids.begin(alert("t", "2")).unwrap();
        ids.closed(7);
        ids.finished("t", 0, 0);
        let (_, replaces, _) = ids.begin(alert("t", "3")).unwrap();
        assert_eq!(replaces, 0);
    }

    #[test]
    fn a_new_server_owner_forgets_every_id_and_drops_replies_in_flight() {
        let mut ids = Ids::default();
        ids.begin(alert("a", "1")).unwrap();
        ids.finished("a", 7, 0);
        ids.begin(alert("b", "1")).unwrap();
        assert!(ids.reset().is_empty());
        assert_eq!(ids.account_of(7), None);
        // The call in flight for "b" answers late: nothing is stored.
        assert!(ids.finished("b", 9, 0).is_none());
        assert_eq!(ids.account_of(9), None);
        let (_, replaces, _) = ids.begin(alert("a", "2")).unwrap();
        assert_eq!(replaces, 0);
        let (_, replaces, _) = ids.begin(alert("b", "2")).unwrap();
        assert_eq!(replaces, 0);
    }

    #[test]
    fn a_late_reply_from_the_old_server_cannot_settle_a_newer_call() {
        let mut ids = Ids::default();
        let (_, _, old) = ids.begin(alert("t", "1")).unwrap();
        ids.reset();
        let (_, _, new) = ids.begin(alert("t", "2")).unwrap();
        assert_ne!(old, new);
        // The old call answers while the new one is in flight.
        assert!(ids.finished("t", 9, old).is_none());
        assert_eq!(ids.account_of(9), None);
        // The new call is still waiting: a third alert queues behind it.
        assert!(ids.begin(alert("t", "3")).is_none());
        let (next, replaces, _) = ids.finished("t", 5, new).unwrap();
        assert_eq!((next.body.as_str(), replaces), ("3", 5));
    }

    #[test]
    fn an_alert_that_waited_survives_a_reset_as_a_new_card() {
        let mut ids = Ids::default();
        ids.begin(alert("t", "1")).unwrap();
        assert!(ids.begin(alert("t", "2")).is_none());
        let again = ids.reset();
        assert_eq!(again.len(), 1);
        let (sent, replaces, epoch) = &again[0];
        assert_eq!((sent.body.as_str(), *replaces), ("2", 0));
        // It is the call in flight now, under the new epoch.
        assert!(ids.begin(alert("t", "3")).is_none());
        let (next, _, _) = ids.finished("t", 4, *epoch).unwrap();
        assert_eq!(next.body, "3");
    }
}
