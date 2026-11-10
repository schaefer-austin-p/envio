//! The IMAP operations sync uses. Each is one or two commands; arguments
//! are built (and checked) in `query`.

use futures::TryStreamExt;

use super::client::map_error;
use super::names;
use super::query::{self, Flag, MessageFlags};
use super::{FlagUpdate, Folder, FolderState, ImapSession, Summary};
use crate::connect;
use crate::error::{NetError, Result};

/// How many of the newest messages the Message-ID fallback reads.
pub const SCAN: u32 = 200;

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
        Ok(state)
    }

    /// Flags, size, date and header of each message in `uids`.
    pub async fn fetch_summaries(&mut self, uids: &[u32]) -> Result<Vec<Summary>> {
        let Some(set) = query::uid_set(uids) else {
            return Ok(Vec::new());
        };
        let items = query::summary_items(self.caps.condstore);
        connect::within(async {
            let fetches: Vec<_> = self
                .session
                .uid_fetch(&set, items)
                .await
                .map_err(map_error)?
                .try_collect()
                .await
                .map_err(map_error)?;
            Ok(fetches
                .iter()
                .filter_map(|fetch| {
                    Some(Summary {
                        uid: fetch.uid?,
                        flags: flags_of(fetch),
                        size: fetch.size.unwrap_or(0),
                        internal_date: fetch.internal_date().map(|date| date.timestamp()),
                        modseq: fetch.modseq,
                        header: fetch.header().unwrap_or_default().to_vec(),
                    })
                })
                .collect())
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
        let Some(set) = query::uid_set(uids) else {
            return Ok(Vec::new());
        };
        self.fetch_flags_in(&set, changed_since).await
    }

    /// Flags of every message from `first` up (`first:*`).
    pub async fn fetch_flags_from(
        &mut self,
        first: u32,
        changed_since: Option<u64>,
    ) -> Result<Vec<FlagUpdate>> {
        self.fetch_flags_in(&format!("{first}:*"), changed_since)
            .await
    }

    async fn fetch_flags_in(
        &mut self,
        set: &str,
        changed_since: Option<u64>,
    ) -> Result<Vec<FlagUpdate>> {
        let items = query::flag_items(changed_since);
        connect::within(async {
            let fetches: Vec<_> = self
                .session
                .uid_fetch(set, &items)
                .await
                .map_err(map_error)?
                .try_collect()
                .await
                .map_err(map_error)?;
            Ok(fetches
                .iter()
                .filter_map(|fetch| {
                    Some(FlagUpdate {
                        uid: fetch.uid?,
                        flags: flags_of(fetch),
                        modseq: fetch.modseq,
                    })
                })
                .collect())
        })
        .await
    }

    /// The whole message, without setting \Seen. None if it's gone. `size`
    /// is the message's size from its summary: the limit grows with it, as
    /// it does for sending.
    pub async fn fetch_body(&mut self, uid: u32, size: usize) -> Result<Option<Vec<u8>>> {
        connect::within_for(connect::transfer_timeout(size), async {
            let fetches: Vec<_> = self
                .session
                .uid_fetch(uid.to_string(), "(UID BODY.PEEK[])")
                .await
                .map_err(map_error)?
                .try_collect()
                .await
                .map_err(map_error)?;
            Ok(fetches
                .iter()
                .find(|fetch| fetch.uid == Some(uid))
                .and_then(|fetch| fetch.body())
                .map(<[u8]>::to_vec))
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
        self.store_flags(uids, true, &[Flag::Deleted]).await?;
        if !self.caps.uidplus {
            return Ok(());
        }
        connect::within(async {
            let _: Vec<_> = self
                .session
                .uid_expunge(&set)
                .await
                .map_err(map_error)?
                .try_collect()
                .await
                .map_err(map_error)?;
            Ok(())
        })
        .await
    }

    /// UIDs in the selected folder with this Message-ID (the Outbox's
    /// "may have been sent" check). Some servers (Stalwart 1.0) never
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
            let fetches: Vec<_> = self
                .session
                .fetch(
                    format!("{first}:*"),
                    "(UID BODY.PEEK[HEADER.FIELDS (MESSAGE-ID)])",
                )
                .await
                .map_err(map_error)?
                .try_collect()
                .await
                .map_err(map_error)?;
            let mut uids: Vec<u32> = fetches
                .iter()
                .filter(|fetch| {
                    query::message_id_of(fetch.header().unwrap_or_default())
                        .is_some_and(|found| query::same_message_id(&found, message_id))
                })
                .filter_map(|fetch| fetch.uid)
                .collect();
            uids.sort_unstable();
            Ok(uids)
        })
        .await
    }

    /// "Search on server" in the selected folder.
    pub async fn search_text(&mut self, text: &str) -> Result<Vec<u32>> {
        let key = query::search_text(text, self.caps.literal_plus)?;
        self.search(&key).await
    }

    async fn search(&mut self, key: &str) -> Result<Vec<u32>> {
        connect::within(async {
            let found = self.session.uid_search(key).await.map_err(map_error)?;
            let mut uids: Vec<u32> = found.into_iter().collect();
            uids.sort_unstable();
            Ok(uids)
        })
        .await
    }
}

fn flags_of(fetch: &async_imap::types::Fetch) -> MessageFlags {
    use async_imap::types::Flag as F;
    MessageFlags::from_flags(fetch.flags().filter_map(|flag| match flag {
        F::Seen => Some(Flag::Seen),
        F::Answered => Some(Flag::Answered),
        F::Flagged => Some(Flag::Flagged),
        F::Deleted => Some(Flag::Deleted),
        F::Draft => Some(Flag::Draft),
        F::Custom(word) => Some(Flag::Keyword(word.into_owned())),
        F::Recent | F::MayCreate => None,
    }))
}
