//! Commands whose tagged status must be read. async-imap's FETCH stream
//! ends with Ok when the server answers with a tagged NO or BAD (it drops
//! the tagged reply), so a refused command looks like a short result.
//! These read the replies themselves and map the status like every other
//! command (`command_end`).

use async_imap::imap_proto::{AttributeValue, MailboxDatum, Response};

use super::client::{command_end, map_error};
use super::fetched::{self, Attributes};
use super::{FlagUpdate, ImapSession};
use crate::error::{NetError, Result};

impl ImapSession {
    /// Runs a UID SEARCH and returns the UIDs found, sorted, or the error of
    /// its tagged NO or BAD.
    pub(super) async fn checked_search(&mut self, key: &str) -> Result<Vec<u32>> {
        let id = self
            .session
            .run_command(format!("UID SEARCH {key}"))
            .await
            .map_err(map_error)?;
        let mut found = Vec::new();
        loop {
            let response = reply(self.session.read_response().await)?;
            match response.parsed() {
                Response::MailboxData(MailboxDatum::Search(uids)) => found.extend(uids),
                Response::Fetch(..)
                | Response::Expunge(_)
                | Response::Vanished { .. }
                | Response::MailboxData(MailboxDatum::Exists(_)) => self.missed_changes = true,
                other => {
                    if let Some(end) = command_end(other, &id) {
                        end?;
                        found.sort_unstable();
                        found.dedup();
                        return Ok(found);
                    }
                }
            }
        }
    }

    /// Runs a FETCH (or UID FETCH) and returns what `map` makes of each
    /// reply that has a UID (`None`: not one it wants), or the error of the
    /// tagged NO or BAD. A reply without a UID is an unsolicited change.
    pub(super) async fn checked_fetch<T>(
        &mut self,
        command: &str,
        mut map: impl FnMut(Attributes<'_>) -> Option<T>,
    ) -> Result<Vec<T>> {
        let id = self.session.run_command(command).await.map_err(map_error)?;
        let mut out = Vec::new();
        loop {
            let response = reply(self.session.read_response().await)?;
            match response.parsed() {
                Response::Fetch(_, attributes) => {
                    if fetched::uid(attributes).is_none() {
                        self.missed_changes = true;
                    } else if let Some(item) = map(attributes) {
                        out.push(item);
                    }
                }
                Response::Expunge(_)
                | Response::Vanished { .. }
                | Response::MailboxData(MailboxDatum::Exists(_)) => self.missed_changes = true,
                other => {
                    if let Some(end) = command_end(other, &id) {
                        return end.map(|()| out);
                    }
                }
            }
        }
    }

    /// Runs a command whose replies carry no data (UID STORE, UID EXPUNGE)
    /// and returns the error of its tagged NO or BAD. Unsolicited changes
    /// are tracked; `own_expunge` says EXPUNGE lines are the command's own
    /// result (UID EXPUNGE), not a change.
    pub(super) async fn checked_ok(&mut self, command: &str, own_expunge: bool) -> Result<()> {
        let id = self.session.run_command(command).await.map_err(map_error)?;
        loop {
            let response = reply(self.session.read_response().await)?;
            match response.parsed() {
                Response::Expunge(_) if own_expunge => {}
                Response::Fetch(..)
                | Response::Expunge(_)
                | Response::Vanished { .. }
                | Response::MailboxData(MailboxDatum::Exists(_)) => self.missed_changes = true,
                other => {
                    if let Some(end) = command_end(other, &id) {
                        return end;
                    }
                }
            }
        }
    }

    /// Runs a UID FETCH for flags and returns its updates, or the error of
    /// its tagged NO or BAD. A reply without a UID counts as a change.
    pub(super) async fn checked_flag_fetch(&mut self, command: &str) -> Result<Vec<FlagUpdate>> {
        let id = self.session.run_command(command).await.map_err(map_error)?;
        let mut updates = Vec::new();
        loop {
            let response = reply(self.session.read_response().await)?;
            match response.parsed() {
                Response::Fetch(_, attributes) => match flag_update(attributes) {
                    Some(update) => updates.push(update),
                    // No UID: an unsolicited flag change, which the
                    // channel path would have counted.
                    None => self.missed_changes = true,
                },
                Response::Expunge(_)
                | Response::Vanished { .. }
                | Response::MailboxData(MailboxDatum::Exists(_)) => self.missed_changes = true,
                other => {
                    if let Some(end) = command_end(other, &id) {
                        return end.map(|()| updates);
                    }
                }
            }
        }
    }
}

fn flag_update(attributes: &[AttributeValue<'_>]) -> Option<FlagUpdate> {
    Some(FlagUpdate {
        uid: fetched::uid(attributes)?,
        flags: fetched::flags(attributes),
        modseq: fetched::modseq(attributes),
    })
}

/// The next reply, or why there is none (the connection ended).
fn reply<T>(read: std::io::Result<Option<T>>) -> Result<T> {
    match read {
        Ok(Some(response)) => Ok(response),
        Ok(None) => Err(NetError::Io(std::io::ErrorKind::UnexpectedEof.into())),
        Err(error) => Err(NetError::Io(error)),
    }
}
