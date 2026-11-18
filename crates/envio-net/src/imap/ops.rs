//! The IMAP operations sync uses. Each is one or two commands; arguments
//! are built (and checked) in `query`.

use std::time::Duration;

use futures::TryStreamExt;

use super::client::map_error;
use super::query::{self, Flag, MessageFlags};
use super::untrusted::neutralize;
use super::{FlagUpdate, Folder, FolderState, ImapSession, Summary};
use super::{fetched, names};
use crate::connect;
use crate::error::{NetError, Result};

/// How many of the newest messages the Message-ID fallback reads.
pub const SCAN: u32 = 200;

/// What one message adds to a summary FETCH reply (flags, size, date and
/// header), for the time limit.
const SUMMARY_BYTES: usize = 1024;
/// What one message adds to a flags-only FETCH reply.
const FLAGS_BYTES: usize = 64;

/// The time limit of a FETCH of `count` messages at `per_message` bytes.
fn fetch_timeout(count: usize, per_message: usize) -> Duration {
    connect::transfer_timeout(count.saturating_mul(per_message))
}

impl ImapSession {
    /// Every folder (LIST "" "*").
    pub async fn list_folders(&mut self) -> Result<Vec<Folder>> {
        connect::within(async {
            let names: Vec<_> = self
                .session
                .list(Some(""), Some("*"))
                .await
                .map_err(map_error)?
                .try_collect()
                .await
                .map_err(map_error)?;
            Ok(names
                .iter()
                .map(|name| Folder {
                    path: name.name().to_owned(),
                    name: names::decode_utf7(name.name()).unwrap_or_else(|| name.name().to_owned()),
                    delimiter: name.delimiter().map(str::to_owned),
                    role: names::role_of(name.name(), name.attributes()),
                    selectable: names::selectable(name.attributes()),
                })
                .collect())
        })
        .await
    }

    /// Opens a folder read-write, with CONDSTORE if the server has it.
    pub async fn select(&mut self, path: &str) -> Result<FolderState> {
        let condstore = self.caps.condstore;
        let state = connect::within(async {
            let mailbox = if condstore {
                self.session.select_condstore(path).await
            } else {
                self.session.select(path).await
            }
            .map_err(map_error)?;
            let uid_validity = mailbox
                .uid_validity
                .ok_or_else(|| NetError::Protocol("SELECT without UIDVALIDITY".into()))?;
            Ok(FolderState {
                uid_validity,
                uid_next: mailbox.uid_next.unwrap_or(0),
                exists: mailbox.exists,
                highest_modseq: mailbox.highest_modseq,
            })
        })
        .await?;
        self.exists = state.exists;
        // What the old folder reported no longer matters.
        let _ = self.take_changes();
        Ok(state)
    }

    /// Flags, size, date and header of each message in `uids`.
    pub async fn fetch_summaries(&mut self, uids: &[u32]) -> Result<Vec<Summary>> {
        let Some(set) = query::uid_set(uids) else {
            return Ok(Vec::new());
        };
        let items = query::summary_items(self.caps.condstore);
        connect::within_for(fetch_timeout(uids.len(), SUMMARY_BYTES), async {
            self.checked_fetch(&format!("UID FETCH {set} {items}"), |attributes| {
                Some(Summary {
                    uid: fetched::uid(attributes)?,
                    flags: fetched::flags(attributes),
                    size: fetched::size(attributes).unwrap_or(0),
                    internal_date: fetched::internal_date(attributes),
                    modseq: fetched::modseq(attributes),
                    header: fetched::header(attributes).unwrap_or_default().to_vec(),
                })
            })
            .await
        })
        .await
    }

    /// Flags of `uids`; with `changed_since` (CONDSTORE), only those that
    /// changed after it.
    pub async fn fetch_flags(
        &mut self,
        uids: &[u32],
        changed_since: Option<u64>,
    ) -> Result<Vec<FlagUpdate>> {
        // One command per chunk keeps each line under the server's limit.
        let mut updates = Vec::new();
        for (set, count) in query::uid_set_chunks(uids, query::MAX_SET_LEN) {
            updates.extend(self.fetch_flags_in(&set, count, changed_since).await?);
        }
        Ok(updates)
    }

    /// Flags of every message from `first` up (`first:*`).
    pub async fn fetch_flags_from(
        &mut self,
        first: u32,
        changed_since: Option<u64>,
    ) -> Result<Vec<FlagUpdate>> {
        // `first:*` has no count; the folder's size is the most it can be.
        let most = self.exists as usize;
        self.fetch_flags_in(&format!("{first}:*"), most, changed_since)
            .await
    }

    /// `count` is how many messages `set` may name, for the time limit.
    async fn fetch_flags_in(
        &mut self,
        set: &str,
        count: usize,
        changed_since: Option<u64>,
    ) -> Result<Vec<FlagUpdate>> {
        let items = query::flag_items(changed_since);
        connect::within_for(fetch_timeout(count, FLAGS_BYTES), async {
            self.checked_flag_fetch(&format!("UID FETCH {set} {items}"))
                .await
        })
        .await
    }

    /// The whole message, without setting \Seen. None if it's gone. `size`
    /// is the message's size from its summary: the limit grows with it, as
    /// it does for sending.
    pub async fn fetch_body(&mut self, uid: u32, size: usize) -> Result<Option<Vec<u8>>> {
        connect::within_for(connect::transfer_timeout(size), async {
            // The reply that carries the body (an unsolicited update for the
            // same UID may come first).
            let bodies = self
                .checked_fetch(
                    &format!("UID FETCH {uid} (UID BODY.PEEK[])"),
                    |attributes| {
                        (fetched::uid(attributes) == Some(uid))
                            .then(|| fetched::body(attributes))
                            .flatten()
                            .map(|body| neutralize(body).into_owned())
                    },
                )
                .await?;
            Ok(bodies.into_iter().next())
        })
        .await
    }

    /// Adds (`add`) or removes `flags` on `uids`.
    pub async fn store_flags(&mut self, uids: &[u32], add: bool, flags: &[Flag]) -> Result<()> {
        let Some(set) = query::uid_set(uids) else {
            return Ok(());
        };
        let change = format!(
            "{}FLAGS.SILENT {}",
            if add { '+' } else { '-' },
            query::flag_list(flags)?
        );
        connect::within(async {
            let _: Vec<_> = self
                .session
                .uid_store(&set, &change)
                .await
                .map_err(map_error)?
                .try_collect()
                .await
                .map_err(map_error)?;
            Ok(())
        })
        .await
    }

    /// Moves `uids` from the selected folder to `target`. Without MOVE:
    /// COPY, mark \Deleted, then UID EXPUNGE if the server has UIDPLUS. A
    /// plain EXPUNGE would also remove other messages marked \Deleted, so
    /// without UIDPLUS the originals stay, marked \Deleted.
    pub async fn move_to(&mut self, uids: &[u32], target: &str) -> Result<()> {
        let Some(set) = query::uid_set(uids) else {
            return Ok(());
        };
        if self.caps.r#move {
            return connect::within(async {
                self.session.uid_mv(&set, target).await.map_err(map_error)
            })
            .await;
        }
        connect::within(async { self.session.uid_copy(&set, target).await.map_err(map_error) })
            .await?;
        self.delete(uids).await
    }

    /// Deletes `uids` in the selected folder for good: marks them
    /// \Deleted, then UID EXPUNGE if the server has UIDPLUS. Without
    /// UIDPLUS they stay marked, as in `move_to`.
    pub async fn delete(&mut self, uids: &[u32]) -> Result<()> {
        let Some(set) = query::uid_set(uids) else {
            return Ok(());
        };
        self.store_flags(uids, true, &[Flag::Deleted]).await?;
        if !self.caps.uidplus {
            return Ok(());
        }
        connect::within(self.checked_ok(&format!("UID EXPUNGE {set}"))).await
    }

    /// UIDs of live (not \\Deleted) messages in the selected folder with
    /// this Message-ID (the Outbox's "may have been sent" check). Some servers (Stalwart 1.0) never
    /// match HEADER Message-ID, so an empty answer is checked against the
    /// Message-ID headers of the newest [`SCAN`] messages.
    pub async fn search_message_id(&mut self, message_id: &str) -> Result<Vec<u32>> {
        let key = query::search_message_id(message_id)?;
        let found = self.search(&key).await?;
        if !found.is_empty() || self.exists == 0 {
            return Ok(found);
        }
        let first = self.exists.saturating_sub(SCAN - 1).max(1);
        connect::within(async {
            // A refused scan is an error: "not found" would send again.
            let mut uids: Vec<u32> = self
                .checked_fetch(
                    &format!("FETCH {first}:* (UID FLAGS BODY.PEEK[HEADER.FIELDS (MESSAGE-ID)])"),
                    |attributes| {
                        let live = !fetched::flags(attributes).deleted;
                        let same =
                            query::message_id_of(fetched::header(attributes).unwrap_or_default())
                                .is_some_and(|found| query::same_message_id(&found, message_id));
                        (live && same).then(|| fetched::uid(attributes)).flatten()
                    },
                )
                .await?;
            uids.sort_unstable();
            Ok(uids)
        })
        .await
    }

    /// "Search on server" in the selected folder.
    pub async fn search_text(&mut self, text: &str) -> Result<Vec<u32>> {
        let key = query::search_text(text, self.caps.literals())?;
        self.search(&key).await
    }

    /// UIDs in the selected folder that arrived on or after the day of
    /// `unix_secs`: the messages inside sync's retention window.
    pub async fn search_since(&mut self, unix_secs: i64) -> Result<Vec<u32>> {
        self.search(&query::search_since(unix_secs)).await
    }

    pub(super) async fn search(&mut self, key: &str) -> Result<Vec<u32>> {
        connect::within(self.checked_search(key)).await
    }
}

pub(super) fn message_flags<'a>(
    flags: impl Iterator<Item = async_imap::types::Flag<'a>>,
) -> MessageFlags {
    use async_imap::types::Flag as F;
    MessageFlags::from_flags(flags.filter_map(|flag| match flag {
        F::Seen => Some(Flag::Seen),
        F::Answered => Some(Flag::Answered),
        F::Flagged => Some(Flag::Flagged),
        F::Deleted => Some(Flag::Deleted),
        F::Draft => Some(Flag::Draft),
        F::Custom(word) => Some(Flag::Keyword(word.into_owned())),
        F::Recent | F::MayCreate => None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fetch_gets_time_in_proportion_to_its_size() {
        assert_eq!(fetch_timeout(0, SUMMARY_BYTES), connect::COMMAND_TIMEOUT);
        // 16 summaries are 16 KiB: one extra second.
        assert_eq!(
            fetch_timeout(16, SUMMARY_BYTES),
            connect::COMMAND_TIMEOUT + Duration::from_secs(1)
        );
        assert_eq!(
            fetch_timeout(20_000, SUMMARY_BYTES),
            connect::COMMAND_TIMEOUT + Duration::from_secs(1250)
        );
        assert_eq!(
            fetch_timeout(20_000, FLAGS_BYTES),
            connect::COMMAND_TIMEOUT + Duration::from_secs(78)
        );
        assert!(fetch_timeout(usize::MAX, SUMMARY_BYTES).as_secs() > 1_000_000);
    }
}
