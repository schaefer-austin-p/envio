//! Removing an account without leaving anything behind.
//!
//! The removal is several writes, so a marker file lists the accounts being
//! removed: written first, cleared last. An interrupted removal is finished
//! at the next start (`resume_removals`), which is why every step here
//! tolerates having been done already.

use std::fs;
use std::io;

use envio_store::StoreError;
use envio_sync::SyncError;

use super::Worker;
use super::conn::Conn;
use super::store::{Store, secrets_error};
use crate::api::event::Event;
use crate::api::view::UnsentCounts;
use crate::fsutil::write_atomic;
use crate::health::AccountStatus;
use crate::text;

pub(crate) const MARKER: &str = "removing.json";

pub(crate) fn store_error(error: SyncError) -> StoreError {
    match error {
        SyncError::Store(error) => error,
        SyncError::Net(error) => StoreError::Io(io::Error::other(error.to_string())),
    }
}

/// The keys in the marker. A damaged marker is moved aside (like the
/// passwords file) and reported as `InvalidData`; its keys are never
/// guessed at or overwritten.
fn read_marker(store: &Store) -> io::Result<Vec<String>> {
    let path = store.data_dir.join(MARKER);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    serde_json::from_slice(&bytes).map_err(|_| {
        let aside = store.data_dir.join(format!("{MARKER}.damaged"));
        log::error!("{MARKER} is damaged; moving it to {MARKER}.damaged");
        match fs::rename(&path, &aside) {
            Ok(()) => io::Error::new(io::ErrorKind::InvalidData, "damaged removal marker"),
            Err(error) => error,
        }
    })
}

fn write_marker(store: &Store, keys: &[String]) -> io::Result<()> {
    let bytes = serde_json::to_vec(keys).map_err(io::Error::other)?;
    write_atomic(&store.data_dir.join(MARKER), &bytes, 0o600)
}

/// Everything held for the account: settings, then pending work and cache,
/// then the password. Safe to repeat.
fn delete_everything(store: &mut Store, key: &str) -> envio_store::Result<()> {
    match store.accounts.remove(key) {
        Ok(()) | Err(StoreError::NotFound) => {}
        Err(error) => return Err(error),
    }
    let (mut local, drafts) = store.local_and_drafts();
    envio_sync::driver::remove_account(&mut local, drafts, key).map_err(store_error)?;
    // Other accounts' messages share the folder: keep the one on screen.
    let shown = store.open_message.map(|id| id.0.to_string());
    super::inline::clear_except(&store.paths.inline(), shown.as_deref());
    store.secrets.remove(key).map_err(secrets_error)
}

#[cfg(test)]
pub(super) fn remove_for_test(store: &mut Store, key: &str) {
    remove(store, key).unwrap();
}

/// Marks the account as being removed, deletes it, clears the mark.
fn remove(store: &mut Store, key: &str) -> envio_store::Result<()> {
    let mut keys = read_marker(store)?;
    if !keys.iter().any(|k| k == key) {
        keys.push(key.to_owned());
    }
    write_marker(store, &keys)?;
    delete_everything(store, key)?;
    keys.retain(|k| k != key);
    write_marker(store, &keys)?;
    Ok(())
}

/// Finishes the removals a crash cut short; returns their keys.
pub(crate) fn resume_removals(store: &mut Store) -> envio_store::Result<Vec<String>> {
    let keys = match read_marker(store) {
        Ok(keys) => keys,
        // Moved aside; there is nothing known to finish.
        Err(error) if error.kind() == io::ErrorKind::InvalidData => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    for key in &keys {
        delete_everything(store, key)?;
    }
    if !keys.is_empty() {
        write_marker(store, &[])?;
    }
    Ok(keys)
}

impl Worker<'_> {
    pub(crate) fn remove_account(&mut self, account: &str, confirmed: bool) {
        if self.store.accounts.get(account).is_none() {
            log::warn!("removing unknown account {account}");
            return;
        }
        if !confirmed && self.ask_before_removing(account) {
            return;
        }
        let before = self.status_of(account);
        // Before the rows go: waiting opens are answered, not leaked.
        self.open_failed(Some(account));
        self.older_account_removed(account);
        self.book.drop_account(account);
        self.drop_outbox_of(account);
        if let Some(live) = self.live.get_mut(account) {
            // Sessions and the idler go; its IDLE future ends on the
            // notify and its result is dropped with the account.
            match &live.conn {
                Conn::Idling(stop) => stop.notify_one(),
                Conn::Opening(handle) => handle.abort(),
                _ => {}
            }
            live.conn = Conn::Closed;
            live.wants = super::plan::Wants::default();
            live.health.status = AccountStatus::Removing;
        }
        self.report(account, &before);
        match remove(&mut self.store, account) {
            Ok(()) => {
                self.live.remove(account);
                self.unread_sent.borrow_mut().remove(account);
                self.badge();
                self.emit(Event::AccountRemoved {
                    account: account.to_owned(),
                });
            }
            Err(error) => {
                // The marker stays: the next start finishes it.
                log::error!("removing account {account}: {error}");
                self.emit(Event::Notice {
                    text: text::setup::remove_failed(),
                });
            }
        }
    }

    /// Asks the app when unsent work exists; true if the removal waits
    /// (for the answer, or because the work could not be counted).
    fn ask_before_removing(&mut self, account: &str) -> bool {
        let unsent = match self.store.unsent(account) {
            Ok(unsent) => unsent,
            Err(error) => {
                log::error!("counting unsent work of {account}: {error}");
                self.emit(Event::Notice {
                    text: text::setup::remove_failed(),
                });
                return true;
            }
        };
        let counts = UnsentCounts {
            outbox: unsent.outbox,
            drafts: unsent.drafts,
            actions: unsent.actions,
        };
        if counts.outbox + counts.drafts + counts.actions == 0 {
            return false;
        }
        self.emit(Event::ConfirmRemove {
            account: account.to_owned(),
            text: text::setup::confirm_remove(&counts),
            unsent: counts,
        });
        true
    }
}
