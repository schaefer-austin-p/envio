//! Deleting the Drafts list's pseudo-rows (#228, Decision 11): the drafts
//! are held, hidden but with their files, for the Undo window. The files go
//! when it ends. Nothing here deletes sooner, and the hold lives in memory
//! only, so a crash in the window loses nothing.

use std::collections::BTreeSet;

use envio_store::MessageRowId;

use super::Worker;
use crate::api::command::UserAction;
use crate::api::event::Event;
use crate::undo::Offered;

/// The offer for an Act that held `local` drafts and did `actions` on
/// server rows: one offer for both.
pub(super) fn offered_with(local: Vec<String>, actions: Offered) -> Offered {
    if local.is_empty() {
        return actions;
    }
    let nothing_else = matches!(
        &actions,
        Offered::Actions { undos, accounts, .. } if undos.is_empty() && accounts.is_empty()
    );
    Offered::LocalDrafts {
        ids: local,
        rest: (!nothing_else).then(|| Box::new(actions)),
    }
}

impl Worker<'_> {
    /// Holds the drafts behind the negative `ids` when the action is a
    /// Delete, and tells the list. Other actions skip those ids (the UI
    /// does not offer them). Returns the held draft ids.
    pub(super) fn hold_local(&mut self, ids: &[MessageRowId], action: UserAction) -> Vec<String> {
        if action != UserAction::Delete {
            return Vec::new();
        }
        let mut held: Vec<String> = Vec::new();
        for id in ids.iter().filter(|id| id.0 < 0) {
            let Some(draft) = self.local_ids.borrow().draft_of(*id).map(str::to_owned) else {
                continue;
            };
            if held.contains(&draft) {
                continue;
            }
            // Held by an earlier Delete: that offer owns it, no second one.
            if self.local_ids.borrow().is_held(&draft) {
                continue;
            }
            // A compose that is closing still counts: its last save may be queued.
            if self.book.holder(&draft).is_some() {
                log::info!("draft {draft}: a compose holds it, not deleting it");
                continue;
            }
            if self.store.drafts.load(&draft).is_err() {
                continue;
            }
            self.local_ids.borrow_mut().hold(&draft);
            held.push(draft);
        }
        let accounts = self.accounts_of(&held);
        self.tell(accounts);
        held
    }

    /// The window ended: the held drafts and their files go, except one
    /// that is no longer held (an Undo restored it) and one a compose holds
    /// by now. A failed delete keeps the draft (its row returns).
    pub(super) fn delete_local(&mut self, ids: &[String]) {
        let accounts = self.accounts_of(ids);
        for id in ids {
            if !self.local_ids.borrow().is_held(id) {
                log::info!("draft {id}: restored meanwhile, not deleting it");
            } else if self.book.holder(id).is_some() {
                log::info!("draft {id}: a compose holds it, not deleting it");
            } else if let Err(e) = self.store.drafts.delete(id) {
                log::warn!("deleting draft {id}: {e}");
            }
        }
        for id in ids {
            self.local_ids.borrow_mut().release(id);
        }
        self.tell(accounts);
    }

    /// Undo: puts the held drafts back. Returns whether any was, and what
    /// is left of the offer for the server-side half.
    pub(super) fn restore_local(&mut self, what: Offered) -> (bool, Offered) {
        let Offered::LocalDrafts { ids, rest } = what else {
            return (false, what);
        };
        for id in &ids {
            self.local_ids.borrow_mut().release(id);
        }
        let accounts = self.accounts_of(&ids);
        self.tell(accounts);
        let rest = rest.map_or(
            Offered::LocalDrafts {
                ids: Vec::new(),
                rest: None,
            },
            |rest| *rest,
        );
        (!ids.is_empty(), rest)
    }

    /// The accounts the drafts belong to, read while their files exist.
    fn accounts_of(&self, ids: &[String]) -> BTreeSet<String> {
        ids.iter()
            .filter_map(|id| self.store.drafts.load(id).ok())
            .map(|draft| draft.account)
            .collect()
    }

    /// Tells each account that its Drafts list moved.
    fn tell(&mut self, accounts: BTreeSet<String>) {
        for account in accounts {
            self.emit(Event::LocalDraftsChanged { account });
        }
    }
}

#[cfg(test)]
mod tests;
